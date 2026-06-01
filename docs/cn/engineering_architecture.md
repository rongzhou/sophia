# Sophia 工程架构

> 本文档定义 Sophia 的工具链组装、工程目录结构、依赖分层、CLI 规范和外围基础设施。
> 语言概念和工作流概念见 `language_design.md`。
> 编译器内部实现（AST、IR、类型系统）见 `language_implementation.md`。

---

## 一、整体定位

Sophia 被设计为面向 AI-native 与 Agent-native 系统的语义执行平台（Semantic Execution Platform）。架构关注：

- 语义稳定性（Semantic Stability）
- 执行图建模（Execution Graph Modeling）
- 异步编排（Async Orchestration）
- 增量分析（Incremental Analysis）
- 运行时可观测性（Runtime Introspection）
- 长期可演化性（Long-term Evolvability）

工程上长期最核心的资产是 **Semantic IR、Execution Graph IR、Runtime Semantics**，而不是 parser 或 backend code generation。工程架构的所有取舍优先服务这一核心。

---

## 二、核心技术栈

| 层级              | 技术方案              |
| ----------------- | --------------------- |
| 实现语言          | Rust                  |
| Parser            | Tree-sitter           |
| Async Runtime     | Tokio                 |
| LSP               | tower-lsp             |
| 序列化            | Serde + MessagePack   |
| Snapshot Testing  | insta                 |
| Diagnostics       | miette                |
| CLI               | clap                  |
| Tracing           | tracing               |
| Stable IDs        | slotmap               |
| Arena Allocation  | typed-arena / bumpalo |
| Graph 持久化      | SQLite (rusqlite/sqlx)|
| Prompt 模板       | minijinja             |
| JSON Schema 验证  | jsonschema            |
| Markdown 解析     | pulldown-cmark        |

---

## 三、Crate 划分

按依赖方向严格分层：

```
sophia/
├── core/               ← 纯语言语义，零外部 IO 依赖
│   ├── syntax/         ← Tree-sitter binding + CST → AST 转换
│   ├── hir/            ← HIR + 名称解析 + 模块解析
│   ├── semantic/       ← Semantic IR（type / effect / contract 三层）
│   ├── exec-ir/        ← Execution Graph IR
│   └── library/        ← 库契约类型（sophia-library）：清单解析 + LibraryRegistry（无 Value，零 IO）
│
├── workflow/           ← 启发式工作流引擎，依赖 core
│   ├── graph-db/       ← Development Graph 持久化（SQLite + 事件溯源）
│   ├── llm/            ← LLM client 抽象 + 结构化输出
│   ├── prompt/         ← Prompt 模板管理（核心语法基线；不持库内容）
│   └── engine/         ← 工作流编排（snapshot→LLM→emit；loop/scheduler/select-materialize）
│
├── tools/              ← 确定性工具链，依赖 core
│   ├── check/          ← checker + diagnostics
│   ├── audit/          ← constraint audit
│   ├── materialize/    ← gate 逻辑 + 文件写入
│   └── codegen/        ← WASM codegen（工作流 A）
│
├── stdlib/             ← 标准库内容（sophia-stdlib）：libs/<lib>/ 清单 + 资产 + native host；依赖 core+runtime
├── lsp/                ← Language Server，依赖 core + tools
├── cli/                ← 入口，依赖所有上层 crate
└── runtime/            ← 解释器 + Tokio substrate + HostRegistry（路线 B host 注册表）
```

### 3.1 核心约束

`core` 下的所有 crate **不得依赖任何 IO**，**不得依赖 `workflow`**。这个隔离保证了：

- 编译器的可测试性（无需构造异步测试运行时）；
- 编译器的可推理性（无竞态条件）；
- 未来编译到 WASM 的可能性（WASM 的 async 支持有限）。

### 3.2 异步边界

**同步**（不引入 Tokio）：

- 全部 `core`
- `tools`（check / audit 的核心分析逻辑）

**异步**（使用 Tokio）：

- `llm`（LLM API 网络请求）
- `graph-db`（SQLite 操作）
- `materialize`（文件写入）
- `cli`（协调层）
- `runtime`（execution graph 调度）

### 3.3 编排层 `workflow/engine` 与「注入报告」分层模式

实现中浮现出一个设计文档原先未明确的分层细节，固化如下：

- **`workflow/engine` 是工作流编排协调者**：把「确定性建 ContextSnapshot」与「非确定 LLM
  调用」的边界固化为单一代码路径（`run_llm_step`），并在其上叠加 design/implement/repair/decompose
  单步（`loop_steps`）、implement-loop 预算闭环（`implement_loop`）、goal 推进调度 spine（`scheduler`）、
  目标树遍历层（`traversal`，在 spine 之上驱动 decompose 子树 / backtrack 分支）、Selection/Materialize
  编排（`select_materialize`）。它依赖 `graph-db` + `llm` + `prompt`，并**可
  依赖 `tools`**（如 `materialize` 的类型状态链）。`engine` 是唯一允许同时依赖 `workflow` 与
  `tools` 的 crate；`graph-db` / `llm` / `prompt` 之间不互相依赖（持久化层不反向依赖 LLM）。

- **「注入报告」模式（tools 不依赖 workflow 图）**：`tools/*`（check / audit / materialize）是
  确定性分析器，**不依赖 Development Graph**，也不自行决定 gate 通过与否的图后果。它们只产出结构化
  报告（`CheckReport` / `AuditReport` / `GateReport`、`VerifierOutcome`），由编排层（engine 或 CLI）
  消费报告、emit `DiagnosticNode` 并连 `checks→` 边。对称地，需要执行 check 的编排件（如
  implement-loop）通过**注入的回调**（`CodeChecker`）拿到确定性结果，而不是自己 `use` 一个
  checker——保证「编排层不自行运行 checker」「tools 层不感知图」两个方向都成立。这一模式使
  `core` / `tools` 保持纯确定性、易测试，把所有非确定与图副作用收敛到 `workflow` / `cli`。

- **类型状态证明不可跨进程**：Materialize Gate 的 `CodeCandidate<Selected>` 是编译期 gate 通过
  证明，但它**无法序列化跨进程持久化**。因此分属不同进程的 `graph select` 与 `graph materialize`
  必须各自从落盘候选**重跑全部 gate** 重新构造该证明——这对「唯一不可逆写盘」反而是更稳妥的姿态
  （见 §九 9.2 与 `language_design.md` 10.10）。

---

## 四、内置 effect 与标准库

### 4.1 内置 effect 族 vs 库 effect 族

唯一的**内置** effect 族是 `Console`（输出原语，`print` 触发），由 `core/hir` 的
`builtins::BUILTIN_EFFECT_OPS`（`(family, op, 参数个数)`）承载——这是「机制 vs 能力族」边界里的例外
（语言内置输出原语）。文件 / 网络等 I/O 的 effect 族由**库**提供（`File` / `Http`，见 `stdlib_design.md`），
**不在** `BUILTIN_EFFECT_OPS`：它们由库清单（`library.toml`）声明，经 `LibraryRegistry` 注入
`AsgIndex`（`with_libraries`）——**核心不硬编码任何具体库**。

```
Console.Write          ← 标准输出（print 触发，语言内置，BUILTIN_EFFECT_OPS）
File.Read / File.Write ← 本地文件读 / 写（库 File，清单驱动，见 file_lib.md）
Http.Get               ← 网络 GET（库 Http，清单驱动，见 http_lib.md）
```

用户可用 `effect` 顶层构造声明领域 effect 族，名称解析时与内置 / 库 effect 并入同一符号表（见
`language_design.md` 第十三节）。

> **历史变更（2026-05-31）**：① 曾有 `DB.Read/Write` 内置 effect + `storage` 顶层节点（语义不清的内存
> KV），已移除；② `File`/`Http` 曾在 `BUILTIN_EFFECT_OPS` 硬编码，**库插件重构后迁出**——改由清单驱动
> 的 `LibraryRegistry` 承载（见 `stdlib_design.md` §二）。

### 4.2 库 = 清单驱动插件；标准库是 crate

库（标准库 + 三方库）统一为**清单驱动插件**：一个库 = 一个目录 + `library.toml`，各层从 `LibraryRegistry`
（清单构建、冻结）派生，不硬编码。**标准库**是 `sophia-stdlib` crate（`libs/<lib>/` 编译进二进制 + native
host）；**契约类型** `sophia-library` crate（`core/*` 可依赖，无 `Value`）；**host 注册表** `HostRegistry`
（路线 B，`(family,op) → Box<dyn HostFn>`）落 `sophia-runtime`。`core` 经只读 `&AsgIndex`（携库契约）/
`&LibraryRegistry` 消费、**不依赖 `sophia-stdlib`**。详见 `stdlib_design.md` / `stdlib_implementation.md`。

---

## 五、Sophia 项目目录结构

用户使用 Sophia 创建的项目遵循 **domain-first 文件布局**：

```
my_project/
├── sophia.toml
├── sophia.lock
├── domains/
│   └── TodoDomain/
│       ├── domain.sophia
│       ├── entities/
│       │   └── Todo.sophia
│       ├── states/
│       │   └── TodoStatus.sophia
│       ├── errors/
│       │   └── TodoError.sophia
│       ├── capabilities/
│       │   └── TodoCapability.sophia
│       ├── transitions/
│       │   └── CompleteTodoTransition.sophia
│       ├── actions/
│       │   └── CompleteTodo.sophia
│       └── tasks/
│           └── ImplementCompleteTodo.sophia
└── sophia-runs/
    ├── generated/
    ├── asg_index.json
    ├── task_closures/
    ├── build/
    └── graph/                  ← Development Graph 事件流（SQLite）
```

### 5.1 文件布局约束

- **一个文件只能定义一个顶层 formal node**；
- node 文件必须放在所属 domain 目录内；
- 文件布局使用 PascalCase domain 目录、PascalCase entity/action/capability 文件名、PascalCase 节点名；
- domain 定义文件固定为 `domain.sophia`；
- 顶层 node 之间的关系必须通过显式引用形成 ASG 边，不能通过文件嵌套或隐式 owner 推断；
- **Entity 不是最高级容器**；domain 是聚合边界，ASG node 是语义单位；
- 禁止隐式 import；
- 禁止同名 shadowing；
- 跨 domain 引用必须通过 boundary 或 task include 显式声明；
- `asg_index.json` 是可重建缓存，不是语义源。

### 5.2 sophia.toml

最小配置：

```toml
[project]
name = "mini_todo"
version = "0.1.0"
sophia_version = "0.1"

[source]
domain_root = "domains"
generated_dir = "sophia-runs/generated"

[layout]
strategy = "domain_first"
one_top_level_node_per_file = true
forbid_global_kind_dirs = true

[build]
target = "interpreter"
out_dir = "sophia-runs/build"

[check]
require_strip_assist_equivalence = true
forbid_implicit_imports = true
forbid_shadowing = true
require_explicit_cross_domain_boundary = true
```

---

## 六、Development Graph 持久化

### 6.1 选择 SQLite + 事件溯源

Development Graph 持久化采用基于事件溯源的 SQLite 方案。

不采用纯 JSON 文件方案的原因：

- 祖先链查询（DecisionNode 需要）、预算统计、评分排序需要复杂查询，JSON 上实现繁琐；
- 节点数增长（`max_total_nodes_per_goal = 40`，多个 goal 并行时快速超过几百节点）后全量读取性能下降；
- 多进程并发写入（CLI 命令和 LSP 同时操作）需要锁机制，SQLite 原生支持。

使用 `rusqlite` 或 `sqlx`，单文件，零配置，适合本地开发。

### 6.2 事件溯源模型

Development Graph 的 append-only、节点不可变语义天然对应事件溯源：

```rust
enum GraphEvent {
    NodeCreated   { id: NodeId, kind: NodeKind, payload: Bytes },
    EdgeAdded     { from: NodeId, to: NodeId, kind: EdgeKind },
    StatusChanged { id: NodeId, status: NodeStatus },
    SelectionMade { node: NodeId, reason: String },
    GateAttempted { node: NodeId, gate: GateKind, result: GateResult },
}
```

每个事件 append-only 写入 SQLite 的 `graph_events` 表。当前图状态由事件流 replay 得出，或维护一个 materialized view 表加速查询。

### 6.3 GraphStore 接口约束

GraphStore 必须实现 `workflow_graph_spec.md` 第三节 列出的"工程层后果"：

- `update_node` 不暴露 payload 写权限；
- `append_edge` 在写入前校验 `(from.role, to.role, type)`；
- `append_node` 在写入前校验 `(role, provenance)` 与 `creation_status` 约束；
- `supersedes` 校验链不成环、两端 role 相同；
- LLM-provenance 节点必须配套 `consumed→ ContextSnapshot` 边。

---

## 七、LLM 结构化输出层

### 7.1 模型无关的 LLM 抽象

工作流引擎多处要求 LLM 输出 JSON（DecisionNode、repair 结果等）：

```rust
#[async_trait]
trait LlmClient {
    async fn complete(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionResponse>;

    async fn complete_structured<T: DeserializeOwned>(
        &self,
        req: CompletionRequest,
        schema: &JsonSchema,
    ) -> Result<T>;
}
```

`complete_structured` 是关键接口。

### 7.2 本地模型的 fallback 机制

支持本地模型（如 Ollama 上的 qwen3 等）。本地模型无法使用原生 Structured Outputs API，需要在这一层实现**重试 + schema 验证的 fallback 机制**：

1. 请求 LLM 输出 JSON，在 prompt 中嵌入 schema 描述；
2. 解析响应，用 `jsonschema` crate 验证；
3. 验证失败时携带错误信息重试，最多 N 次；
4. 超过重试次数时返回结构化错误，**不伪造成功结果**。

Rust 没有对等 Python `instructor` 的库，这部分需要在 `llm` crate 中自行实现。

### 7.3 失败的兜底节点

LLM 调用失败必须 emit `RawLlmNode`（schema 见 `workflow_graph_spec.md` 4.4.8 节），通过 `attempted→` 边指向意图执行的目标节点，便于审计每个目标域的失败次数。

如果 LLM 后端（远端 API 或 Ollama 等本地服务）不可用，依赖 LLM 的命令必须显式失败并保留 `RawLlmNode` 产物，不得伪造成功 CodeNode。

---

## 八、Prompt 模板管理

Prompt 是 Sophia 工作流引擎的**核心资产**，与语言代码同等重要，需要纳入版本控制和测试体系。

### 8.1 目录结构

```
prompt/
├── templates/
│   ├── design_solution.md.jinja
│   ├── implement_design.md.jinja
│   ├── repair_code.md.jinja
│   ├── revise_design.md.jinja
│   ├── decision.md.jinja
│   └── decompose.md.jinja
├── schemas/
│   ├── design_result.json      ← design_solution 期望输出（purpose + pseudocode）
│   ├── implement_result.json   ← implement_design / repair 期望输出（files[]）
│   ├── decision_node.json       ← DecisionNode 期望输出（oneOf 三 state_assessment kind）
│   ├── pseudo_check.json        ← pseudocode_check 期望输出（schema 就绪，检查命令待接入）
│   ├── repair_result.json       ← repair_code 期望输出（files[] + changes[]）
│   └── decompose_result.json   ← decompose 期望输出（rationale + children[]）
└── assets/
    └── sophia_syntax_baseline.md  ← 语言语法基线（system preamble，见 8.3）
```

每个进入工作流闭环的 LLM 步骤（design / implement / repair / decision / decompose）都必须有一个
对应的严格 schema（`additionalProperties:false`）。**schema 必须是忠实契约**：实现中曾出现
`decision_node.json` 只 require `state_assessment.kind`、比 Rust 端判别联合宽松，导致"schema 通过但
反序列化失败"——已收紧为 `oneOf` 三个 kind 各自 required 全字段。新增工作流步骤时，schema 的字段
约束必须与服务端反序列化目标类型严格对齐，不留宽松缝隙。

### 8.2 工程要求

- 使用 `minijinja`（Rust 的 Jinja2 实现）做 prompt 模板渲染，**不用字符串拼接**；
- 每个 template 对应一个 schema 文件；schema 文件是 `complete_structured` 调用的输入；
- Prompt 模板的变更用 `insta` snapshot testing 捕获渲染结果，**防止模板变更静默影响 LLM 行为**。

### 8.3 语言基线 prompt 资产（preamble）

LLM 没见过 Sophia-Core 语法，任何要求模型**产出 / 修复 `.sophia` 源码**的步骤
（`implement_design` / `repair_code`，以及调度器分派到的等价步骤）都必须先给它一份
**Sophia-Core 语法基线**。这份基线沉淀为**正式的 prompt 资产**，由 `prompt` crate
统一持有、被所有工作流共享，而不是散落在各 example / CLI 命令里各写一份。

**为什么是 prompt 资产，而不是 stdlib：**

| 维度 | 结论 |
| --- | --- |
| 制品种类 | 语法基线是**面向 LLM 的自然语言指令 + 中立示例**，不是能被编译器解析的形式化 `.sophia` 源码——放进 stdlib 会污染"stdlib = 可解析的形式化契约"这一不变量 |
| 消费方 | 它只在 **workflow / LLM 层**被消费；stdlib 由零 IO 的编译器 `core` 消费，二者消费方不同 |
| 分层 | `core` / stdlib **不得依赖 workflow**；语法基线天然属 workflow 侧，放 prompt 不破坏分层 |
| 漂移守护 | 与 templates / schemas 同类，复用 §8.2 已有的 `insta` snapshot 守护；stdlib 的守护是"能否解析"，对自然语言基线不适用 |

因此语法基线作为 prompt 资产落在 `prompt/assets/`，与 templates / schemas 并列：

```
prompt/
├── templates/
├── schemas/
└── assets/
    └── sophia_syntax_baseline.md   ← 语言语法基线（system preamble，所有实现/修复步骤共享）
```

> **库资产不在 prompt crate**：库目录 / 库资产（`<lib>.md`）由 `sophia-stdlib` 的 `LibraryRegistry`
> 承载（清单驱动，见 `stdlib_design.md`）——`prompt` crate 只持核心语法基线，不持任何库内容（库知识与
> 核心语法边界分明，且 prompt 不应依赖库内容层）。

`prompt` crate 暴露资产取用 API：`preamble(name)`（常驻语法基线），以及工作流步骤的**规范 system prompt
文案** `design_system_prompt()` / `implement_system_prompt(stdlib_block)`（单一来源——此前 CLI / e2e /
benchmark 三处各有副本、易漂移，已收敛到此）。库目录 / 资产由调用方从 `LibraryRegistry` 算得后传入
（`design_solution` 模板 `stdlib_catalog` 变量 = `registry.catalog()`；`implement_system_prompt` 的
`stdlib_block` = `registry.preamble(libs)`，见 `stdlib_implementation.md`）。资产均 `include_str!` 内嵌，
自包含可复现。

**两条硬约束（与"防答案泄漏"原则一致，最要紧）：**

1. **只含可泛化的标准语法规则**——语法形状、类型集合、语句 / 表达式子集、effect /
   capability 何时必需等**语言级事实**；用与任何具体任务无关的中立示例（如 `Toggle`）。
   **严禁**出现任何具体任务的领域名 / 节点名 / 状态值 / 业务逻辑（那是答案，不是语言基线）。
2. **只放决断性信息**——语法是确定的、人人适用的事实，放进脚手架降低 LLM 负荷是合理的；
   但**不把格式压给伪代码**：设计（`design_solution`）阶段产出的是**语义伪代码**
   （semantics > format），不注入语法基线，否则会逼模型产出格式化伪代码、反在实现阶段
   制造不必要的修复点。语法基线只进**实现 / 修复**阶段。

snapshot 测试守护该资产的内容变更；后续语言子集扩展（如 §16 起步子集之外的特性落地）时，
基线在此**单点**更新，所有工作流与 example 自动同步。

### 8.4 Prompt 在调用时刻渲染：`StepPrompts` 提供者

**这是接近语言层的设计**：在 Sophia 的模型里，prompt 是 LLM 看到的**全部世界**。`language_design.md`
§10.7 / §10.8 规定——每次 LLM 调用的 prompt 必须由**调用时刻的 active context** 渲染（节点摘要、
祖先链、相关诊断、预算、action-rooted 语义上下文），而这恰恰就是 `consumed→ ContextSnapshot` 边所
快照、所审计的那一份。因此**prompt 必须随图状态在每一步重新渲染**，不能预先渲染一次后复用。

#### 8.4.1 缺陷（实现反哺，必须根除）

调度器 `run_goal_loop` 早期实现把每步的 `CompletionRequest` 作为**预渲染的静态值**经 `StepRequests`
传入，每轮复用同一份。这违反 §10.7 / §10.8，且不可用：

- **状态不演进**：第 N 轮 decision 看到的 prompt 与第 1 轮**逐字相同**，prompt 里没有"已 design 过"
  / "当前有伪代码" / "上一候选的诊断"等演进信息。LLM 因此无法自主推进（典型：反复选 `design_solution`
  原地打转，或耗尽伪代码版本预算），多步启发式编排（§10.8 的核心能力）名存实亡。
- **snapshot 失真**：`ContextSnapshot` 记录的是**当前** active context，但 LLM 实际看到的 prompt 由
  **陈旧**状态渲染——二者不一致，破坏 §10.7 "snapshot 100% 复现 LLM 当时所见" 的根本保证与 anti-cheat
  审计前提。
- **实现步骤拿不到伪代码**：调度器在运行时 design 出一个 PseudocodeNode，却无法把它的正文注入到一个
  预先构造好的静态 implement 请求里——实现步骤因此看不到要实现的伪代码。

这不是可以打补丁的局部问题：静态请求模型与"prompt = 调用时刻 active context 的投影"这一根本设定
直接冲突。**正解是把"预渲染的请求"换成"在调用时刻据当前状态渲染请求的提供者"**。

#### 8.4.2 理想设计：`StepPrompts` 提供者 trait

引入一个由 `workflow/engine` 定义、协调层（CLI / example）实现的**提供者**抽象。调度器在每一步**即将**
发起某类 LLM 调用时，回调提供者，传入"当前该步骤所需的、源自图状态的输入"，由提供者据此**当场渲染**
出 `CompletionRequest`：

```rust
/// 工作流步骤的 prompt 提供者：在调用时刻据当前图状态渲染请求。
///
/// 分层：engine 不持有 prompt 模板与 active-context→prompt 的抽取逻辑（那是协调层职责，
/// 见 §3.3）；engine 只在恰当时机回调，并把"该步骤源自图的输入"作为参数交给提供者。
pub trait StepPrompts {
    /// 渲染 decision 步骤请求。`ctx` 是调用时刻的 active context 视图，
    /// `budget` 是当前剩余预算，`focus` 是当前焦点目标域。
    fn decision(&self, ctx: &ActiveContext, budget: BudgetView, focus: NodeId) -> CompletionRequest;

    /// 渲染 design 步骤请求（语义伪代码阶段，不注入语法基线）。
    fn design(&self, ctx: &ActiveContext, focus: NodeId) -> CompletionRequest;

    /// 渲染 implement 步骤请求。`pseudocode` 是本轮 design 产出的伪代码正文
    /// （由调度器在运行时取得后传入——根除"静态请求拿不到伪代码"的缺陷）。
    fn implement(&self, ctx: &ActiveContext, pseudocode: &str) -> CompletionRequest;

    /// 渲染 repair 步骤请求（据上一候选正文 + 结构化诊断）。
    fn repair(&self, files: &[(String, String)], diagnostics: &[DiagnosticItem]) -> CompletionRequest;
}
```

要点：

- **每步请求在调用时刻渲染**：调度器在 `make_decision` / `dispatch_design` / `dispatch_implement`
  内部，先（已有逻辑）确定性计算 active context 并建 `ContextSnapshot`，**用同一份 active context**
  回调提供者渲染请求——保证 prompt 与 snapshot 同源，根除失真。
- **schema 仍随请求一起注入**：schema 是结构契约，与请求同属一步；提供者可在内部持有，或仍由调度器
  按步骤选取内置 schema（`prompt::schema_for`）。为最小化耦合，schema 由调度器按固定步骤选取（design→
  `design_result`、implement→`implement_result`、repair→`repair_result`、decision→`decision`），
  提供者只负责渲染请求正文与 system。
- **伪代码正文回传**：`design` 步骤产出的 PseudocodeNode 正文（`PseudocodeArtifact.text`）由调度器
  持有并在 `implement` 步骤回调时作为参数交给提供者——这是静态模型根本做不到、而提供者模型自然支持的。
- **repair 请求**由 implement-loop 在每次 check 失败后据诊断回调提供者渲染（implement-loop 已持有
  上一候选文件与诊断），与 decision/design/implement 同构。
- **分层守恒**：engine 仍不含任何 prompt 模板或 active-context→文本的抽取规则；它只定义 trait 与回调
  时机。模板渲染、语法基线拼装、防答案泄漏纪律全部留在协调层的提供者实现里。

> 上面的 trait 是设计骨架。实际 `StepPrompts`（`workflow/engine` `prompts` 模块）随能力扩充已增 `revise`
> （概念性诊断驱动重写伪代码）与 `decompose`（目标树遍历层用）两个方法，`decision` 传 `GoalProgress`
> 进度视图，`implement` / `repair` 增 `libraries`（design 阶段所选标准库，S2 按需注入库资产）；确切签名
> 以代码为准（设计骨架不复制精确签名以免漂移）。

#### 8.4.3 为何不是折中

- 不保留"静态请求 + 可选提供者"双路：那是双栈，违反单一路线原则（engineering_notes）。`StepRequests`
  静态结构整体**被提供者 trait 取代**，调度器与 implement-loop 的签名相应改为收 `&impl StepPrompts`。
- 不在 engine 内塞模板：会破坏 §3.3 分层（engine 不依赖具体 prompt 资产语义）。
- 不让 snapshot "尽量接近"：要求**同源**——同一次 active-context 计算既喂 snapshot 又喂提供者。

落地后，G3（启发式节点处理）才能真正考察"LLM 据演进状态自主决策 decision→design→implement 推进到
可物化候选"，而非被静态 prompt 锁死。

### 8.5 目标树遍历层（spine 之上的非线性图操作）

调度器 spine（`run_goal_loop`）只推进**单个**目标域，遇到 `decompose` / `backtrack` 这类非线性
树操作时刻意让位（`Outcome::Yielded`），不内联其语义。承接这两个让位动作的是 spine **之上**的
独立**目标树遍历层** `engine::run_goal_tree`（`traversal` 模块）：

```text
run_goal_tree（遍历层，非线性）
  └─ run_goal_loop（spine，单目标线性推进）
       ├─ Yielded(Decompose) → decompose_goal（LLM 拆解 + build_decomposition 落图）→ 递归子目标
       ├─ Yielded(Backtrack) → 放弃分支（GoalResolution::Backtracked）
       └─ CandidateReady / BudgetExhausted / Failed / 其它 Yielded → 直接归结
```

要点：

- **分层纯净**：spine 不变（仍是线性薄层）；遍历层只在 spine 让位后**执行**树操作并递归，
  动作选择仍由 spine 内的 `DecisionNode` 产生（10.8 动作选择 / 执行分离不破坏）。
- **decompose 的图构造是确定性的**：LLM 只给出拆解**结构**（rationale + children[]，受
  `decompose_result` schema 约束）；落图由 `graph-db::build_decomposition`（确定性 helper）完成
  ——建 `Decomposition`、`parent decomposes→ Decomposition`、子目标 `Objective` 各 `member_of→
  Decomposition`。这与「LLM 产出内容、确定性管线落图」的全局纪律一致。
- **I6 锚点**：`Decomposition` 是 `decompose` 动作的 **LLM 执行产物节点**（承载 LLM 生成的 rationale
  与拆解结构），与 design 的 `Pseudocode`、implement 的 `Code`、评估的 `Assessment` 同属 LLM 输出，
  故它**自身** `consumed→ ContextSnapshot`（I6）——锚定在产出这次拆解的 LLM 调用快照上，而非触发
  它的 `DecisionNode(decompose)`（那是另一次"该不该拆"的决策调用，§10.8 动作选择 / 执行分离）。
  `build_decomposition` 因此接收并校验 `snapshot` 参数，由 `decompose_goal` 把 `run_llm_step` 建的
  snapshot 透传进来。子 `Objective` 是结构性派生节点（类比 assessment 协议里的 FirstSlice /
  Constraint），经 `member_of` 间接锚定，**不单独** `consumed→`。spec 的 I6 集合与 `consumed` 边
  目录已相应纳入 `Decomposition`（workflow_graph_spec §I6 / 6.1 / 4.1.4）。
- **不伪造人类授权**：backtrack 只在遍历层记录"放弃"，append-only 图保留被弃子树，**不创建
  `WithdrawalEvent`**（撤销是人类权威，N4）；binding 也不伪造——LLM 派生子目标默认未绑定，
  人类接受 `Decomposition` 后才沿 `member_of` 继承 binding（spec 5.3）。
- **人类授权检查点（拆解审查者）**：上一条的"人类接受 `Decomposition`"由遍历层的注入回调
  `DecompositionReviewer` 承载——decompose 落图后、递归子目标**前**回调它：裁决 `Accept` → 遍历层
  建**真实** human `AcceptanceEvent accepts→ Decomposition`，子目标随即沿 `member_of` 继承 binding、
  进入各自 active context（这正是子目标 design/implement 能看到自己题面的前提）；裁决 `Reject` →
  不递归、不伪造 withdrawal（记 `GoalResolution::DecompositionRejected`）。引擎**不**在审查者之外伪造
  授权；`AutoAcceptReviewer` 把"人类接受"自动化（调用方代表人类，仍走真实 `AcceptanceEvent` 落图，
  非绕过 binding 谓词），适用于 e2e harness 与无人值守策略，真人 CLI 应实现交互式审查者。
- **预算**：`TreeBudget` 增设 `max_depth`（decompose 嵌套深度）与 `max_goals`（spine 调用总数），
  在 `SchedulerBudget` 之上防递归爆炸。

#### 8.5.1 为何独立成层，而非塞进 spine

把树操作塞进 spine 会让它退化为"塞满分支语义的大杂烩"（违反单一职责）；而把 spine 做成树驱动器
又会让"单目标线性推进"这个可独立测试、可独立复用的能力消失。分两层后：spine 仍可单独用于
"只推进一个已确定目标"（CLI implement-loop 即如此），遍历层则专注非线性控制——各自职责单一、
可分别测试。

---

## 九、CLI 规范

`clap` 作为 CLI 框架。

### 9.1 编译器命令（确定性，不调用 LLM）

| 命令                                   | 作用                                     |
| -------------------------------------- | ---------------------------------------- |
| `sophia init`                          | 创建标准目录结构和 `sophia.toml`         |
| `sophia index`                         | 扫描 node 文件并生成 `asg_index.json`    |
| `sophia parse <file>`                  | 解析单个 node 文件                       |
| `sophia graph`                         | 输出 ASG 摘要                            |
| `sophia context --action <ActionName>` | 生成 action-rooted semantic context     |
| `sophia context --task <TaskName>`     | 生成确定性 task closure                   |
| `sophia check`                         | 执行静态检查和 strip-assist 等价门禁     |
| `sophia build`                         | check 通过后 emit WASM artifact + strip-assist artifact 层门禁（v1 工作流 A） |
| `sophia run <ActionName>`              | 执行 action（解释执行）；`--trace` 打印执行图 Trace 投影 |
| `sophia smoke`                         | 一键跑通 init / check / build / run 烟雾测试 |
| `sophia repair-context --error <id>`   | 生成 LLM 修复上下文，不调用 LLM          |
| `sophia lsp`                           | 以 stdio 运行 Language Server（hover / diagnostics / goto） |

当前编译子集把 strip-assist 等价作为 `check` / `build` 门禁执行；单独的 `sophia strip-assist` CLI 是后续扩展点。

### 9.2 工作流命令（涉及 LLM）

已实现的 `graph` 子命令：

```bash
sophia graph init
sophia graph start "目标"
sophia graph nodes
sophia graph context
sophia graph design <NodeId> --model <ModelName> [--mode openai|ollama] [--base-url <url>]
sophia graph implement-loop <NodeId> --pseudo <PseudoId> --model <ModelName> --max-repairs 2
sophia graph select <NodeId>
sophia graph materialize <NodeId>
```

`start → design → implement-loop → select → materialize` 端到端贯通（确定性子命令
`init` / `start` / `nodes` / `context` 不调用 LLM）。

实现进展与固化的约定（实现反哺）：

- **`sophia graph` 双形态**：无子命令时输出 ASG 摘要（编译器侧，向后兼容）；带子命令时操作
  Development Graph（`sophia-runs/graph/dev_graph.sqlite`，事件溯源，跨进程 replay 持久化）。
- **LLM 后端 flags 统一**：`--model`（必填）/ `--mode openai|ollama` / `--base-url` / `--api-key`
  （或 `SOPHIA_LLM_API_KEY` 环境变量）→ 构造 `HttpLlmClient`。CLI 用一次性 current-thread Tokio
  运行时跨越异步边界（`core`/`tools` 保持同步）。
- **失败不伪造成功**：后端不可达 / schema 超重试失败时，命令以失败退出码结束，并在图中保留
  `RawLlmNode`（`attempted→` 目标）+ 调用前已建的 `ContextSnapshot`（保证可审计可复现）。
- **`design` / `implement-loop` 的中间产物**：`.pseudo` 文本与候选 `.sophia` 文件正文落盘到
  `sophia-runs/graph/artifacts/`（**未物化**）；图节点只记路径（4.4.3/4.4.4）。
- **`select` / `materialize` 重跑 gate**：二者分属两个进程，类型状态 gate 证明不可跨进程持久化，
  故各自从 artifacts 重新加载候选并**重跑全部 materialize gate**（code_check / constraint_audit /
  artifact_diff / runtime validation），任一未过即阻断并 emit 对应 `DiagnosticNode`，绝不伪造通过。
  `materialize` 沿 `selects→` 边定位候选 Code，gate 通过后原子写入 `domains/`。

**路线图（尚未实现的子命令）**：`pseudo-check` / `pseudo-outline` / `pseudo-scaffold`（`.pseudo`
独立校验 / 大纲 / 脚手架）、`check` / `audit` / `diff` / `verify`（对图中节点单独跑某一 gate）；
调度器高层动作（decompose / backtrack / revise / 澄清）的 CLI 入口亦待接入（引擎侧 `run_goal_tree`
等已就绪，见 workflow/engine）。

#### 9.2.1 隐藏验证用例存储与 constraint_audit gate 接线（设计）

constraint_audit gate 的 regression 由 bound invariant 的 hidden case 驱动。hidden case 的期望
输入 / 输出是 **validation-only** 数据，绝不能让被验证的 LLM 看见（防答案泄漏，最要紧）。完整 schema
见 `workflow_graph_spec.md` 五A 节；本节定调实现接线与分层归属：

- **三层隔离（结构性 anti-cheat）**：① 图节点只存不透明引用 `ConstraintPayload.verifier.ref`，不存
  用例正文；② active context 的 `ConstraintView` 整体剔除 `verifier`（连 ref 都不投影给 LLM，已是
  现状）；③ 用例正文存于图外的 `sophia-runs/verifiers/hidden.json`（`ref → {entry_action, args,
  expected}`），与 `dev_graph.sqlite` 物理隔离，**只有确定性 gate 取用**。
- **gate 流程（materialize 时，确定性）**：取 bound invariants → 对每条 `verifier.kind=HiddenCase`
  按 `ref` 从 `hidden.json` 加载用例（缺 → 硬错误阻断，与"无运行器"同等诚实）→ 反序列化为 runtime
  值 → `runtime::run_hidden_case`（v0 解释器真正执行候选）→ 映射 `sophia_audit::VerifierOutcome`
  注入 `audit_constraints` → emit `DiagnosticNode(ConstraintAudit / RegressionGate)`。
- **分层归属（沿用「注入报告」模式）**：**执行**属 `runtime`（`run_hidden_case`，已实现）；**判定**属
  `tools/audit`（消费注入的 `VerifierOutcome`，已实现）；**加载 `hidden.json` + 串联执行与判定 + 写图**
  属**协调层**（CLI `graph_cmd` 的 `run_constraint_audit`）。`tools` / `runtime` 都不感知 `hidden.json`
  与 Development Graph——隐藏存储的存在只对协调层可见。gate 侧需从**图节点原始 payload**（非 `*View`）
  读 `verifier.ref` 来查 `hidden.json`，故 `run_constraint_audit` 直接遍历 bound invariant 的
  ConstraintNode payload，而非经 active context 的 `ConstraintView`（后者刻意不含 verifier）。
- **来源纪律**：`hidden.json` 由出题方 / 维护者写入，**不由 LLM 产生**（LLM 产被验证的代码，不产
  验证它的标准答案）；其写入路径与生成代码路径物理隔离，CI 可单独核查它从不进任何 prompt。
- **状态**：**已实现**。执行器（`run_hidden_case`）、判定（`audit_constraints`）、`verifier_store`
  加载器、`run_constraint_audit` 内的 gate 自动驱动均已落地并经 3 项 CLI 集成测试验证（通过→放行 /
  失败→阻断 / 缺用例→诚实 RegressionGate 阻断）。`graph select` / `materialize` 的 constraint_audit
  gate 现据 `hidden.json` 在候选上真正执行 hidden case 驱动 regression gate。e2e harness 的
  `Case.expect` 是同一机制的测试态对应物（harness 充当临时隐藏存储）。


---

## 十、Language Server

### 10.1 技术栈

- `tower-lsp`

### 10.2 功能

- goto definition
- hover
- diagnostics
- rename
- autocomplete
- semantic navigation

### 10.3 架构原则

LSP **基于 semantic data 工作，而不是直接遍历 AST**。

LSP 是真正需要增量分析的场景（hover/completion 必须低延迟）。增量分析在后续阶段引入；起步阶段先用 module/symbol/type cache 满足基本需求。

---

## 十一、Formatter

### 11.1 工作流程

```text
AST / HIR
    ↓
Pretty Printer
```

### 11.2 职责

- stable formatting
- semantic-aware formatting
- deterministic formatting output

输出必须确定：同源同输出。

---

## 十二、Runtime Tracing

### 12.1 技术栈

`tracing` crate。

### 12.2 职责

- runtime inspection
- async task tracing
- execution timelines
- semantic event tracing
- agent execution debugging

### 12.3 与 Execution Graph 的映射

Trace 必须携带对 Execution Graph 中具体节点和边的引用。详见 `language_implementation.md` 9.4 节。

---

## 十三、Testing Infrastructure

### 13.1 Snapshot Testing

`insta` crate。Snapshot 目标：

- AST
- HIR
- Semantic IR
- Execution Graph IR
- Prompt 模板渲染结果

### 13.2 Semantic Testing

- type inference validation
- effect analysis validation
- scheduling validation
- runtime trace validation

### 13.3 Append-only 不变量测试

CI 中的 diff 检测测试守护：节点和边一旦写入文件即只读。

---

## 十四、阶段路线

### 14.1 v0：解释执行

```text
源码 → AST → HIR → Semantic IR → Execution Graph IR → Interpreter
```

无 codegen。`sophia run` 由 Rust 进程内解释器执行；runtime input/output validation 由解释器直接消费 metadata。

### 14.2 v1：WASM codegen + 语言 / 标准库扩充

v1 是"把玩具语言变成严肃语言"的阶段（见 `language_design.md` §1.1 目标 1）。它有**两条并行工作流**，
都服务"让 Sophia 真正可用"，而非仅服务论文：

**工作流 A — WASM codegen（既定的执行后端升级，v1 头等事项）**
- 引入第一个 codegen target：WASM。把 Semantic IR / Execution Graph IR 投影为可部署的 WASM artifact，
  使执行后端从"仅 Rust 进程内解释器"扩展为"可被 Node / Python / 浏览器 / 边缘 runtime 嵌入运行"。
  理由与 emit 形态见 `language_implementation.md` 第十二节。
- **解释器不退役**：它继续作为 codegen 的**等价基线 oracle**——WASM 输出必须与解释器结果一致
  （差测试）；strip-assist 等价门禁在 v1 增加对 WASM artifact 的字节级比对（`language_design.md` §5.1）。
- 配套引入基于 Salsa 思想的增量查询架构，支撑 LSP 低延迟。

**工作流 B — 语言能力 / 标准库扩充（需求驱动，支撑更复杂的程序与更有说服力的基准）**
- 目的：v0 起步子集只够跑最小语言面；要写出"严肃程序"和更复杂的基准，需要补齐语言能力。但扩充
  **需求驱动**——由具体演示需求触发、逐项过设计门，**不预先铺特征清单**（详见 `dev_checklist_v1.md`
  §二）。
- 两条准入通道（`language_design.md` §1 定位原则、技术报告 §3）：① 演示需求驱动（缺什么补什么、需求
  封顶）；② 强论证的 LLM-native 特征（门槛更高，须给出"为何专门服务 LLM 自动编程"的论证 + 可度量收益）。
- v1 范围由三个演示需求封顶：**D1** 可失败结果建模、**D2** 网络获取 + intent 安全（旗舰 LLM-native 演示，
  落地报告 §7/§8 的 accept/reject 主张）、**D3** 严肃管线综合题；反推出最小扩展集 **F1**（类型语法
  统一 + 可失败返回 `one of {...}`，见 `docs/type_system.md`）
  + **F2**（`Http` effect 族，标准库 effect 族进 `BUILTIN_EFFECT_OPS`、复用 intent + capability，零新语法）+ **S1**
  （HTTP host 标准库，仅 workflow/runtime 层）+ **S2**（标准库提示词脚手架）。**标准库重定位**（2026-05-31）：
  确立「I/O = 库」（文件 / 网络 / 数据库都是标准库，`Console` 是内置输出原语），移除语义不清的 `storage`
  节点 + `DB` 内置 effect + `Persisted` intent，新增 `File` 库（v1 内）。
- **显式推迟 v2+**（无 v1 演示需求触发）：`entity.with`、跨 domain / library intent 数据流、`requires`/
  `ensures` 合约证明子系统、`task` 执行入口——见 `dev_checklist_v1.md` §二。
- **标准库 = 功能库，非协议栈**：随演示需求按需引入最小 host / 约定（如只做 `Http.Get` 功能、不自建
  TCP/IP / TLS / socket 底层，交给宿主 host 运行时）；仍遵循"无 ambient authority、effect 显式声明"原则。
- **标准库提示词脚手架（S2）**：LLM 对标准库无先验知识，必须为每个库提供**标准化、按需取用**的库介绍
  prompt 资产（复用 §8.3 preamble 机制 + `prompt/assets/`，用到哪个库才注入哪份），否则 LLM 无法使用
  标准库。属提示词工程，v1 必须考虑。

> v1 完成判据：① 起步子集程序可经 WASM 后端编译并与解释器结果逐一等价；② **三个演示需求 D1/D2/D3 在
> sophia mode 端到端跑通（benchmark L6+），其中 D2 给出一条真实 accept/reject 矩阵条目**；③ strip-assist
> 等价在 artifact 层成立。WASM 与（有界的）语言扩充都达标，v1 方完成——二者缺一不可。

### 14.3 v2 起：可选 backend 与演化能力

backend（按需添加）：

- native（cranelift / LLVM lowering）：性能场景；
- 具名语言 emit（TS / Python 等）：仅当出现明确的部署需求时才考虑，不进入默认路线。

演化能力（把"图语言 + 开发过程也是图"做实，支撑真实多轮项目而非一次性合成）：

- **edit transition 成为图一等动作 + Evolution Boundary**：字段新增、intent 收紧、error 扩展、
  action 拆分等多轮演化在无人审查下拒绝未授权的 semantic drift；
- **Semantic Identity** 与跨 domain / library protocol（`sophia.lock`、publish/consume、formal-only 视图）；
- **更强 strip-assist 等价**：从 IR / artifact 比对推进到独立的 formal-only hash。

> 这些是 v1（让单 domain 语言真正可编译可用）之后的方向：让 Sophia 能维护多 domain、多轮演化的真实
> 项目。它们依附于 v1 打下的可编译基础，不与 v1 的 WASM / 语言扩充争优先级。

---

## 十五、架构原则

| 原则                          | 表述                                                       |
| ----------------------------- | ---------------------------------------------------------- |
| semantic-first architecture   | 语义优先，parser/codegen 是手段                            |
| runtime introspection         | 运行时可观测，trace 投影到 Execution Graph                 |
| graph-based execution         | 所有执行都有显式图结构                                     |
| incremental analysis          | 起步阶段不实现，接口预留                                   |
| stable semantic infrastructure| Semantic IR 是长期资产                                     |
| async-native execution        | execution layer 默认 async                                  |
| agent-native semantics        | 节点语义服务 LLM 与 agent 系统                             |

长期最核心的资产：

- **Semantic IR**
- **Execution Graph IR**
- **Runtime Semantics**

而**不是** parser 或 backend code generation。

---

## 十六、Non-goals（工程层）

- **不追求与 LangChain、LangGraph 等框架的兼容性**：Sophia 是独立语言，不是 wrapper；
- **v0 不实现任何 codegen**：解释执行是唯一执行后端；
- **v1 仅实现 WASM 一种 codegen target**：native / 具名语言 emit 推迟到出现明确需求；
- **不实现分布式执行**：checkpoint/resume 语义在 IR 层定义，但不跨进程；
- **编译器不调用 LLM**：所有 LLM 调用只发生在 `workflow` 层，`core` 保持纯确定性；
- **`pseudocode_check` 不做语义质量判断**：只验证结构完整性（heading 存在性），语义质量是写作纪律问题。
