# Sophia 工程进展 · v0（dev_checklist_v0，已冻结归档）

> **状态：v0（解释执行）阶段归档，已冻结。** v0 核心链路 + 工作流闭环已完成，本文档作为 v0 的
> 进展记录与变更历史**只读保留**，不再新增条目。v1 的进展跟踪见 **`dev_checklist_v1.md`**（当前
> 活跃 SSOT）。工程决策日志仍统一在 `engineering_notes.md`（跨版本，不分 v0/v1）。
>
> 本文档按 `language_implementation.md` 第十九节 v0 构建顺序（step 1–15）组织，并补充工程基础设施项。
> 状态分为四类：**已完成** / **部分完成** / **尚未完成** / **计划完成（路线图）**。

---

## 一、概述

当前阶段：**v0 解释执行（已基本完成）→ 准备进入 v1**。构建顺序 step 1–15 的核心已全部落地：编译器主链路
（`parse → HIR → semantic → exec-ir → 解释器 run`）跑通起步子集；工作流图持久化
（SQLite + 事件溯源 + 不变量强制 + provenance 工厂）、Active Context 推导、LLM 结构化
输出 fallback 与 prompt 模板、Materialize Gate 类型状态链、Language Server 起步功能均已实现。

已建立 Rust 工作区与严格分层（`core` 无 IO、不依赖 `workflow`）。LLM 已可接入：
`HttpLlmClient` 支持 OpenAI 兼容与 Ollama；`run_llm_step` 固化「建 snapshot → 结构化调用 →
emit 节点 / RawLlmNode 兜底」；评估拆解协议落地；**工作流执行闭环**（design/implement/repair
建 Pseudocode/Code 节点 + `addresses→`/`implements→`/`repairs→` 边）、**implement-loop**
（implement→check→repair 预算闭环）、**Selection/Materialize 节点编排**与据 DecisionNode 驱动的
**工作流总调度器 spine**（decision → design → implement-loop → 可物化候选，含预算 / I6）均已落地。
**调度器高层动作**（revise / 澄清接入 spine；decompose / backtrack 由 spine 之上的目标树遍历层
`run_goal_tree` 承接）、**多候选评分排序**、**`effect` 顶层声明**（消除 grammar 硬编码 effect，
`Family.Op(args)` 通用引用 + 内置族 Console/DB 表 + 用户可声明领域 effect）、**执行 Trace 投影**
（exec-ir 稳定边 ID + runtime trace + CLI `--trace`）、**constraint audit verifier 执行器 + 隐藏验证
用例存储**（hidden case 在候选上真正驱动 regression gate）亦已落地，全部位于单一代码路径内、不提供
功能性 fallback（见 `engineering_notes.md` 单一路线原则）。CLI 已能从命令行端到端驱动
Development Graph 工作流：`graph init`/`start`/`context`/`nodes`/`design`/`implement-loop`/
`select`/`materialize`（`start → design → implement-loop → select → materialize` 全链路贯通，
materialize 重跑 gate 后原子写入 `domains/`），并提供确定性便利命令 `smoke` / `repair-context` /
`run --trace`。

**v0 收尾 / v1 启动**：v0（解释执行）核心链路与工作流闭环已完整，e2e 6 组 + benchmark 难度阶梯
L1–L5 均跑通真实 LLM。下一阶段 **v1** 是"把玩具语言变成严肃语言"（`language_design.md` §1.1 目标 1），
两条并行工作流：**A WASM codegen**（执行后端从解释器扩展到可部署 artifact，解释器转为等价 oracle）+
**B 语言 / 标准库扩充**（`Result<T,E>` / error handle / `task` 执行 / `entity.with` / 跨 domain
boundary / 合约证明，支撑更复杂的"严肃程序"与基准阶梯 L6+）。v1 构建顺序见 `language_implementation.md`
§19.1，路线见 `engineering_architecture.md` §14.2。

> 注：早期曾引入 `node` 顶层构造与 agent 编排（prompt/router/aggregator/tool/stream 内置节点、
> `Llm`/`Tool`/`Stream` effect、`sophia-stdlib` crate、单 node 解释执行）。因偏离语言定位（agent
> 编排非本语言目标），该方向已于 2026-05-30 **彻底移除**（见变更记录）；`effect` 顶层构造作为
> 独立正确的去硬编码成果**保留**。

构建顺序里程碑（来自 `language_implementation.md` 第十九节）：

| 步骤 | 子系统 | 状态 |
| ---- | ------ | ---- |
| 1 | `syntax`：grammar / CST / **AST** / span | 已完成 |
| 2 | `hir`：名称解析 / 模块解析 / scope | 已完成 |
| 3 | `semantic` 三层：type / effect / contract | 已完成 |
| 4 | `exec-ir` + 解释器：跑通起步子集 | 已完成 |
| 5 | 解释器 runtime input/output validation | 已完成 |
| 6 | GraphStore + 节点/边 schema | 已完成 |
| 7 | ContextSnapshot + active context 推导 | 已完成 |
| 8 | 小节点（Decomposition/Constraint/AcceptanceCriterion） | 已完成 |
| 9 | 核心目标节点（Objective/Milestone） | 已完成 |
| 10 | 事件节点（Acceptance/Withdrawal/Activation） | 已完成 |
| 11 | 评估族（Assessment/FirstSlice/Decision 拆解协议） | 已完成 |
| 12 | DiagnosticNode（5 类 kind） | 已完成 |
| 13 | 接入 LLM（design/implement/repair/decision） | 已完成 |
| 14 | Selection/Materialize + Materialize Gate 类型状态链 | 已完成 |
| 15 | LSP 起步（hover/diagnostics/goto） | 已完成 |

---

## 二、工作清单

### 2.1 已完成

#### 工程基础设施
- [x] Cargo 工作区与 14 个成员 crate 划分（`core` ×4 / `workflow` ×4 / `tools` ×3 / `lsp` / `cli` / `runtime`）
- [x] 严格分层纪律：`core/*` 零 IO、不依赖 `workflow`（仅用 `thiserror`/`slotmap`/`serde` 等纯库）
- [x] `workspace.dependencies` 统一依赖版本来源
- [x] 版本对齐：tree-sitter crate 0.26 + CLI 0.26.9 + ABI 15（满足三方一致硬约束）
- [x] `rustfmt.toml`（edition 2021 / max_width 100 / Unix 换行）
- [x] `.gitignore`（target、生成物、SQLite、node_modules）
- [x] git 初始化（本地 `main` 分支，无远端；初始 commit 已建立，见变更记录 2026-05-30）
- [x] 错误处理基线：库用 `thiserror`，二进制（cli）用 `anyhow`
- [x] **append-only / I9 CI 不变量测试**（`graph-db/tests/append_only.rs`）：经只读审计访问器
      `GraphStore::raw_event_log` 守护事件日志只增不改——每次写后旧日志逐字节为新日志前缀、
      被拒写无副作用、重开库 replay 不改写历史（确定性、进 `cargo test` / CI）

#### 语法层（core/syntax，构建顺序 step 1）
- [x] Sophia-Core Tree-sitter grammar：覆盖全部 9 类顶层节点
      （domain/entity/state/transition/error/capability/storage/action/task）
- [x] Body 子语言：let/set/return/raise/if-else/match/repeat/print + 受限表达式
- [x] 类型语法：标量、`List<T>`、`Optional<T>`、Intent wrapper、entity/state 引用
- [x] Semantic Assist 字段解析为独立节点（为 strip-assist 等价门禁预留）
- [x] 关键消歧：表达式点号访问为 `field_access`；状态值 pattern 为 `qualified_name`；
      `match` 头部使用 no-struct 表达式变体；`_` catch-all 在语法层不可解析（永久禁止）
- [x] `tree-sitter.json`（ABI 15）+ `build.rs`（仅编译本地 `parser.c`，不嵌入外部仓库）
- [x] CST 包装 `SyntaxTree`：根节点、源码片段、`to_sexp`、容错诊断收集（确定性前序遍历）
- [x] `Span` / `Point`（0 基行列 + 字节偏移），贯穿后续 IR
- [x] 稳定入口 `parse_str`；类型化 `SyntaxError` + `SyntaxDiagnostic`
- [x] **AST 数据模型**（`ast` 模块）：Arena + `ExprId` 引用（表达式 arena），
      顶层 Item / Callable / Block / Stmt / Pattern / Expr 全覆盖；Semantic Assist 独立建模
- [x] **CST → AST lowering**（`lower` 模块）：丢弃 trivia 保留 span；容错、不 panic；
      字符串去引号 + 转义；稳定入口 `parse_ast` / `SyntaxTree::to_ast`
- [x] 单元测试 + CST `insta` 快照 + lowering 集成测试（13 项，覆盖全部节点类型与 body 子语言）

#### CLI（cli，确定性命令端到端接通）
- [x] `clap` 框架 + `tracing` 初始化；模块分层（`project` 文件扫描 / `render` 诊断呈现 / `commands`）
- [x] **`sophia init`**：创建标准目录（domains + sophia-runs/{generated,task_closures,build,graph}）
      与 `sophia.toml`（5.2 最小配置）
- [x] **`sophia parse <file>`**：单文件解析 + 语法诊断（1 基行列）
- [x] **`sophia index`**：扫描 `domains/`（字典序确定性）→ 生成 `sophia-runs/asg_index.json`（17.2 规范）
- [x] **`sophia graph`**：输出 ASG 节点摘要（名 / kind / domain）
- [x] **`sophia check`**：语法 + HIR 名称解析 + 语义三层；诊断**按文件精确归属**
      （`resolve_item` / `analyze_one_callable`，带稳定 code）
- [x] **`sophia context --action/--task`**：从 action/task root 计算语义闭包（§8，确定性，不调 LLM），
      稳定输出节点 / 解释边 / 文件列表，`--sources` 附带源码内容
- [x] **`sophia build`**：v0 空操作（check 通过后声明，无 codegen）
- [x] **`sophia run <action>`**：扫描 → check gate → 解释执行；`--arg 类型:值` 实参；
      回放 console 输出、呈现返回值或 raise（领域错误以失败退出码）
- [x] 6 项端到端集成测试（init/index/check/run/语义失败拒绝/raise 传播，直接驱动编译出的二进制）

#### CLI Development Graph 工作流子命令（cli `graph_cmd`，架构 §9.2）
- [x] `sophia graph` 改造为可选子命令（无子命令 = ASG 摘要，向后兼容；`--root` 标志化）
- [x] **确定性子命令**（不调 LLM）：`graph init`（建事件溯源 SQLite）/ `graph start <title>`
      （建 human ObjectiveNode）/ `graph context`（推导并展示 active context，不写图）/
      `graph nodes`（列节点，事件溯源 replay 跨进程持久化）
- [x] **LLM 子命令**：`graph design <NodeId>`（design_solution → PseudocodeNode，`.pseudo` 正文落盘
      `sophia-runs/graph/artifacts/`）；`graph implement-loop <NodeId> --pseudo <PseudoId>
      --max-repairs N`（implement→code_check→repair 预算闭环 → 候选文件落盘 artifacts，未物化）
- [x] **select / materialize 子命令**（工作流闭环最后一公里）：`graph select <CodeId>`
      （重跑 gate → SelectionNode `selects→ Code`）；`graph materialize <SelectionId>`
      （沿 `selects→` 找候选 → 重跑 gate → 原子写入 `domains/` + MaterializeNode `materializes→ Selection`）
- [x] **gate 重跑**（类型态证明不可跨进程持久化，对不可逆写盘是更稳妥姿态，design 10.10）：
      code_check（桥接 tools/check）/ constraint_audit（对 bound invariant 跑 tools/audit，
      声明可执行 verifier 却无运行器 → 硬错误阻断，忠实反映「待接入」）/ artifact_diff（strip-assist）
      / runtime validation（起步阶段无 hidden case 可跑则通过，非伪造）；各 gate emit `DiagnosticNode`
      连 `checks→ Code`，任一未过即阻断（不伪造成功）
- [x] engine 重构：`run_selection_materialize` 拆为可组合的 `run_selection` + `run_materialization`
      两原语（CLI select / materialize 分属两进程），`CodeCandidate<Selected>` 仍为两者的 gate 证明
- [x] **LLM 后端 flags**（`BackendArgs`）：`--model` / `--mode openai|ollama` / `--base-url` /
      `--api-key`（或 `SOPHIA_LLM_API_KEY` 环境变量）→ 构造 `HttpLlmClient`；CLI 用一次性
      current-thread tokio 运行时跨异步边界
- [x] **code_check 桥接**（`code_check_files`）：把候选文件桥接 `tools/check`（语法 → HIR → 语义三层
      → strip-assist），产出 `DiagnosticPayload(CodeCheck)` 注入 implement-loop——CLI 是注入点，
      engine 不自行运行 checker（分层）
- [x] **失败不伪造成功**：LLM 后端不可达 → 保留 RawLlmNode（attempted→ 目标）+ 失败退出码；
      候选文件落盘拒绝绝对路径 / `..` 逃逸
- [x] 6 项 CLI 集成测试（graph 无子命令仍出 ASG 摘要 / dev 工作流 init→start→nodes→context /
      start 跨进程 append / design 不可达后端 emit RawLlmNode 并失败 / design 拒非法节点 /
      implement-loop 拒非 Pseudocode 源）+ 7 项单测（4 `code_check_files` + 3 select/materialize：
      干净候选 select→materialize 写 domains / code_check 失败阻断 select / materialize 拒非 Selection）

#### HIR 层（core/hir，构建顺序 step 2）
- [x] **ASG index**（`AsgIndex`）：节点名 → `NodeInfo{kind,domain,path}`；`BTreeMap` 稳定排序；
      `to_json` 产物与 17.2 规范一致（仅顶层节点）；一文件一节点、禁止跨文件重名校验
- [x] **error variant 成员符号表**：variant 不是顶层节点，单独建表（`#[serde(skip)]` 不入 JSON），
      校验 `errors { ... }` 与 `raise Variant`，并禁止 variant 跨 error 重名
- [x] **名称解析**（5.2）：类型引用（标量 / wrapper / entity / state）、capability 绑定、
      errors variant、entity 构造 / transition 调用、callee（内置函数 / transition / action）、
      task include；区分 `WrongReferenceKind` 与 `UnresolvedReference`
- [x] **内置名字表**（`builtins`）：标量类型、13 个 wrapper（容器 + 渐进 + Intent）、
      内置函数 `to_text`、`self` / `output` / `storage` 特殊根
- [x] **scope 分析**（语言设计第七节）：input 为根作用域；`let`/`if`/`repeat`/match-arm 子作用域；
      禁止 shadow 可见变量（含 input）；`Some(name)` 绑定仅限该 arm；`set` 目标须已声明且 mutable
- [x] **跨 domain 检查**：隐式跨 domain 引用诊断（`ImplicitCrossDomain`）；task include 为显式入口豁免
- [x] 容错诊断收集（`HirDiagnostic` + 6 类 `HirDiagnosticKind`，带 span）；硬错误用 `HirError`
- [x] 顶层入口 `resolve_program`（构建 index + 逐节点解析，确定性排序）；15 项集成测试

#### Task Closure / Semantic Paging（core/hir，语言设计第八节）
- [x] **`action_context`**（§8.1）：从 action root 沿 ASG 做邻域遍历——绑定 capability、input/output
      类型、effect 引用的 storage、errors 引用的 error（经 variant 表）、body 中调用的
      action/transition（递归）、构造的 entity、各节点所属 domain 文件
- [x] **`task_context`**（§8.2）：从 `task.include` 入口并入各自 formal 依赖；应用 `task.exclude`——
      formal 依赖（storage）被 exclude 命中则报 `ExcludedDependency`（不静默删除）
- [x] **解释边 `ContextEdge`**（§8.1 步骤 8）：`binds_capability`/`calls`/`raises`/`reads`/`writes`/
      `uses_type`/`in_domain`/`includes`，说明每个节点为何进入 context
- [x] 纯 HIR 计算（消费 AST + AsgIndex，零 IO）；输出确定性（节点 / 边 / 文件按稳定序排序去重）；
      root kind 校验（action/task）与缺失 root 报错
- [x] 7 项测试（action 闭包全邻域 / 解释边 / 确定性 / root 类型校验 / 缺失 root / task 闭包依赖 /
      exclude 命中报错）

#### Semantic IR（core/semantic，构建顺序 step 3）
- [x] **三层结构**：`type_layer` / `effect_layer` / `contract_layer`，对外统一入口 `analyze_program`
- [x] **Table 模式**（6.2）：声明信息（`SemanticModel`，不可变）与推导结果（`TypeTable`，按
      `ExprId` 索引、可重算）解耦；AST 节点不被修改
- [x] **规范化类型 `Ty`**：标量 / `List` / `Optional` / `Schema` / `Unknown` / entity / state /
      `Intent`；`Unknown`/`Error` 渐进恢复；`assignable_to` 实现 intent 严格相等（7.2）
- [x] **类型层检查**（7.6）：字段赋值 / return / 调用实参类型匹配；entity 构造全字段覆盖 /
      未知字段 / 字段类型；表达式 intent 推导（`+` 保留左侧 intent）；非 Unit 全路径 return/raise
      （Flow 终止性分析）；match 穷尽（Bool / state / Optional，禁止 `_`）
- [x] **效应层检查**（7.3）：`used ⊆ declared`（含被调用方 effect 经类型层并入 `used` 的子集传播）、
      `Pure` 与其他 effect 互斥
- [x] **契约层检查**（7.4 / 7.5）：capability deny 优先于 allow、有 effect 须绑 capability；
      `raise` variant 须声明、被调用 action 的 errors 须传播
- [x] **Intent 边界**：`Console.Write` 仅接受字面量 / `Sanitized<T>` / `Redacted<T>`
- [x] 编译器诊断（`SemanticDiagnostic`，带 span 与稳定诊断码，17 类 kind）；18 项集成测试
      （含规范 TodoDomain 子集端到端通过）

#### Execution Graph IR（core/exec-ir，构建顺序 step 4）
- [x] **`ExecGraph` / `ExecNode` / `ExecEdge`**：每 callable 一执行节点（`Action`/`Transition`），
      按名字典序构建（输出确定性）；`EdgeKind` 五类（Data/Stream/Control/Conditional/Fallback）
- [x] `from_model` 从 Semantic 声明模型构建节点；body 中对 action/transition 的调用物化为
      `Control` 调用边（起步子集无并发 / await / retry）
- [x] **解释器经 Execution Graph IR 执行**（设计 9.2 流水线 `Semantic IR → Execution Graph IR →
      Interpreter`）：解释器持有 `ExecGraph`，每次 callable 调用先经图解析（节点存在 + 调用边校验），
      使 exec-ir 成为真实桥梁而非死产物

#### 解释器（runtime，构建顺序 step 4–5）
- [x] **运行时值模型 `Value`**：标量 / List / Optional / entity 记录 / state 标记联合；
      `RaisedError`（variant tag + 字段）
- [x] **解释器 `Interpreter`**：body 子语言全覆盖（let/set/return/raise/if-else/match/repeat/print +
      表达式）；`Signal` 表达 return/raise 控制流；词法作用域环境；`match` 模式匹配与绑定
- [x] **经 Execution Graph IR 路由调用**：解释器借 `ExecGraph`，callable 调用先在图上解析
      （`ExecNode` 存在性 + `Control` 调用边），落实设计 9.2 的 `Semantic IR → Execution Graph IR →
      Interpreter` 流水线（此前 exec-ir 未被消费，已修正）
- [x] **跨文件调用解析**：持有全程序 AST，`cur_ast` 随递归调用保存/恢复（`ExprId` 仅在所属 AST 有效）；
      transition 经构造式语法调用按 input 顺序重排实参
- [x] **effect host 抽象 `EffectHost`**：v0 唯一可在运行时执行的可观测 effect 是
      `Console.Write`（经 `print` 触发），由宿主 `console_write` 处理；默认 `InMemoryHost`
      捕获 console 输出，解耦副作用便于测试。`DB.Read/Write` 可声明 / 可静态检查，但其运行时
      执行依赖 body 级 storage 操作（§16.6 扩展子集），不在 v0 解释器内，故 host 不预留死接口
- [x] **runtime input/output validation**（step 5）：在 action 边界用 entity/state/error metadata
      校验实参与返回值结构（`validate::check_value`，直接消费 Semantic 元信息，不经中间语言）；
      intent 为静态属性，运行时剥离只校验结构
- [x] 集成测试（parse→HIR→semantic→exec-ir→run 全链路：算术 / 控制流 / repeat / print 捕获 /
      state & optional match / raise / entity 构造 / 跨文件调用 / transition 调用 / 输入校验）

#### Development Graph 持久化（workflow/graph-db，构建顺序 step 6）
- [x] **基础词汇**：`NodeId`（`N0001` 规范格式，serde 为字符串）、`Provenance`、`NodeRole`（20 类）、
      `NodeCreationStatus`；`Provenance::allowed_for` 实现第二节 provenance×role 矩阵
- [x] **NodeMeta**：`#[serde(deny_unknown_fields)]`，承载 id/role/provenance/creation_status/
      created_at/summary/tags/model/prompt_artifact/response_artifact（1.2）
- [x] **20 类 payload schema**（第四节）：全部 `deny_unknown_fields`；`StateAssessment` 判别联合；
      `NodePayload` 按 role 标签的统一联合，`role()` 校验 meta.role 与 payload 一致
- [x] **边目录**（第六节）：`EdgeKind` 27 类；`allows(from_role,to_role)` 实现全部 `(from,to,type)`
      硬约束表（含 `T*` 多 role 端）
- [x] **GraphStore**（SQLite + 事件溯源）：`graph_events` append-only 表；`replay` 重建内存视图；
      **无任何 update/delete API**（N1/N2/I9）；ID 分配不复用
- [x] **append 期不变量强制**：I2（role×provenance）、I8（Failed 仅 RawLlm）、I3（边 role 约束）、
      I5（悬空引用）、I4（supersedes 同 role / 不成环 / 单出边）；payload 字段约束
      （非空、Pseudocode artifact_path、ContextSnapshot digest 64 位 hex、Decision confidence∈[0,1]、
      Clarification kind↔provenance）
- [x] **I6 整体校验** `validate_i6`：LLM-provenance 的 Decision/Pseudocode/Code/Assessment 必须有
      `consumed→ ContextSnapshot` 边（作为收尾不变量检查，允许节点先建边后补的写入顺序）
- [x] 16 项测试（含事件溯源 replay 持久化往返、各不变量负路径）

#### Active Context 推导（workflow/graph-db，构建顺序 step 7）
- [x] **`ActiveContext` 与 `*View` 类型**：仅暴露字段子集（id + 关键字段），不泄漏 NodeMeta 全量
- [x] **binding 谓词**（5.2）：链头 + （human 隐式接受 ∨ 链上有 AcceptanceEvent），
      且无更晚 WithdrawalEvent；版本链 `chain_of` / `head_of_chain` 沿 supersedes 导航
- [x] **binding 继承**（5.3）：沿 `member_of` / `groups` / `requires` 单向传播（Decomposition → 子目标、
      Milestone → groups 目标与 requires 不变量）
- [x] **active milestone**（5.4 步骤 5）：bound milestone 中最新 `ActivationEvent` 指向者
- [x] **聚合**：bound_constraints（active milestone requires/excludes + bound objective constrained_by）、
      bound_acceptance_criteria（validated_by）、open_change_requests（无接受无撤销）、
      outstanding_questions（未被 answers）
- [x] **稳定序列化 + digest**：集合按 NodeId 排序、字段固定顺序，SHA-256 lower-case hex（I10 确定性）
- [x] **`snapshot_payload` helper**：推导 → 装入 `ContextSnapshotPayload`，digest 与内容同源一致
- [x] 11 项测试（binding / 接受 / 撤销 / supersedes 链头 / 继承 / active milestone / 约束 /
      change request / 问题 / digest 确定性 / snapshot 通过 store 校验）

#### 节点工厂层（workflow/graph-db，构建顺序 step 8–12）
- [x] **provenance 分组工厂**（N6）：`as_human` / `as_llm` / `as_deterministic` 三个创建路径入口；
      `GraphStore::append_node` 降为 crate 私有原语，外部无法自由设定 / 伪造 provenance
- [x] **HumanFactory**：objective / constraint / acceptance_criterion / milestone / change_request /
      acceptance_event / withdrawal_event / activation_event / answer（Clarification kind=Answer）
- [x] **LlmFactory**：objective / constraint / acceptance_criterion / decomposition（step 8 小节点）/
      milestone / assessment / first_slice / question（kind=Question）/ decision / pseudocode / code /
      raw_llm（强制 Failed）
- [x] **DeterministicFactory**：context_snapshot / baseline_decision / diagnostic（step 12，5 类 kind）/
      selection / materialize
- [x] 每个入口固定 provenance 与（必要时）creation_status / Clarification kind，编译期封住伪造路径
- [x] 6 项工厂测试（各路径 provenance 固定、question/answer kind、raw_llm Failed、baseline decision）
      + provenance×role 矩阵逻辑单测

#### LLM 抽象与结构化输出（workflow/llm，构建顺序 step 13）
- [x] **`LlmClient` trait**：模型无关后端抽象，只需实现自由文本 `complete`（单一路线）；
      `CompletionRequest`（model/system/prompt，`with_repair_hint`）/ `CompletionResponse`
- [x] **`complete_structured`**：JSON 提取（容忍前后说明文字）+ `jsonschema` 严格验证
      （`additionalProperties:false`）+ 重试 fallback（携带错误信息重试，超次数返回结构化错误，
      **不伪造成功**）；后端不可用立即上报不重试
- [x] **`LlmError`**：变体与 `RawLlmFailureKind`（4.4.8）对齐，失败由上层 emit `RawLlmNode`
- [x] **具体后端 `HttpLlmClient`**（reqwest）：两种模式 `BackendMode::{OpenAiCompatible, Ollama}`，
      共享 system+user message 形状；OpenAI 走 `/chat/completions`、Ollama 走 `/api/chat`
      （`stream:false`）；非 2xx / 网络错误 → `BackendUnavailable`（绝不伪造成功）
- [x] 13 项测试（6 结构化 fallback + 7 后端：endpoint / message 构造 / 响应解析）

#### Prompt 模板管理（workflow/prompt，构建顺序 step 13）
- [x] **`PromptRegistry`**（minijinja）：内嵌 6 个模板（design_solution / implement_design /
      repair_code / revise_design / decision / decompose），Strict undefined（缺变量报错不静默）
- [x] **6 个 JSON Schema**（design_result / implement_result / decision_node / pseudo_check /
      repair_result / decompose_result），`schema_for` 取用；均 `additionalProperties:false`
      （每个工作流步骤一个 schema；`pseudo_check` schema 就绪、检查命令待接入）
- [x] 11 项测试 + 4 个 insta 渲染快照（守护模板 / 语法基线变更不静默影响 LLM 行为）

#### 评估拆解协议（workflow/graph-db，构建顺序 step 11）
- [x] **`AssessmentLlmOutput` / `AssessmentSelfCheck`**（4.2.2）：strict schema 的 LLM 输出契约
      （`#[serde(flatten)]` 内联 head；`deny_unknown_fields`）
- [x] **`decompose_assessment`**：确定性 helper 把 LLM 输出拆为多节点 + 边——
      Assessment（`assesses→` 被评估对象）、可选 FirstSlice（`proposes→`）、
      0..N 个 Constraint(Invariant)（`proposes→`，强制 kind=Invariant）、Decision（`proposes→`，
      change-kind state assessment）；各 LLM 节点连 `consumed→ ContextSnapshot`（I6）
- [x] self-check 全真才拆解（否则拒绝，视为无效评估）
- [x] 7 项测试（最小拆解 / 全量拆解 / self-check 失败拒绝 / 非 Invariant 拒绝 / Decision 形状 /
      strict schema 拒多余字段 / flatten 解析）

#### LLM 调用编排（workflow/engine，构建顺序 step 13）
- [x] **`run_llm_step`**：固化 §7 接入点——① 调用前由 active context 确定性建 `ContextSnapshot`；
      ② `complete_structured` 调用；③ 成功返回值 + snapshot（调用方建下游节点连 `consumed→`），
      失败 emit `RawLlmNode`（`attempted→ target`，failure_kind 由 `LlmError` 映射）
- [x] 失败路径也先建 snapshot（保证可审计 / 可复现）；**不伪造成功**
- [x] 新增 `workflow/engine` crate（依赖 graph-db + llm + prompt）；分层：持久化层不反向依赖 LLM
- [x] 4 项测试（成功建 snapshot / 后端不可用兜底 + attempted 边 / schema 失败兜底 / 失败也建 snapshot）

#### 工作流执行闭环、implement-loop 与 Selection/Materialize 编排（workflow/engine，构建顺序 step 13+ / step 14 配套）
- [x] **design / implement / repair 执行步骤**（`loop_steps`）：在 `run_llm_step` 之上把单步
      LLM 调用串成图产物——`design_solution` 建 `PseudocodeNode`（`addresses→` 目标域）；
      `implement_design` 建 `CodeNode`（`addresses→` 目标域 + `implements→ Pseudocode`）；
      `repair_code` 建新 `CodeNode`（`addresses→` 目标域 + `repairs→` 旧 Code）
- [x] 产物正文随 outcome 返回（`PseudocodeArtifact.text` / `CodeArtifact.files`）：图节点不存正文
      （4.4.3/4.4.4），但下游 gate 与物化需要正文，故由工件类型交给调用方（落盘 / 喂 gate）
- [x] 「动作选择与执行分离」的**执行**侧：每个 LLM 节点的 `consumed→ ContextSnapshot` 由
      `run_llm_step` 建立（I6）；任一步失败返回 `LoopStepOutcome::Failed`（已 emit RawLlmNode +
      `attempted→` 目标域），**不伪造成功**——闭环中止交由调用方决定后续
- [x] 图结构前置校验：`addresses→` 目标域限 Objective/Milestone/FirstSlice；implement 的源须
      Pseudocode、repair 的前序须 Code；CodeNode 只记文件路径、正文由上层物化（4.4.3/4.4.4）
- [x] **implement-loop**（`implement_loop`，对应 CLI `sophia graph implement-loop`，架构 §9.2）：
      预算受限的 implement → code_check → repair 收敛循环——首次 implement → 注入的确定性
      code_check（[`CodeChecker`]，kind 必须 CodeCheck）→ emit `DiagnosticNode`（`checks→ Code`）→
      `ok` 即返回通过候选；否则在 `max_repair_attempts`（design 10.9）预算内据诊断重渲染
      `repair_code` 模板并 `repair_code`，预算耗尽返回 `BudgetExhausted`（保留最后候选 + 诊断节点）
- [x] implement-loop 分层：engine 不自行运行 checker（属 tools 层），check 结果由调用方注入——
      与 materialize 消费 `GateReport` 同构；LLM 失败仍走 RawLlmNode 兜底
- [x] **Selection/Materialize 节点编排**（`select_materialize`）：消费 `tools/materialize` 的
      `CodeCandidate<Selected>`（类型层已保证经全部 gate），建 `SelectionNode`（`selects→ Code`）
      → 原子物化写盘 → 建 `MaterializeNode`（`materializes→ Selection`，payload 记逻辑根 +
      相对文件列表，不写机器相关绝对路径以保持确定性）
- [x] 分层纪律：engine 依赖 graph-db + llm + prompt + materialize；物化原子写入仍由
      materialize crate 负责，graph 节点由编排层在 gate 通过后单独创建
- [x] 17 项测试（design→implement→repair 闭环 + I6 守护 / design 失败兜底无 Pseudocode /
      implement 拒非 Pseudocode 源 / design 拒非目标域；implement-loop 一次通过 / 修复后通过 /
      预算耗尽 / implement 失败上浮 / 拒错误诊断 kind；Selection/Materialize 全流程 + 边 + 写盘 /
      拒非 Code 目标 / 编排后 I6 仍成立 / 多文件物化）

#### 工作流总调度器（workflow/engine，构建顺序 step 13+）
- [x] **`run_goal_loop`**（`scheduler`）：据 DecisionNode 驱动的 goal 推进循环——每轮先
      `run_llm_step` 取结构化 `DecisionPayload`，emit `DecisionNode`（`considers→ 焦点` +
      `consumed→ snapshot`），再据 `selected_action` 分派（design 10.8「动作选择必须由 LLM 产生」）
- [x] **执行委派**：`design_solution` → Pseudocode（记为当前版本）；`implement_design` 用当前
      Pseudocode 跑 `run_implement_loop`（implement→check→repair），通过即 `CandidateReady` 交回
      调用方做 select/materialize
- [x] **预算强制**（design 10.9 顶层子集）：`max_decisions`（≈max_depth）、`max_pseudocode_versions`、
      `max_total_llm_nodes`（max_total_nodes_per_goal 的 LLM 子集），超限 `BudgetExhausted`
- [x] **物化是显式收尾**：`select`/`materialize` 等不可逆写盘不在调度器内自动执行（design 10.10
      唯一写 `domains/` 路径），通过 gate 的候选经 `CandidateReady` 交回调用方
- [x] **高层动作让位**：`decompose`/`backtrack`/`revise_design`/`needs_clarification` 等图操作语义
      超出本 spine，spine 以 `Yielded` 交回，**不擅自臆造语义**（单一路线）。其中 `revise_design` /
      `needs_clarification` 已在 spine 内接入；`decompose` / `backtrack` 由 spine 之上的独立**目标树
      遍历层**（`engine::run_goal_tree`，构建顺序 step 13+，architecture §8.5）承接执行

#### 目标树遍历层（workflow/engine `traversal`，构建顺序 step 13+）
- [x] **`run_goal_tree`**：在 spine 之上驱动非线性目标树——spine 让位 `Decompose` → 执行
      `decompose_goal`（LLM 拆解结构 → 确定性 `graph-db::build_decomposition` 落图）后**递归**深度
      优先驱动每个子目标；让位 `Backtrack` → 记录放弃分支（`GoalResolution::Backtracked`）
- [x] **拆解审查者 `DecompositionReviewer`（人类授权检查点，design 5.3 / N4）**：decompose 落图后、
      递归子目标前回调——`Accept` → 建**真实** human `AcceptanceEvent accepts→ Decomposition`，子目标
      沿 `member_of` 继承 binding 进入各自 active context；`Reject` → 不递归 / 不伪造 withdrawal
      （`GoalResolution::DecompositionRejected`）。提供 `AutoAcceptReviewer`（调用方代表人类授权，仍
      走真实 AcceptanceEvent 落图，非绕过 binding 谓词）。引擎不伪造人类授权，授权权威留在调用方
- [x] **`graph-db::build_decomposition`**（确定性 helper）：建 `Decomposition`、`parent decomposes→
      Decomposition`、每个子目标建 `Objective` 并 `member_of→ Decomposition`；拒非 Objective 父、
      拒 <2 子目标
- [x] **诚实性硬约束**：`Decomposition` 是 decompose 的 **LLM 执行产物节点**（承载 LLM 生成的拆解
      结构），故**自身** `consumed→ ContextSnapshot`（I6，与 Pseudocode / Code / Assessment 同构），
      锚定在产出本次拆解的 LLM 调用快照上，而非触发它的 DecisionNode（"该不该拆"是另一次调用，
      §10.8 动作选择 / 执行分离）；`build_decomposition` 接收并校验 snapshot 参数。子 `Objective`
      是结构性派生节点，经 `member_of` 间接锚定，不单独 `consumed→`。`backtrack` 不伪造
      `WithdrawalEvent`、binding 不伪造（撤销 / 接受是人类权威 N4，LLM 派生子目标默认未绑定，人类
      接受 Decomposition 后才沿 `member_of` 继承 binding，spec 5.3）
- [x] **`TreeBudget`**：`max_depth`（decompose 嵌套深度）+ `max_goals`（spine 调用总数）防递归爆炸；
      每目标 spine 推进仍受 `SchedulerBudget` 约束
- [x] 6 项 graph-db decomposition 测试（建节点 + consumed→ snapshot / 连边 / 拒非法父 / 拒非
      snapshot 锚点 / 拒过少子 / 接受后 binding 继承）+ 5 项 engine traversal 测试（拆解后各子归结为
      候选含 I6 校验 / 叶子直接归结 / backtrack 放弃不伪造撤销 / 深度上限 / 目标总数上限）
- [x] 审核修正：`decision_node.json` schema 此前仅 require `state_assessment.kind`，比
      `StateAssessment` 判别联合宽松（schema 通过但反序列化可能失败）——已收紧为 `oneOf` 三 kind
      各自 required 全字段（strict 模式 1.3：schema 是忠实契约）
- [x] 分层纪律：确定性 code_check 由调用方注入（`CodeChecker`），调度器不自行运行 checker；
      prompt 上下文抽取属 CLI 协调层，请求与 schema 由调用方注入（`StepRequests`）
- [x] 7 项测试（design→implement 产候选 + considers 边 + I6 / 无伪代码 implement 让位 / 高层动作
      让位 / decision 轮数预算 / 伪代码版本预算 / decision 后端失败兜底 / 拒非法焦点）

#### Materialize Gate（tools/materialize，构建顺序 step 14）
- [x] **类型状态链**（impl §15）：`CodeCandidate<S>`，状态 `Unchecked → CheckPassed → AuditPassed →
      RuntimeValidated → Selected`；`materialize` 仅在 `Selected` 上存在
- [x] **gate 条件**（design 10.10）：code_check → constraint_audit → artifact_diff（strip-assist 等价）
      + runtime input/output validation → select；各 gate 消费确定性 `GateReport`（不重复实现检查）
- [x] **编译期 gate 保证**：`compile_fail` 文档测试证明跳过 gate 直接 materialize 无法编译
- [x] **原子写入**：先写隐藏 staging 目录，全部成功后 rename 替换目标；失败清理不污染 `domains/`；
      拒绝绝对路径与 `..` 逃逸
- [x] 9 项集成测试 + 2 项文档测试（全流程物化 / 各 gate 失败中止 / 多文件 / 路径逃逸拒绝）
- [x] 分层纪律：不依赖 workflow 图（MaterializeNode 由编排层在 gate 通过后单独创建）

#### Language Server（lsp，构建顺序 step 15）
- [x] **协议无关分析核心 `Workspace`**（基于 semantic data，10.3）：多文档解析 → ASG index +
      符号表（module/symbol cache）；query 风格接口为后续增量分析预留
- [x] **诊断**：syntax + hir + semantic 三层合并；**按文档精确归属**——HIR 用 `resolve_item`、
      semantic 用新增 `analyze_one_callable` 逐 item/callable 收集，规避跨文档 0 基 span 碰撞
- [x] **hover**：返回光标处标识符对应符号的 kind 与定义位置（基于符号表）
- [x] **goto definition**：解析光标处标识符到其顶层符号定义（支持跨文档）
- [x] **span ↔ LSP 位置换算**：字节偏移 ↔ 0 基行 + UTF-16 列（正确处理多字节字符）
- [x] **tower-lsp 外壳**：didOpen/didChange(FULL)/didClose + publishDiagnostics + hover + definition；
      `initialize` 声明 capabilities；`run_stdio` 入口
- [x] 9 项分析集成测试（含跨文档诊断归属、跨文档 goto）+ 3 项位置换算单测

#### 确定性检查器（tools/check）
- [x] **`check_program`**：组装 HIR 名称解析 + 语义三层 + strip-assist 门禁，返回结构化 `CheckReport`
- [x] **strip-assist 等价门禁**（design 5.1）：`Ast::strip_assists` 移除全部 Semantic Assist
      （meaning/not/... 及 entity 的 semantic_identity / evolution），比对移除前后的 **Semantic IR
      指纹**（声明模型 `formal_fingerprint` + 语义三层诊断输出），不一致即报首个差异行
- [x] `SemanticModel::formal_fingerprint`（确定性 `Debug`、无 span、无 assist）作为形式核心指纹
- [x] 已接入 CLI `sophia check`（sophia.toml `require_strip_assist_equivalence` 落地）
- [x] 7 项测试（干净通过 / 丰富 assist 等价 / state value assist / 语义诊断 / HIR 诊断 / diff 定位）

#### Constraint audit（tools/audit）
- [x] **`audit_constraints`**：对约束集合做审计，产出结构化 `AuditReport`（对齐 Diagnostic
      kind=ConstraintAudit / RegressionGate，workflow_graph_spec 4.4.5）
- [x] **regression gate 规则**（4.1.2 / 第七节 4）：仅 `Invariant` + 可执行 verifier
      （HiddenCase / AuditRule）驱动 gate（由注入的 `VerifierOutcome` 决定 Pass/Fail）；
      Manual / 无 verifier / 非 Invariant → Skipped（仅上下文）；声明了可执行 verifier 却缺结果 → 硬错误
- [x] 分层纪律：tools 层不依赖 workflow 图与运行器；verifier 执行结果由确定性管线注入
      （与 materialize 消费 `GateReport` 同构）
- [x] 7 项测试（invariant pass/fail、非 invariant 跳过、manual 跳过、无 verifier 跳过、缺结果硬错误、
      混合约束仅报 invariant 失败）

#### `effect` 顶层声明（core，语言设计第十三节 / 实现第二十节）
- [x] **`effect` 顶层构造**：`effect Family { operation Op { param... } }`；grammar `effect_def` +
      `effect_operation` + `effect_param`；AST `EffectDef`；lowering
- [x] **通用 effect 引用**：`effect_ref`（`Family.Op` / `Family.Op(args)` / `Pure`）取代硬编码分支；
      AST `Effect::{Pure, Op{family,op,args}}` + `EffectArg`（单一路线，移除旧 4 变体）
- [x] **HIR effect 符号表**：`AsgIndex::effect_ops`（`Family.Op → arity`），内置族
      `Console/DB` 由 `builtins::BUILTIN_EFFECT_OPS` 预置 + 用户 `effect` 声明并入；
      名称解析校验引用已声明、arity 相符（`UnresolvedEffect`）；`NodeKind::Effect` 入 index
- [x] **semantic 三元组表示**：`Effect=(family,op,args)`，`EffectArg::{Lit,Binding}`；capability 匹配
      `covered_by`——字面量须相等、绑定名通配（保 `DB.Read("A")≠DB.Read("B")`）
- [x] 测试：syntax lowering（effect 声明 / strip-assist）+ HIR effect 解析（含用户领域 effect）+
      semantic effect/capability 检查；CST 快照用 `effect_ref`

#### 调度器高层动作与目标树遍历层（workflow/engine，构建顺序 step 13+）
- [x] **调度器 prompt 调用时刻渲染**（`StepPrompts` 提供者取代静态 `StepRequests`）：`run_llm_step`
      收 prompt 渲染闭包（与 ContextSnapshot 用同一份 active context 渲染，§10.7 同源）；
      `design_solution`/`implement_design`/`repair_code` 收 `FnOnce(&ActiveContext) -> CompletionRequest`
      渲染器、schema 按步固定取内置；`run_implement_loop`/`run_goal_loop` 收 `&impl StepPrompts`
      （`prompts` 模块定义 trait + `GoalProgress`）。静态 `StepRequests` 整体移除（单一路线）。
      设计见 `engineering_architecture.md` §8.4
- [x] **`revise_design` / `needs_clarification` 接入 spine**：implement-loop 预算内未过 check 回到
      decision（`Dispatch::ImplementExhausted` + `GoalProgress.last_implement_failed`），LLM 可选
      `revise_design` 重写伪代码（建新 Pseudocode + `revises→` 旧，使 revise 可达，design 10.8 原则 3）；
      `needs_clarification` 真正 emit `Clarification(Question)` + `asks_about→ 焦点` 再让位
- [x] **目标树遍历层 `run_goal_tree`**（`traversal`，承接 decompose / backtrack，design 10.9 不塞进
      spine，architecture §8.5）：spine 让位 `Decompose` → 执行 `decompose_goal`（LLM 拆解结构 → 确定性
      `build_decomposition` 落图）后**递归**深度优先驱动每个子目标；让位 `Backtrack` → 记录放弃分支
      （`GoalResolution::Backtracked`）。`TreeBudget`（max_depth / max_goals）防递归爆炸
- [x] **`graph-db::build_decomposition`**（确定性 helper）：建 `Decomposition`、`parent decomposes→
      Decomposition`、每个子目标建 `Objective` 并 `member_of→ Decomposition`；拒非 Objective 父、
      拒非 snapshot 锚点、拒 <2 子目标。诚实性硬约束：`Decomposition` 是 decompose 的 **LLM 执行产物
      节点**，故**自身** `consumed→ ContextSnapshot`（I6，与 Pseudocode / Code / Assessment 同构），
      锚定在产出本次拆解的 LLM 调用快照、而非触发它的 DecisionNode（§10.8 动作选择 / 执行分离）；
      子 `Objective` 经 `member_of` 间接锚定不单独 `consumed→`。`backtrack` 不伪造 `WithdrawalEvent`、
      binding 不伪造（撤销 / 接受是人类权威 N4，人类接受 Decomposition 后才沿 `member_of` 继承，5.3）
- [x] 审核修正：`decision_node.json` schema 收紧为 `oneOf` 三 state_assessment kind 各自 required 全
      字段（此前仅 require kind，schema 通过但反序列化可能失败，违反 strict 模式忠实契约 1.3）
- [x] 9 项调度器测试 + 6 项 graph-db decomposition + 5 项 engine traversal 测试

#### 多候选评分排序（tools/materialize + workflow/engine，design 10.9 score 块）
- [x] **`score` 模块**：`score_candidate` / `rank_candidates` / `Score` / `ScoreInputs` / `ScoreWeights`
      ——七维度加权和（compile / tests / constraints 取自 gate 报告真实信号；simplicity / locality /
      capability_minimality 由候选源码可度量结构性属性按明确公式计算；pseudocode_clarity 调用方有信号
      才提供、否则中性 0.5，不伪造），硬约束 `compile=0 → overall≤0.49`、确定性平局打破（按下标升序）
- [x] **engine `run_ranked_selection`**：多候选排名 → 选 winner → 建 `SelectionNode`（rationale 记评分
      摘要）。评分是内存启发式，**不入图**（spec 无 Score role）。7 项 score 单测 + 2 项 engine ranked 测试

#### 执行 Trace 投影（core/exec-ir + runtime，impl §9.4）
- [x] **稳定 `ExecEdgeId(u32)`**：`core/exec-ir` 为执行图边引入稳定 ID（`add_edge` 返回 ID，
      `call_edge_id` / `edge` 查询）——trace 投影的前置条件
- [x] **`runtime/trace`**：`Trace` / `ExecutionSpan` / `SpanOutcome`，解释器每次 callable 进入开一条
      span（pre-order）、执行完写回结局，`run_action` 返回 `Execution { outcome, host, trace }`。span 携带
      `node_id` /（触发它的）`edge_id`（顶层入口 None）/ `depth` / `outcome`。确定性优先：不记墙钟时长
      （破坏可复现），只记图结构投影与进入序 `seq`；LLM 计量（tokens/cost）待 LLM 执行节点引入
- [x] **CLI `sophia run --trace`**：呈现投影（按 depth 缩进 + 节点 / 边 ID + 结局）。4 项 trace 测试 +
      1 项 CLI `--trace` 集成测试

#### Constraint audit verifier 执行 + 隐藏验证用例存储（runtime + tools/audit + cli，spec 五A）
- [x] **hidden case 执行器**（`runtime/verify`）：`HiddenCase` / `ExpectedOutcome`（Returns / Raises）/
      `run_hidden_case` / `run_hidden_cases`——在 v0 解释器上**真正执行** hidden case 并与期望比对，
      产出 `VerificationResult`（passed + detail）；执行硬错误判 fail 不伪造
- [x] **隐藏验证用例存储**（CLI `verifier_store`）：hidden case 正文存于图外的
      `sophia-runs/verifiers/hidden.json`（`ref → HiddenCase`），与 dev_graph 物理隔离、绝不入 active
      context。三层 anti-cheat 隔离：① 图节点只存不透明 `verifier.ref`；② `ConstraintView` 整体剔除
      verifier（spec 5.6）；③ 正文在图外。`runtime::{Value, HiddenCase, ExpectedOutcome}` 加 serde
      （单一值模型，不另设镜像）
- [x] **gate 自动驱动**：`run_constraint_audit` 从 **ConstraintNode 原始 payload**（非 `ConstraintView`）
      读 `verifier.ref` → `run_hidden_verifiers` 在候选上构模型 + `runtime::run_hidden_cases` 真正执行 →
      零损耗映射 `VerifierOutcome` 注入 `audit_constraints`；缺用例 → 不注入 → audit
      `MissingVerifierOutcome` 硬错误阻断。分层守恒：执行属 runtime、判定属 tools/audit、加载+串联+写图
      属 CLI 协调层。6 项 runtime verify 测试 + 4 项 CLI 集成测试

#### CLI 便利命令（cli，架构 §9.1，确定性、不调用 LLM）
- [x] **`sophia smoke`**：一键串联 init（幂等）→ check → build → run（`--action <Name>` 可选，省略则
      只做 check/build），任一步失败即以失败退出码中止（忠实反映，不伪造通过）
- [x] **`sophia repair-context --error <code>`**：为 LLM 修复循环生成结构化上下文（impl 14.3），从
      `check` 同口径诊断里按诊断码子串筛选，对每条给出归属文件 + 1 基位置 + 诊断码 + 信息 + 该
      action/transition 的 action-rooted 语义闭包（相关节点 / 文件），**不臆造修复建议**（具体改法是 LLM
      职责）。`commands::collect_diagnostics` 抽出供 `check` / `repair-context` 共用。5 项 CLI 集成测试

#### 快照测试基础设施
- [x] 已用于 CST / prompt 渲染 / prompt 资产；扩展到 **HIR**（ASG index JSON）、**Semantic IR**
      （`formal_fingerprint`）、**Execution Graph IR**（节点 + 调用边结构），各加 1 项 `insta` 快照守护
      核心 IR 产物不被静默改动

#### body 级 storage 操作（core/semantic + runtime，§16.6 storage 扩展子集）
- [x] **type 层**（`type_layer::infer_storage_op`）：识别 `storage.<Name>.get(key)` /
      `.save(key, value)` 形状——`get → Optional<ValueTy>`、`save → ValueTy`（v0 不引入 `Result<T,E>`，
      `save` 直接返回 value）；并入 effect `DB.Read("<Name>")` / `DB.Write("<Name>")`（走既有
      `used ⊆ declared` + capability 检查，与声明式 effect 同一路径）；校验 key / value 实参类型与
      storage 声明相容；未知 storage / 未知操作报诊断
- [x] **runtime**（`interp::try_storage_op` + `effect_host`）：解释器识别同一形状，经 `EffectHost`
      （`storage_get` / `storage_save`）委派；默认 `InMemoryHost` 用按 storage 名分桶的内存 key→value
      映射执行；key 显式传入（「全部显式表达」，不靠 entity 字段名约定隐式推 key）
- [x] 7 项测试（runtime：save→get 往返 / get 缺失键返回 None / 同键覆盖；semantic：合法 storage op
      通过 / 未声明 DB effect 报错 / key 类型不符报错 / 未知 storage 报错）+ CLI 端到端实测
      （check 通过 + `run --trace` 返回 42）

### 2.2 部分完成

- [ ] **`graph design` 的 `context_files` 接入**：现 `graph design <ObjectiveId>` 在 Development Graph
      上工作，而 `context` 闭包从项目源 action/task root 计算——两者 root 不同；把 graph Objective
      关联到项目 action 的链接尚未建模，故 design 的 `context_files` 暂仍诚实留空（不臆造）

### 2.3 尚未完成

#### core
- [ ] **Execution Graph IR 调度扩展**（step 4+）：并发 / await / retry / cancellation / checkpoint
      等更丰富调度与边语义（起步子集仅建 callable 执行节点 + Control 调用边；Data/Stream/Conditional/
      Fallback 边为枚举占位，无表层来源故不物化）

#### runtime
- [ ] Tokio substrate 接入（引入网络 / 文件等真实异步 effect 时）

#### workflow / CLI
- [ ] `graph` 工作流子命令补全：`decision` / `assess` 等（`init` / `start` / `context` / `nodes` /
      `design` / `implement-loop` / `select` / `materialize` 已完成——`start → design →
      implement-loop → select → materialize` 端到端贯通）

#### lsp
- [ ] **LSP 扩展**（step 15+）：rename / autocomplete / semantic navigation；增量分析（Salsa 化）

#### 工程
- [ ] CI 流水线接入（自动跑 fmt / clippy / test；本地命令已就绪，见第三节）

### 2.4 计划完成（路线图，起步子集之后）

- [ ] **v1：WASM codegen**（entity/state/error → type section + metadata；action → wasm function；effect → host import）
- [ ] **v1：strip-assist WASM artifact 字节级比对**
- [ ] **增量分析**：基于 Salsa 思想的查询缓存（接口已用 query 风格预留）
- [ ] **MessagePack 序列化**：graph snapshots / runtime state / semantic cache
- [ ] **Formatter**：AST/HIR → pretty printer，确定性输出
- [ ] **Semantic Identity / Evolution Boundary** 的 entropy/演化检查
- [ ] **transition 合约证明 / `Result<T,E>` / 跨 domain boundary / entity.with** 等扩展子集
- [ ] v2 起可选 backend（native cranelift/LLVM；按需的具名语言 emit）

---

## 三、验证方式

- 构建：`cargo build --workspace`
- 测试：`cargo test --workspace`
- Lint：`cargo clippy --workspace --all-targets`
- 格式：`cargo fmt --all -- --check`
- 语法层手验：`cargo run -p sophia-cli -- parse <file.sophia>`

---

## 四、变更记录

- 2026-05-29 — 初始化工程进展文档；记录工作区/语法层完成项与占位现状。
- 2026-05-29 — 完成构建顺序 step 1：AST 数据模型（Arena + `ExprId`）与 CST → AST lowering；
  新增 `parse_ast` / `SyntaxTree::to_ast` 稳定入口与 13 项 lowering 集成测试。语法层标记为已完成。
- 2026-05-29 — 审核修正：`first_named_child` 此前会返回注释（trivia）节点，违反“CST → AST 丢弃 trivia”
  约束（impl §4），已修复并加回归测试（注释出现在表达式内部的场景）。
- 2026-05-29 — 完成构建顺序 step 2：HIR 名称解析 / 模块解析 / scope。新增 `AsgIndex`（含 variant
  成员表）、内置名字表、容错诊断、`resolve_program` 入口与 15 项集成测试。HIR 层标记为已完成。
- 2026-05-29 — 审核修正：HIR `Resolver.diags` 字段的文档注释误描述为“task include 白名单”，
  已更正为“容错收集的诊断”。
- 2026-05-29 — 完成构建顺序 step 3：Semantic IR 三层（type / effect / contract）。新增规范化类型
  `Ty`、effect 代数、`SemanticModel` 声明视图、`TypeTable` 推导表、`analyze_program` 入口与 18 项
  集成测试（含规范 TodoDomain 子集端到端）。Semantic IR 标记为已完成。
- 2026-05-29 — 审核修正（Semantic IR）：① `ensures` 中 `output` 应为以 output 参数为字段的记录
  （`output.<param>.<field>`，设计第五节），原实现误用单输出类型导致 `NoSuchField` 误报，新增
  `Ty::Record` 修正；② output 的 `where` 谓词作用域此前缺少 output 参数自身，已修正；
  ③ `set` 赋值此前不校验值类型与变量声明类型相容，已补 `check_assignable`。三项均加回归测试。
- 2026-05-29 — 完成构建顺序 step 4–5：Execution Graph IR（`ExecGraph`）与解释器。新增运行时值模型、
  `EffectHost` 抽象（默认 `InMemoryHost`）、`Interpreter`（body 子语言全覆盖、跨文件调用、
  transition 构造式调用）、runtime input/output validation。新增 14 项测试（13 解释器 + 1 exec-ir）。
  exec-ir 与 runtime 标记为已完成（起步子集范围）。
- 2026-05-29 — 审核修正（runtime）：被调用方 raise 的领域错误此前在调用方 `run` 边界被当作硬错误
  `RuntimeError::Raised` 返回，偏离错误代数（§7.5 / §16.3，raise 应作为领域结果向上传播）。
  已在 `run` 边界把内部 `RuntimeError::Raised` 通道物化为 `Outcome::Raised`，并加回归测试。
- 2026-05-29 — 完成构建顺序 step 6：Development Graph 持久化（workflow/graph-db）。SQLite + 事件溯源
  `GraphStore`（仅 append、无 update/delete）、20 类节点 payload schema（strict）、27 类边目录与
  `(from,to,type)` 硬约束、append 期不变量 I2/I3/I4/I5/I8 强制、整体 I6 校验。新增 16 项测试
  （含 replay 持久化往返）。graph-db 标记为已完成。
- 2026-05-29 — 审核修正（graph-db）：边的 payload 级约束（§6.1）此前缺失——`answers` 需
  Answer→Question、`asks_about` 需从 Question 发出、`requires`/`excludes` 的目标需特定 Constraint kind。
  这些依赖 payload 而非仅 role，已在 store 层新增 `validate_edge_payload` 强制，并加 3 项回归测试。
- 2026-05-29 — 完成构建顺序 step 7：Active Context 推导。新增 `ActiveContext` / `*View` 类型、
  binding 谓词与继承、active milestone、约束 / 验收 / change request / 问题聚合、稳定序列化 +
  SHA-256 digest、`snapshot_payload` helper。新增 11 项测试。active context 标记为已完成。
- 2026-05-29 — 审核修正（active context）：binding 继承此前为单次快照，漏掉传递链
  （bound Decomposition → member Milestone → groups Objective，§5.4 步骤 4 要求后续阶段读取已更新的
  bound 集合）。已改为不动点迭代（与处理顺序无关），并加传递继承回归测试。
- 2026-05-29 — 完成构建顺序 step 8–12（节点创建侧）：provenance 分组工厂层强制 N6——
  `as_human` / `as_llm` / `as_deterministic` 三入口，`append_node` 降为 crate 私有。覆盖小节点 /
  目标 / milestone / 事件 / 评估族节点 / DiagnosticNode 的创建。新增 6 项工厂测试。step 8/9/10/12
  标记已完成，step 11 评估拆解协议待 LLM 接入（schema 已就绪）。
- 2026-05-29 — step 13（核心）：LLM 抽象层与结构化输出 fallback（`LlmClient` / `complete_structured`，
  重试 + jsonschema 严格验证 + 不伪造成功）、Prompt 模板管理（minijinja 5 模板 + 3 schema +
  insta 快照）。新增 13 项测试。step 13 标记部分完成；具体后端与调用编排（建 snapshot / emit
  节点）待网络层与 CLI 接入。
- 2026-05-29 — 完成构建顺序 step 14：Materialize Gate 类型状态链（`CodeCandidate<S>`：Unchecked →
  CheckPassed → AuditPassed → RuntimeValidated → Selected）与原子写入（staging → rename）。
  `compile_fail` 文档测试锁定「跳过 gate 即编译错误」。新增 9 集成 + 2 文档测试。
  tools/materialize 不依赖 workflow 图（节点编排由上层负责）。step 14 标记已完成。
- 2026-05-29 — 完成构建顺序 step 15：Language Server。协议无关分析核心 `Workspace`（多文档 →
  index + 符号表）、三层诊断按文档精确归属（新增 semantic `analyze_one_callable` 支撑）、hover、
  goto definition、span↔UTF-16 位置换算、tower-lsp 外壳（`run_stdio`）。新增 12 项测试。
  step 15 标记已完成；rename/autocomplete 与增量分析列为后续扩展。至此构建顺序 1–15 全部就绪
  （step 11 评估拆解协议、step 13 具体后端 / 调用编排等编排项待网络与 CLI 接入）。
- 2026-05-29 — CLI 确定性命令端到端接通：`init` / `parse` / `index` / `graph` / `check` / `build` /
  `run`。新增 `project`（文件扫描）/ `render`（诊断呈现）/ `commands` 模块；check 诊断按文件精确归属；
  run 经 check gate 后解释执行并呈现返回值 / raise。新增 6 项端到端集成测试。`asg_index.json`
  产物与 17.2 规范一致。
- 2026-05-29 — 完成 `tools/check`：`check_program` 组装 HIR + 语义三层 + strip-assist 等价门禁。
  新增 `Ast::strip_assists` 与 `SemanticModel::formal_fingerprint`；门禁比对移除 assist 前后的
  Semantic IR 指纹（声明模型 + 语义诊断），已接入 `sophia check`（落地 sophia.toml
  `require_strip_assist_equivalence`）。新增 7 项测试。tools/check 标记已完成；audit 待办。
- 2026-05-29 — 完成 `tools/audit`：`audit_constraints` 约束审计 / regression gate。仅 Invariant +
  可执行 verifier 驱动 gate（由注入的 `VerifierOutcome` 决定），其余跳过，缺结果硬错误。tools 层
  不依赖 workflow 图（与 materialize 消费报告同构）。新增 7 项测试。tools/audit 标记已完成；
  剩 verifier 实际执行器待接入。
- 2026-05-29 — 完成 step 11 与 step 13：① 具体 LLM 后端 `HttpLlmClient`（OpenAI 兼容 + Ollama 两种
  模式，reqwest）；② 评估拆解协议 `decompose_assessment`（AssessmentLlmOutput → 多节点 + 边，
  self-check 把关）；③ LLM 调用编排 `run_llm_step`（新增 workflow/engine crate：建 snapshot →
  complete_structured → 成功值 / RawLlmNode 兜底 + attempted 边）。新增 18 项测试。step 11/13 标记已完成。
- 2026-05-29 — 审核（无偏离）后完成**工作流执行闭环**与 **Selection/Materialize 节点编排**
  （workflow/engine）：① `loop_steps`——`design_solution`/`implement_design`/`repair_code` 在
  `run_llm_step` 之上建 Pseudocode/Code 节点并连 `addresses→`/`implements→`/`repairs→` 边，失败走
  RawLlmNode 不伪造成功，I6 由 snapshot 边保证；② `select_materialize`——消费类型态 `CodeCandidate
  <Selected>`，建 SelectionNode（`selects→ Code`）→ 原子写盘 → MaterializeNode（`materializes→
  Selection`）；③ 为 design/implement 步骤补齐 `design_result`/`implement_result` 两个严格 schema
  （落实架构 §8.2「每模板一 schema」）。engine 新增依赖 sophia-materialize（tools 层，无循环）。
  新增 12 项测试（4 loop + 4 select_materialize 覆盖正/负路径与 I6 守护，原 4 step 保留）+ prompt
  schema 测试扩展。全工作区 189 passed / 0 failed，clippy 0 警告，fmt clean。剩余 workflow 项收敛为
  「工作流总调度器」（据 DecisionNode 驱动循环 + 预算/评分）。
- 2026-05-29 — 审核修正 + 新增 implement-loop。审核发现上一步 `loop_steps` 把 LLM 产物正文
  （`.pseudo` 文本、候选文件内容）解析后用 `#[allow(dead_code)]` 丢弃，导致下游 gate / 物化拿不到
  正文——已改为随 outcome 返回 `PseudocodeArtifact.text` / `CodeArtifact.files`（图节点仍不存正文，
  符合 4.4.3/4.4.4），`LoopStepOutcome<A>` 泛型化。在此基础上新增 **implement-loop**
  （`implement_loop`）：预算受限的 implement → code_check → repair 收敛循环；check 由调用方注入
  （`CodeChecker`，kind 必须 CodeCheck，与 materialize 消费 `GateReport` 同构，engine 不自行运行
  checker 保持分层），每次尝试 emit `DiagnosticNode` 连 `checks→ Code`，预算耗尽返回
  `BudgetExhausted`。新增 5 项 implement-loop 测试 + 更新 loop_steps 测试断言工件正文。全工作区
  194 passed / 0 failed，clippy 0 警告，fmt clean。
- 2026-05-29 — 新增**工作流总调度器 spine**（workflow/engine `scheduler`）：`run_goal_loop` 据
  DecisionNode 驱动 goal 推进——每轮 LLM 决策（emit DecisionNode + `considers→ 焦点`）→ 分派
  design_solution / implement-loop，通过即 `CandidateReady` 交回调用方做 select/materialize；预算
  强制 max_decisions / max_pseudocode_versions / max_total_llm_nodes；高层动作（decompose/backtrack/
  revise/澄清）以 `Yielded` 交回，不臆造语义；物化作为显式收尾不自动执行（design 10.10）。审核修正：
  `decision_node.json` 收紧为 `oneOf` 三 state_assessment kind 各自 required 全字段（此前仅 require
  kind，schema 通过但反序列化可能失败，违反 strict 模式忠实契约 1.3）。分层：code_check 由调用方注入
  （`CodeChecker`），prompt 请求 / schema 经 `StepRequests` 注入，调度器不自行运行 checker / 抽取上下文。
  新增 7 项调度器测试。全工作区 201 passed / 0 failed，clippy 0 警告，fmt clean。剩余 workflow 项：
  调度器高层动作落地 + 多候选评分。
- 2026-05-29 — CLI 接入 Development Graph 工作流子命令（cli `graph_cmd`，架构 §9.2）：`sophia graph`
  改造为可选子命令（无子命令仍出 ASG 摘要，向后兼容）。确定性子命令 `graph init`/`start`/`context`/
  `nodes` 在 `sophia-runs/graph/dev_graph.sqlite` 上事件溯源 append（跨进程 replay 持久化）。LLM 子命令
  `graph design`（→ PseudocodeNode + `.pseudo` 落盘 artifacts）/`graph implement-loop`
  （implement→code_check→repair 预算闭环 → 候选落盘，未物化），后端经 `--model/--mode/--base-url/
  --api-key` 构造 `HttpLlmClient`，CLI 用一次性 tokio 运行时跨异步边界。新增 `code_check_files` 把候选
  桥接 `tools/check`（语法→HIR→语义→strip-assist）产出注入 implement-loop（CLI 是注入点，engine 不自行
  运行 checker）。失败不伪造成功（后端不可达保留 RawLlmNode + 失败退出码）。新增 6 项 CLI 集成 + 4 项
  `code_check_files` 单测。全工作区 211 passed / 0 failed，clippy 0 警告，fmt clean。剩余 CLI 项：
  `graph select`/`materialize`、`context`/`smoke`/`repair-context`。
- 2026-05-29 — CLI 接入 `graph select` / `materialize`，工作流闭环最后一公里贯通
  （`start → design → implement-loop → select → materialize`）。engine 把 `run_selection_materialize`
  拆为可组合的 `run_selection` + `run_materialization`（CLI 两命令分属两进程）。设计要点：类型态
  `CodeCandidate<Selected>` 证明不可跨进程持久化，故 select / materialize 各自从 artifacts 重新加载
  候选并**重跑 materialize gate**（design 10.10，对不可逆写盘更稳妥）——code_check（桥接 tools/check）/
  constraint_audit（对 bound invariant 跑 tools/audit；声明可执行 verifier 却无运行器 → 硬错误阻断，
  忠实反映「待接入」）/ artifact_diff（strip-assist）/ runtime validation（起步阶段无 hidden case
  可跑则通过，非伪造）；各 gate emit `DiagnosticNode` 连 `checks→ Code`，任一未过即阻断不伪造成功。
  materialize 原子写入 `domains/`（先 staging 后 rename，由 materialize crate 保证）。新增 3 项
  select/materialize 单测（干净候选写 domains / code_check 失败阻断 / materialize 拒非 Selection）+
  2 项 engine 拆分原语测试。全工作区 216 passed / 0 failed，clippy 0 警告，fmt clean。
- 2026-05-29 — 可行性核对发现 **stdlib 内置节点契约阻塞于语言设计**（grammar 无 `node`/`effect` 顶层
  语法、effect 集合封闭），不在 v0 起步子集内——已在 checklist 标注阻塞，不臆造语法。改做
  `sophia context --action/--task`（语言设计 §8 Task Closure / Semantic Paging）。新增 core/hir
  `closure` 模块：`action_context`（从 action root 沿 ASG 邻域遍历：capability / input-output 类型 /
  effect→storage / errors→error / body 调用递归 / domain 文件）与 `task_context`（include 入口 +
  formal 依赖 + exclude 命中报错），产出节点 + 解释边（binds_capability/calls/raises/reads/writes/
  uses_type/in_domain/includes）+ 文件列表，纯 HIR 零 IO、输出确定性。CLI 新增 `sophia context` 命令
  （`--sources` 附源码）。新增 7 项 HIR closure 测试 + 2 项 CLI 测试。全工作区 225 passed / 0 failed，
  clippy 0 警告，fmt clean。
- 2026-05-29 — 技术债深度清理（行为不变，225 passed / 0 failed 全程绿）。经 context-gatherer
  子代理系统性勘察后按影响排序修复：
  ① **engine 测试去重**：4 个测试文件各自复制的 `MockClient`、手写 `design_schema`/`impl_schema`
  （与 prompt crate 权威 schema 重复且会漂移）、节点 seed 助手、临时目录助手，统一抽到
  `workflow/engine/tests/common/mod.rs`（`MockClient` / `schema()` 复用 `sophia_prompt::schema_for`
  做单一事实来源 / `seed_objective`/`seed_pseudocode`/`seed_code`/`seed_snapshot` / `temp_dir` /
  `req`）。消除约 200 行重复，测试 schema 不再可能与产物漂移。
  ② **graph_cmd 去重**：`NodeId::parse + with_context` 与目标域 role 校验抽为 `parse_node` /
  `expect_target_domain`；select / materialize 共享的 load+gate 流程抽为 `prepare_selected_candidate`；
  `report`/`report_bool` 合并为单个 `report(impl Into<String>)`；85 行的 `code_check_files` 拆为
  `syntax_diagnostics` + `semantic_diagnostics` 两阶段组合。
  ③ **scheduler 解构**：124 行的 `run_goal_loop` 拆为 `budget_exceeded` 预算门 + `dispatch_design` /
  `dispatch_implement` 动作分派（`Dispatch` 枚举表达「继续 / 结束」），主循环回到可读长度。
  ④ **错误处理**：`SchedulerError` 此前把 `ImplementLoopError` 的 Prompt/WrongDiagnosticKind 变体
  stringize 成 `Graph(InvalidPayload(..))` 丢失类型信息——改为 `#[from] ImplementLoopError` 保留
  typed 变体。
  ⑤ **死代码**：移除从未使用的 `RawLlmKind` 类型别名及其 re-export。
  保留（非债务、属设计文档支撑的前瞻资产）：`revise_design` 模板 / `pseudo_check` schema（架构 §8.1
  prompt 资产集，调度器已为 ReviseDesign 预留让位路径）、`run_selection_materialize`（连贯公共组合，
  有测试覆盖）。clippy 0 警告、fmt clean、总行数 19527 → 19443。
- 2026-05-29 — 完成 stdlib 前置语言设计：`node` / `effect` 两类顶层构造（`language_design.md`
  第十三节，从草案升级为完整设计）。`effect <Family> { operation <Op> { param... } }` 把封闭的 4
  类硬编码 effect 推广为可声明的 effect 族；effect 规范化表示从枚举变体改为 `(family, op, args)`
  三元组（effect 层 / 契约层算法逐字不变，只换表示与来源）；引用语法统一为 `Family.Op(args)`。
  `node <Name> { input|inputs / output|outputs / effects / capability }` 声明内置节点接口契约
  （无 body，实现由运行时后端提供，不进 v0 解释器）。给出各层处理（syntax/HIR/semantic/runtime/
  stdlib）与单一路线迁移计划（硬编码 effect → stdlib 预声明 Console/DB，一次迁移不留双栈，现有
  `DB.Read("Todos")` 等写法对用户无感）。同步更新 `engineering_architecture.md` §4.1 引用、
  `engineering_notes.md` 决策条目（Accepted）。stdlib 由"阻塞于语言设计"转为"设计已就绪、待实现"，
  checklist 拆为 node/effect 构造 + 契约文件两个可执行项。本步为纯设计，无代码改动。
- 2026-05-29 — 实现 `node` / `effect` 顶层构造（语言设计第十三节），解除 stdlib 阻塞，全链路贯通。
  ① syntax：grammar 新增 `effect_def`/`node_def`/`effect_operation`/`inputs_block`/`outputs_block`，
  effect 引用由硬编码 4 分支改为通用 `effect_ref`（`Family.Op(args)`/`Pure`），ABI 15 重生成 parser.c；
  AST 新增 `Item::{Effect,Node}` + `EffectDef`/`NodeDef`/`EffectArg`，`Effect` 改三元组；lowering 同步。
  ② HIR：`NodeKind::{Effect,Node}` 入 index；新增 `AsgIndex::effect_ops` 符号表（内置族
  Console/DB/Llm/Tool/Stream 由 `builtins::BUILTIN_EFFECT_OPS` 预置，因 core 零 IO 不自举解析 stdlib，
  内置族以 Rust 数据承载；`effect` 声明并入同表）；名称解析校验 effect 引用已声明 + arity（新增
  `UnresolvedEffect` 诊断）。③ semantic：`Effect=(family,op,args)`、`EffectArg::{Lit,Binding}`；
  capability 匹配改 `covered_by`（字面量须相等、绑定名通配，使 `Llm.Complete(model)` 匹配
  `allow{Llm.Complete("...")}`，仍保 `DB.Read("A")≠DB.Read("B")`）；新增 node 契约检查（type +
  capability，无 body）。④ stdlib：5 effect + 3 capability + 5 node `.sophia`，crate `include_str!`
  内嵌 + `load_contracts`/`check_contracts` 自检。单一路线：移除旧 4 变体 effect 表示，无双栈；现有
  `DB.Read("Todos")` 写法对用户无感。CST 快照更新为 `effect_ref`。新增 15 项测试（4 syntax + 4 HIR +
  4 semantic + 3 stdlib）。全工作区 240 passed / 0 failed，clippy 0 警告，fmt clean。文档同步：
  `language_design.md` §13 状态转「已实现」并校正各层处理 / 迁移说明，`engineering_architecture.md`
  §4.1 转「已落地」。
- 2026-05-29 — 修复 v0 目标漂移：Execution Graph IR 此前是**死产物**——`exec-ir` 仅在 workspace
  清单被引用、无任何 crate 消费；`from_model` 只建节点不建调用边；解释器直接消费 Semantic 模型 + AST
  执行，**绕过** Execution Graph IR，违背设计 9.2 流水线 `Semantic IR → Execution Graph IR →
  Interpreter`。checklist 表面标「已完成」，实则核心 v0 桥梁缺失。修复：① `ExecGraph::from_model`
  改签名收全程序 AST，扫描每个 callable body 把对 action/transition 的调用（`Call` callee + transition
  构造式 `Name{}`）物化为 `Control` 调用边；新增 `has_node`/`has_call_edge`/`node_id_by_name`。
  ② `runtime` 新增 `sophia-exec-ir` 依赖；`Interpreter` 持有 `ExecGraph`，`run` 在执行前经图解析
  执行入口（节点必须存在）、非顶层调用校验调用边在图中（`cur_callable` 跟踪 from 端），使 exec-ir 成为
  真实执行桥梁。现有跨文件 action 调用 / transition 构造式调用测试现真正经调用边路由。新增 2 项
  exec-ir 调用边测试（建边 / 非 callable 构造不建边）。单一路线：无双栈，解释器唯一执行路径经图。
  全工作区 242 passed / 0 failed，clippy 0 警告，fmt clean。
- 2026-05-29 — v0 一致性逐项排查：消除占位 / 虚假路径 / 冗余路径 / 文档漂移（行为不变，247 passed /
  0 failed，clippy 0 警告，fmt clean）。经 context-gatherer 子代理系统勘察后逐项核实修复：
  ① **死基础设施移除**：`EffectHost::db_read/db_write` + `InMemoryHost.storage`（v0 唯一可执行 effect
  是 `Console.Write`；`DB.Read/Write` 的运行时执行依赖 body 级 storage 操作，属 §16.6 扩展子集，
  不在 v0 解释器内——故不为未实现功能预留死接口，host 只留 `console_write` 真实路径）；
  `LlmError::NotImplemented`（从未构造，仅被 match）及其 `step.rs` 映射 arm。
  ② **死公共 API 移除**（clippy 不报 lib crate 未用 pub 项，故此前隐藏）：`Ast::expr_count`、
  `Span::len`/`is_empty`、`SyntaxTree::source`/`has_errors`、`EffectSet::is_subset_of`/`len`/`is_empty`、
  `Value::as_text`——全工作区零引用，逐一 grep 确认后删除。
  ③ **冗余路径收敛**：`ExecGraph::from_model` 此前直接 `edges.push` 旁路了 `add_edge`（致后者成死
  API），改为统一经 `add_edge`（含两端节点存在断言），单一建边路径。
  ④ **文档-代码漂移修正**（§16 起步子集）：§16 此前把 `transition` / "transition call" 列为「不进入
  起步子集」，但 transition 实为可检查、可解释执行的 callable，全链路（syntax→HIR→semantic→exec-ir→
  runtime）已实现且有测试，规范 TodoDomain 示例依赖 `CompleteTodoTransition`——文档是陈旧侧，已校正
  §16 引言与 §16.6：明确 transition 调用（构造式 / 直接）在子集内，仅其**合约证明**不在；并删去
  §16.6 关于 `requires_runtime_check` 诊断的过期承诺（起步子集只对 `ensures`/`requires` 做名称解析 +
  谓词 Bool 类型检查，不产生证明义务，这是有意子集边界，非静默通过——澄清而非新增伪实现）。
  ⑤ **过期注释更正**：`type_layer::method_ty` 注释删去「storage.get/save 等」误导（仅处理
  list.append）；HIR `resolve_value_ident` 的 `storage` 根注释改为如实说明「允许名称解析以支持完整
  语言源码的解析 / 索引 / 语义闭包，而非声称可执行」。检查确认：storage 操作经 check 后在 `run` 阶段
  以清晰硬错误（不支持的方法调用）失败，非伪造成功；checklist「尚未完成」区已如实列 storage ops /
  Trace 投影 / Tokio 为扩展项。同步更新 effect host checklist 条目。
- 2026-05-29 — v0 首次**真实 LLM 端到端测试**跑通（design → implement → check → 解释执行 闭环）。
  新增 `cli/examples/todo_llm_e2e.rs`：在内存 Development Graph 上种人类 Objective + 验收条件 +
  语法基线约束 → `design_solution`（真实 LLM）产 `.pseudo` → `run_implement_loop`（真实 LLM +
  真实 `tools/check` 作为注入 code_check + 预算内 repair）产候选 `.sophia` → 通过 check 后用
  v0 解释器（`sophia_runtime::run_action`）真正执行并对照期望打印。两个任务：①算术单 action
  （IncrementCounter，1 次收敛，返回 42）；②多文件 Todo 领域（`SOPHIA_LLM_TASK=todo`：state
  TodoStatus + action CompleteTodo，1 次收敛，返回 TodoStatus.Done）。
  **LLM 后端**：NVIDIA OpenAI 兼容端点（`https://integrate.api.nvidia.com/v1`），默认模型
  `deepseek-ai/deepseek-v4-flash`（实测 ~7s、稳定 JSON；GLM 5.1 / Kimi 2.6 在该端点响应超时，故不作
  默认）。**安全**：API key 仅经 `SOPHIA_LLM_API_KEY` 环境变量读取，不落盘、不进图、不打印；
  未设置时 example 干净跳过（CI 安全，不进 `cargo test` 确定性门禁）。模型可经
  `SOPHIA_LLM_MODEL` / `SOPHIA_LLM_BASE_URL` / `SOPHIA_LLM_MAX_REPAIRS` 覆盖。
  经验记录：模型未见过 Sophia 语法，必须在 prompt 中显式注入起步子集语法基线（经现有模板的
  `context_files`/`constraints` 槽注入，**不改**共享模板以保 snapshot 稳定）；design 阶段须明确
  「pseudocode 是单个文档而非文件数组」、implement/repair 阶段须明确输出 JSON 形状（含 repair 的
  `changes` 字段），否则 deepseek-flash 会包裹错误的外层键或返回文件数组导致 schema 验证失败。
  运行：`cargo run -p sophia-cli --example todo_llm_e2e`（默认算术任务）/ 加 `SOPHIA_LLM_TASK=todo`。
- 2026-05-29 — e2e 安全审核 + 起步定位收紧（防答案泄漏 / 要求一次过）。审核发现并修复脚手架答案泄漏：
  ① **语法基线泄漏答案**——`SOPHIA_SYNTAX_PRIMER` 此前直接内嵌算术任务的完整答案
  （`action IncrementCounter { ... return current + 1 }`）与 Todo 任务的完整答案
  （`state TodoStatus { value Pending ... value Done ... }`、`entity Todo`），模型只需照抄即可"通过"，
  并未真正泛化。已重写为**仅含可泛化标准语法规则 + 与任何任务无关的中立示例**（占位符 `<名字>` 形式 +
  `Light`/`Toggle` 示例），严禁出现任务的领域名 / 节点名 / 状态值 / 具体逻辑。
  ② **implement system prompt 泄漏标识符**——此前硬编码 `TodoDomain/states/TodoStatus.sophia` 等路径示例，
  泄漏答案的领域名 / 文件名；改为通用 `<域名>/<复数类别>/<节点名>.sophia` 模式。
  ③ **起步定位收紧为"一次过、不修复"**：默认 `SOPHIA_LLM_MAX_REPAIRS=0`，且 `attempts > 1`（发生修复）
  即判未达标。修复闭环（repair）留待另行测试。任务描述保留其领域词汇（需求规格本身，且 harness 需据此
  构造输入 / 校验输出），但**不示范任何 Sophia 源码**。
  复核：两任务均**一次通过、零修复**，且模型输出体现真实泛化（算术任务自行用 `let result = current + 1;
  return result` 变体、Todo 任务自行命名参数 `status`，均非照抄基线）。全工作区 247 passed / 0 failed，
  clippy（含 example，`-D warnings`）0 警告，fmt clean。grep 确认 key 值与答案 token 不在脚手架（primer /
  system prompt）出现，仅存于任务需求定义与防泄漏注释中。
- 2026-05-29 — 脚手架分层精炼 + 修复闭环真实测试（落实"决断性信息进脚手架、不把格式压给伪代码"原则）。
  ① **design / implement 分层修正**：此前把语法基线注入到 **design**（设计）阶段的 constraints 槽——
  违背"伪代码 semantics > format"原则（把格式压给语义层会逼模型产出格式化伪代码，反在 implement 阶段
  制造不必要修复点）。已改为：design 阶段**不注入任何语法基线**，只看目标 + 验收条件（纯语义）；
  语法基线（决断性事实）只在 **implement / repair** 阶段经 **system prompt** 注入（脚手架降负荷），
  模板的 `context_files` 保持空（greenfield 无语义闭包，不臆造）。复核：去掉 design 阶段语法注入后，
  两任务仍**一次过、零修复**（伪代码更自由、更语义化）。
  ② **新增修复闭环真实测试** `cli/examples/repair_llm_e2e.rs`：从一份**故意有缺陷的候选**
  （`int` 应为 `Int`、`output` 缺花括号、body 用未声明变量 `n`）出发，跑 `check → repair → check` 闭环，
  用**真实 `tools/check` 诊断**驱动 deepseek-flash 修复，实测 **1 轮修复收敛**，修好后由 v0 解释器执行得 42。
  防泄漏复核：起步坏候选是"题目里待修的东西"（非答案）；修复时只喂坏候选正文 + 真实诊断（只报错不报
  "应改成什么"）+ 可泛化语法基线（中立 `Toggle` 示例）；grep 确认正确写法（`output { result: Int }` /
  `return current + 1`）不在任何脚手架（primer / system prompt）出现，仅存于坏候选、任务需求与 harness
  执行内部。`SOPHIA_LLM_MAX_REPAIRS` 控制修复预算（默认 3）。
  全工作区 247 passed / 0 failed，clippy（含两个 example，`-D warnings`）0 警告，fmt clean。
  说明：design 模板的固定 heading（Purpose/Inputs/...）不构成修复点——伪代码只过 `pseudocode_check`，
  不过 `code_check`，故未改共享模板（保 snapshot 稳定）。
- 2026-05-29 — 决策并落地**语言语法基线 prompt 资产**（沉淀 Sophia-Core 语法基线，所有工作流共享，
  不再散落在 example）。**选型：prompt 资产，而非 stdlib**——基线是面向 LLM 的自然语言指令 + 中立示例，
  不是可被编译器解析的形式化 `.sophia` 源码（放 stdlib 会污染"stdlib = 可解析形式化契约"不变量）；
  其消费方是 workflow/LLM 层（stdlib 由零 IO 的 `core` 消费）；分层上 `core`/stdlib 不得依赖 workflow，
  基线天然属 workflow 侧；漂移守护复用 §8.2 已有的 insta snapshot（stdlib 的守护是"能否解析"，对自然语言
  基线不适用）。设计写入 `engineering_architecture.md` §8.3 + §8.1 目录。
  **顺带修复真实工作流缺口**：此前 CLI `graph design`/`implement-loop` 的 system prompt 不含任何语法基线
  （只有 example 各自注入），用真实 LLM 跑必失败；改为共享资产后 CLI 实现/修复路径也获得语法基线。
  落地：`prompt/assets/sophia_syntax_baseline.md` + `prompt` crate 暴露 `preamble(name)`（include_str! 内嵌）；
  CLI 与两个 example 改为引用该单一资产；新增 snapshot 守护资产内容变更。两条硬约束：① 只含可泛化标准语法
  规则 + 中立示例（严禁任务答案/领域名/逻辑）；② 只进实现/修复阶段，不进 design（不把格式压给语义伪代码）。
- 2026-05-29 — 语言语法基线 prompt 资产**落地完成**（前条决策的实现）。新增
  `workflow/prompt/assets/sophia_syntax_baseline.md`（Sophia-Core 起步子集语法基线，仅可泛化标准规则 +
  中立 Light/Toggle 示例）；`prompt` crate 新增 `ASSETS` 表 + `preamble(name)`
  （include_str! 内嵌）。消费方统一引用单一资产：① CLI `render_implement_request` 的 system 注入基线
  （修复了真实工作流缺口——此前 CLI implement 路径无语法基线，真实 LLM 必失败）；② 两个 example
  （`todo_llm_e2e` / `repair_llm_e2e`）删除各自内联的 `SOPHIA_SYNTAX_PRIMER` const，改引用 `preamble`；
  顺手移除 `todo_llm_e2e` 把语法基线塞成图 ConstraintNode 的旧做法（语法基线是 prompt 层关注点，不该
  进 Development Graph 约束）。新增 3 项 prompt 测试：snapshot 守护基线内容（`render__sophia_syntax_baseline`）、
  防答案泄漏断言（基线不含 IncrementCounter/TodoStatus/Pending/Done/current+1 等任务 token）、`preamble`
  未知名返回 None。复核：arithmetic / todo / repair 三个真实 LLM e2e 均仍按预期（前两者一次过、repair
  一轮收敛）。全工作区 250 passed / 0 failed，clippy（`-D warnings`，含 examples）0 警告，fmt clean。
- 2026-05-29 — 系统化 e2e 测试设计（新增活文档 `docs/e2e_test_design.md`）。确立：① 目的与边界
  （e2e 测真实 LLM 下的完整 v0 闭环 design→implement→check→repair→解释执行，不进 cargo test 门禁）；
  ② **防答案泄漏为第一原则**（结构化防线：单一共享语法基线资产 + snapshot + 防泄漏断言；答案只存于
  harness 内部期望/坏候选，不喂 LLM；design 阶段不注入语法）；③ 能力维度分组 G1 基本语法/纯逻辑、
  G2 effect+capability、G3 启发式节点(decompose/assessment/decision)、G4 复杂程序、R 修复闭环(横切)；
  ④ 用例真实性要求（贴近 todo/库存/订单等真实场景，避免伪代码过简，但受 v0 起步子集约束）；
  ⑤ 工程结构（单一 harness + 用例注册表 `cli/examples/e2e/`，新增用例 = 加一个 `Case`，不碰 harness）；
  ⑥ 运行/成功判据/报告/CI 关系。规划：先实现 G1 + 既有 R-01，把现有两个零散 example 收编进统一 harness。
- 2026-05-29 — 实现 e2e 测试 **G1 组（基本语法/纯逻辑，4 用例）+ R-01（修复闭环）**，统一收编进
  `cli/examples/e2e/`（单 harness + 用例注册表 `cases/`，`[[example]] name="e2e"`）。删除原先两个零散
  example（`todo_llm_e2e` / `repair_llm_e2e`），消除脚手架重复。harness 支持 `--group g1` / `--case G1-02`
  过滤、分组汇总报告、瞬时网络抖动有界重试（`RetryClient` 只重试 `BackendUnavailable`，不改库语义——
  库层 complete_structured 仍故意不重试以免放大不可用）。G1 用例（贴近真实业务，非伪代码）：① 整数加一
  ② 待办状态置完成（state 多 value）③ 购物车单项金额合计（entity 多字段 + 乘法）④ 免邮资格判定（Bool +
  比较）。实测 deepseek-flash：**G1 4/4 一次过、零修复；R-01 一轮修复收敛**。
  期间据真实诊断反哺**共享语法基线**一条决断性规则：body 语句末尾不写分号 `;`（G1-03 首跑因模型加 C 风格
  分号失败，补入基线后一次过；同步更新 snapshot）——决断性语言事实进脚手架的范例，可泛化、不含任务答案。
  全工作区 250 passed / 0 failed，clippy（`-D warnings`，含 example）0 警告，fmt clean。防泄漏未回归。
- 2026-05-29 — 实现 e2e **G2 组（effect + capability，2 用例）** + 串行批量脚本。harness 的 `Case` 增
  `expected_console`，成功判据同时校验返回值与 console 输出（验证 effect 经解释器 effect host 真正执行）。
  G2-01 审计日志写入（`Sanitized<Text>` 输入 + `Console.Write` + capability 绑定 + `.length`，返回字符数 5、
  console=["hello"]）；G2-02 双行通知广播（多次 print 顺序执行，返回 2、console=["hello","bye"]）。实测
  deepseek-flash **两用例一次过**。新增 `--list` 标志（枚举用例 ID，不需 key）与 `scripts/run_e2e.sh`
  串行批量执行器（逐用例各起一进程、日志落盘 `sophia-runs/e2e-logs/`、汇总；支持 `g1`/`g2`/`--cases`）。
  据真实诊断反哺共享语法基线两条决断性规则：① `Console.Write` 只接受字面量/`Sanitized<T>`/`Redacted<T>`
  （intent 边界）+ capability allow 形状；② 伪字段 `.length`/`.exists` 与 `to_text(Int)` 方向。均可泛化、
  不含任务答案；防泄漏断言测试同步登记 G2 token。单次请求超时 180s→60s 配合重试容忍公网抖动。
  全工作区 250 passed / 0 failed，clippy（`-D warnings`，含 example）0 警告，fmt clean。
- 2026-05-29 — 设计（纯文档，无代码）：**调度器 prompt 调用时刻渲染**（`StepPrompts` 提供者取代静态
  `StepRequests`）。G3 e2e（启发式节点处理）实现受阻于一个真实设计缺口：调度器 `run_goal_loop` 用预渲染
  的静态 `CompletionRequest` 每轮复用，违反 `language_design.md` §10.7/§10.8（prompt 必须由**调用时刻**的
  active context 渲染、与 `consumed→ ContextSnapshot` 同源）。后果：① 状态不演进、LLM 无法自主多步推进，
  多步启发式编排名存实亡；② snapshot 与 LLM 实际所见不一致，破坏 10.7 复现保证与 anti-cheat 前提；
  ③ 实现步骤拿不到刚 design 出的伪代码正文。按"以最理想设计、不打补丁"的要求，确定正解为**调用时刻的
  prompt 提供者** trait：engine 在每步即将调用 LLM 时回调协调层实现的 `StepPrompts`，传入该步源自图状态
  的输入（implement 步传本轮伪代码正文），用与 ContextSnapshot 同一份 active-context 当场渲染请求；静态
  `StepRequests` 整体被取代（单一路线，无双栈）；engine 仍不含模板/抽取逻辑（§3.3 分层不变）。设计写入
  `engineering_architecture.md` §8.4、`language_design.md` §10.9、`engineering_notes.md` 决策条目。下一步：
  按此设计落地 engine 改造，再实现 G3。
- 2026-05-29 — 落地**调度器 prompt 调用时刻渲染**（`StepPrompts` 取代静态 `StepRequests`）+ 实现 e2e
  **G3 组（启发式节点处理）**。engine 改造（单一路线，无双栈）：① `run_llm_step` 收 prompt 渲染闭包
  `FnOnce(&ActiveContext)->CompletionRequest`，**用与 ContextSnapshot 同一份 active context 渲染请求**
  （根除 §10.7 snapshot 失真）；② `design_solution`/`implement_design`/`repair_code` 收渲染器、schema 按步
  固定取内置（`step_schema`）；③ 新 `prompts` 模块定义 `StepPrompts` trait + `GoalProgress`；
  `run_implement_loop`/`run_goal_loop` 收 `&impl StepPrompts`，`run_implement_loop` 额外收伪代码正文
  （implement 提供者需要——图节点不存正文）；④ 移除 `StepRequests` 与 `build_repair_request`、
  `ImplementLoopError::Prompt`。CLI `graph_cmd` 适配（design 用渲染闭包、implement-loop 用 `CliImplementPrompts`）。
  顺带修正一个潜伏 bug：旧测试给 repair 传了 `implement_result` schema（缺 `changes`），新代码正确用
  `repair_result`，测试 fixture 同步补 `changes`。e2e harness `Case` 增 `kind` 分派三路径，新增
  `HarnessPrompts` 实现 trait。新增 G3-01（库存扣减，`CaseKind::Scheduler`）：LLM 经 `run_goal_loop`
  **2 轮自主决策**（decision→design→decision→implement）推进到候选、v0 解释器执行得 42。全工作区
  250 passed / 0 failed，clippy（`-D warnings`，含 example）0 警告，fmt clean。
- 2026-05-29 — 实现 e2e **G4 组（复杂程序，2 用例）**，e2e 用例分组 G1–G4 + R 全部就绪。harness `Case`
  的 `expected: Value` 升级为 `expect: Expect`（`Returns(Value)` / `Raises(&str)`），成功判据可校验
  正常返回值或 raise 出的领域错误 variant（验证 v0 解释器 `Outcome::Raised` 路径）；既有 G1/G2/G3/R
  用例同步迁移。G4-01 订单总价：`OrderTotal` 跨调用 `LineSubtotal`（经 Execution Graph 调用边路由），
  实测一次过、执行得 42——验证真实 LLM 生成的跨 action 调用代码经 exec-graph 正确路由。G4-02 提现校验：
  LLM 生成 `error WalletError { variant InsufficientFunds { shortfall: Int } }` + `Withdraw` action
  （`errors` 声明 + if 分支 raise），实测 `Withdraw(30,50)` 正确 raise `InsufficientFunds`。据真实需求
  给共享语法基线补两条决断性规则（跨 action 调用 `<ActionName>(args)`、error algebra 形状），防泄漏断言
  登记 G3/G4 token，snapshot 同步。e2e 重试预算 4→6 以容忍当下端点抖动（纯外部网络，非逻辑问题）。
  全工作区 250 passed / 0 failed，clippy（`-D warnings`，含 example）0 警告，fmt clean。
- 2026-05-29 — v0 核对后推进两项（文档先行）：① 修正文档-代码不一致——`language_implementation.md`
  §9.4 的 `ExecutionSpan` 引用了代码中不存在的 `ExecEdgeId`（`core/exec-ir` 只有 `ExecNodeId`，边无
  稳定 ID），已加实现状态注记：trace 未实现，落地前需先为执行图边引入稳定 ID，trace 属可观测性不在
  执行正确性路径上。② **调度器高层动作部分落地**：`revise_design` 接入循环（implement-loop 预算内未过
  不再结束 goal 循环，回到 decision，LLM 可据概念性诊断选 revise 重写伪代码——新增 `engine::revise_design`
  建新 Pseudocode + `revises→` 旧；`GoalProgress.last_implement_failed` + `Dispatch::ImplementExhausted`
  使 revise 可达，design 10.8 原则 3）；`needs_clarification` 真正 emit `Clarification(Question)` +
  `asks_about→ 焦点` 再让位。`decompose`/`backtrack` 仍 `Yielded`（非线性树操作，§10.9 不塞进 spine）。
  新增 2 项调度器测试（revise 可达 / clarification emit 节点），G3-01 真实 LLM 复测仍通过。
  ③ **核心 IR snapshot 测试**：HIR（ASG index JSON）/ Semantic IR（formal_fingerprint）/ Execution
  Graph IR（节点 + 调用边结构）各加 1 项 `insta` 快照（exec-ir 新增 insta dev-dep）。
  全工作区 255 passed / 0 failed，clippy（`-D warnings`）0 警告，fmt clean。
- 2026-05-29 — 实现**多候选评分排序**（design 10.9 score 块）。`tools/materialize` 新增 `score` 模块：
  `Score`（七维度子分 + overall）/ `ScoreInputs`（compile/tests/constraints 真实 gate 信号 + 候选源码 +
  可选 pseudocode_clarity）/ `ScoreWeights`（默认：正确性维度合计 0.75 主导、结构性 0.25 次要）/
  `score_candidate` / `rank_candidates`（降序 + 下标升序确定性平局打破）。诚实性：compile/tests/constraints
  取自 gate 报告；simplicity（源码长度）/ locality（文件数）/ capability_minimality（effect+capability
  声明数）按明确公式由源码计算；pseudocode_clarity 无信号取中性 0.5（不伪造）。硬约束
  `compile=0 → overall≤0.49` 编码。engine 新增 `run_ranked_selection`（RankedCandidate 列表 → 排名 →
  选 winner → 建 SelectionNode，rationale 记评分摘要可审计）+ `Score`/`ScoreWeights` re-export。评分是
  **内存启发式不入图**（spec 无 Score role；只为 winner 建一个 SelectionNode）。新增 7 项 score 单测
  （compile=0 封顶 / 可编译胜出 / 更简单胜出 / 最小权限 / 确定性平局 / 空输入 / clarity 中性默认）+
  2 项 engine ranked 测试。全工作区 264 passed / 0 failed，clippy（`-D warnings`）0 警告，fmt clean。
- 2026-05-30 — 推进 B 组（CLI 便利命令，§9.1 确定性、不调用 LLM）：① **`sophia smoke`**——一键串联
  init（幂等）→ check → build → run（`--action <Name>` 可选，省略则只做 check/build）的烟雾测试，
  任一步失败即以失败退出码中止（忠实反映，不伪造通过）；② **`sophia repair-context --error <code>`**
  ——为 LLM 修复循环生成结构化上下文（impl 14.3），从 `check` 同口径诊断里按诊断码子串筛选，对每条
  给出归属文件 + 1 基位置 + 诊断码 + 信息 + 该 action/transition 的 action-rooted 语义闭包（相关节点 /
  文件），**不臆造修复建议**（具体改法是 LLM 职责），无匹配则成功退出并提示。重构：抽出
  `commands::collect_diagnostics`（带 `CollectedDiagnostic { rel_path, span, code, message, callable }`）
  供 `check` / `repair-context` 共用，保持诊断口径单一。新增 5 项 CLI 集成测试（smoke 通过 /
  smoke 无 action / smoke check 失败中止 / repair-context 结构化输出含闭包 / repair-context 无匹配）。
  全工作区 269 passed / 0 failed，clippy（`-D warnings`）0 警告，fmt clean。B 组其余项有真实前置阻塞、
  不硬上：trace 投影需先为 Execution Graph 边引入稳定 ID（impl §9.4 已注记）；constraint audit verifier
  执行器需与运行时 / 测试 harness 接入（审计框架已就绪，缺结果时已诚实硬错误阻断，非伪造）。
- 2026-05-30 — 推进 A 组：**目标树遍历层**（design 10.8 动作 6 decompose / 动作 7 backtrack，
  10.9 不塞进 spine）。在 spine（`run_goal_loop`）之上新增独立遍历层 `engine::run_goal_tree`
  （`traversal` 模块）：spine 仍让位 `Decompose` / `Backtrack`（`Outcome::Yielded`），遍历层承接——
  decompose 执行 `engine::decompose_goal`（LLM 给拆解结构 `decompose_result` schema → 确定性
  `graph-db::build_decomposition` 落图：`Decomposition` + `parent decomposes→` + 子 `Objective`
  各 `member_of→`）后**递归**深度优先驱动每个子目标；backtrack 记 `GoalResolution::Backtracked`
  放弃分支。新增 prompt `decompose` 模板 + `decompose_result` schema；`StepPrompts` trait 增
  `decompose` 方法（StaticPrompts / HarnessPrompts / CliImplementPrompts 全适配）；engine 导出
  `run_goal_tree` / `GoalResolution` / `TreeBudget` / `decompose_goal` / `DecompositionArtifact`，
  graph-db 导出 `build_decomposition` / `ChildGoal` / `DecompositionNodes`。诚实性硬约束：
  Decomposition / 子目标作为结构性派生节点不单独 `consumed→ snapshot`（I6 锚点是触发的
  DecisionNode，与 assessment 协议同构）；backtrack 不伪造 `WithdrawalEvent`、binding 不伪造（撤销 /
  接受是人类权威 N4，LLM 派生子目标默认未绑定，人类接受 Decomposition 后才沿 member_of 继承）。
  `TreeBudget`（max_depth / max_goals）防递归爆炸。新增 5 项 graph-db decomposition + 5 项 engine
  traversal + 1 项 prompt decompose render 快照测试。文档同步：design §10.9（目标树遍历层）、
  architecture §8.5（含分层动机 / I6 锚点 / 不伪造人类授权）、engineering_notes 决策条目。
  全工作区 280 passed / 0 failed，clippy（`-D warnings`）0 警告，fmt clean。
- 2026-05-30 — decompose 重构审核修正（单一路径 / I6 一致性）：审核发现 `decompose_goal` 经
  `run_llm_step` 建了 `ContextSnapshot` 却在成功分支丢弃（`{ value, .. }`），导致 ① snapshot 悬空、
  ② decompose 的 LLM 生成内容（拆解结构）无复现锚点。原文档"由触发的 DecisionNode 承载 I6"不成立
  ——decision（"该不该拆"）与 decompose（"怎么拆"）是 §10.8 分离的两次独立 LLM 调用、各有 snapshot。
  修正（最佳实现、非补丁）：把 `Decomposition` 提升为**一等 LLM 执行产物节点**（与 Pseudocode /
  Code / Assessment 同构），**自身** `consumed→ ContextSnapshot`。`build_decomposition` 增 `snapshot`
  参数并校验其 role，`decompose_goal` 透传 `run_llm_step` 所建 snapshot；`Decomposition` 纳入
  `validate_i6` 必检集合与 `Consumed` 边 `(from,to)` 允许集合。子 `Objective` 仍为结构性派生节点
  （经 `member_of` 间接锚定）。spec 同步：`workflow_graph_spec.md` §I6 / 4.1.4 / 5.4 / 6.1 的
  `consumed` 行均纳入 `Decomposition`；design §10.9、architecture §8.5、engineering_notes 决策条目
  的 I6 表述一并更正。新增 1 项 graph-db 测试（拒非 snapshot 锚点）+ 强化 traversal 测试（断言
  `Decomposition consumed→ ContextSnapshot`）。全工作区 281 passed / 0 failed，clippy（`-D warnings`）
  0 警告，fmt clean。
- 2026-05-30 — 解锁 B 组两个被阻塞项（各自满足其文档化前置条件后落地）：
  ① **Trace 投影到 Execution Graph**（impl §9.4）。前置「执行图边需稳定 ID」已满足：`core/exec-ir`
  新增 `ExecEdgeId(u32)`（按构建序分配，`add_edge` 返回 ID，新增 `call_edge_id` / `edge` 查询）。
  `runtime/trace` 新增 `Trace` / `ExecutionSpan` / `SpanOutcome`：解释器每次 callable 进入开一条 span
  （pre-order，携带 `node_id` + 触发它的 `edge_id`（顶层 None）+ `depth`），执行完写回 `outcome`；
  `run_action` 返回值由 `(Outcome, InMemoryHost)` 改为 `Execution { outcome, host, trace }`（单一路径，
  所有调用点同步迁移）。确定性优先：span 不记墙钟时长（破坏可复现），只记图结构投影 + 进入序 `seq`；
  LLM 计量（tokens/cost）待 LLM 执行节点引入。4 项 trace 测试（单节点 / 跨调用投影边 / raise 结局 /
  重复调用有序）。
  ② **constraint audit verifier 执行器**。前置「与运行时接入」已满足：`runtime/verify` 新增
  `HiddenCase` / `ExpectedOutcome` / `run_hidden_case[s]`，在 v0 解释器上真正执行 hidden case 并与
  期望比对，产出 `VerificationResult`。分层守恒：执行属 runtime（不依赖 tools/audit），判定属
  tools/audit（消费注入的 `VerifierOutcome`），协调层零损耗映射。6 项 runtime verify 测试 + 1 项 CLI
  闭环测试（runtime 执行 → 映射 VerifierOutcome → audit 判定：通过放行 / 失败阻断 / 硬错误判 fail
  不伪造）。诚实性：hidden case 期望不喂被验证程序（防泄漏），只在执行后比对；任何无法确认匹配
  （含执行硬错误）一律判 fail。文档同步：impl §9.4（标记已实现 + 更新 ExecutionSpan 形状）、
  dev_checklist runtime / tools 两项。全工作区 292 passed / 0 failed，clippy（`-D warnings`）0 警告，
  fmt clean。剩余接线（非执行器空缺）：trace 的人读 / 持久化呈现、verifier 的 ConstraintView ref 投影 +
  hidden-case 隐藏存储后在图 gate 内自动驱动。
- 2026-05-30 — Trace 投影 CLI 呈现接线：`sophia run --trace` 打印 Execution Graph 执行 Trace 投影
  （render `print_trace`：按 depth 缩进 + `#seq` + callable + `node Nx` / `edge Ex`（顶层入口标注）+
  结局 return/raise）。`commands::run_action` 增 `with_trace` 形参（smoke 调用点传 false）；`Run`
  子命令增 `--trace` flag。新增 1 项 CLI 集成测试（跨调用程序的 trace 含顶层入口 + 调用边投影）。
  文档：architecture §9.1 命令表标注 `--trace`。全工作区 293 passed / 0 failed，clippy 0 警告，fmt clean。
  Trace 投影项至此「数据 + 执行器 + 人读呈现」三者就绪；持久化 / 计时计量仍按确定性原则留待 LLM
  执行节点引入。
- 2026-05-30 — 起草**隐藏验证用例存储**设计（hidden-case 来源建模，纯文档，无代码）。确立 regression
  gate 的 hidden case 正文如何存放而不泄漏给 LLM：三层结构性隔离——① 图节点 `ConstraintPayload`
  只存不透明引用 `verifier.ref`（已是现状）；② active context 的 `ConstraintView` 整体剔除 `verifier`
  字段（连 ref 都不投影，已是现状，本次明文写入 spec 5.6 作为 anti-cheat 纪律）；③ 用例正文存于
  **图外** `sophia-runs/verifiers/hidden.json`（`ref → HiddenVerifierSpec{entry_action, args, expected}`），
  与 `dev_graph.sqlite` 物理隔离，只有确定性 gate 在 materialize 时按 ref 取用，推导 active context 的
  代码路径根本不接触它。gate 流程与分层：执行属 runtime（`run_hidden_case`，已实现）、判定属 tools/audit
  （`audit_constraints`，已实现）、加载 hidden.json + 串联 + 写图属协调层；缺用例 / 缺运行器一律诚实
  硬错误阻断不伪造；hidden.json 由出题方写入、不由 LLM 产生。新增 `workflow_graph_spec.md` 五A 节
  （存储形态 + gate 取用流程 + 与 e2e harness 关系）+ 5.6 节（验证用例对 LLM 不可见纪律）；
  `language_design.md` §10.10 补 hidden verifier 三层隔离段；`engineering_architecture.md` §9.2.1
  接线与分层归属。设计就绪，待实现 `hidden.json` 加载器 + gate 内自动驱动（已登记 checklist 待办项）。
- 2026-05-30 — 实现**隐藏验证用例存储 + constraint_audit gate 自动驱动**（按前一条设计落地）。
  `runtime::{Value, HiddenCase, ExpectedOutcome}` 加 serde（单一值模型，不另设 VerifierValue 镜像；
  外部标签判别联合，如 `{"Int":42}` / `{"Returns":{"Int":42}}`）。新增 CLI `verifier_store` 模块：
  从图外 `sophia-runs/verifiers/hidden.json` 加载 `ref → HiddenCase`（缺文件=空存储、ref 唯一校验、
  与 dev_graph 物理隔离）。`run_constraint_audit` 改造：从 **ConstraintNode 原始 payload**（非
  `ConstraintView`，后者刻意不投影 verifier）读 `verifier.ref` → `run_hidden_verifiers` 在候选上
  parse+resolve+analyze 构模型 → `runtime::run_hidden_cases` 真正执行 → 零损耗映射 `VerifierOutcome`
  注入 `audit_constraints`；缺用例则不注入 → audit `MissingVerifierOutcome` 硬错误阻断（不伪造）。
  `build_selected_candidate` / `prepare_selected_candidate` 透传 `root` + 候选 files。三层 anti-cheat
  隔离落实：图存不透明 ref / `ConstraintView` 剔除 verifier（spec 5.6）/ 正文图外。分层守恒：执行属
  runtime、判定属 tools/audit、加载+串联+写图属 CLI 协调层；tools/runtime 均不感知 hidden.json 与图。
  新增 3 项 CLI 集成测试（hidden case 通过→select 放行且 ConstraintAudit 诊断 ok / 失败→阻断不建
  Selection / 缺用例→诚实 RegressionGate 阻断）。文档：dev_checklist tools 项 + 本项、architecture
  §9.2.1 状态、design §10.10 已在设计稿写明。全工作区 296 passed / 0 failed，clippy（`-D warnings`）
  0 警告，fmt clean。至此 constraint audit verifier 从「框架就绪」推进到「真实工作流端到端可驱动」。
- 2026-05-30 — 实现 **body 级 storage 操作**（§16.6 storage 扩展子集，v0 唯一有实感的功能缺口）。
  此前 `storage.X.get/save` 能 check、`run` 时硬错误失败；现贯通 type / effect / runtime：
  ① **semantic**（`type_layer::infer_storage_op`）识别 `storage.<Name>.get(key)` /
  `.save(key, value)` 形状——`get → Optional<ValueTy>`、`save → ValueTy`（v0 不引入 `Result<T,E>`，
  save 直接返回 value，非 Ok/Err）；并入 effect `DB.Read/Write("<Name>")`（走既有 used⊆declared +
  capability 检查，与声明式 effect 同一路径，无新增检查支路）；校验 key/value 实参类型与 storage 声明
  相容；未知 storage / 未知操作报诊断。② **runtime**（`interp::try_storage_op` + `effect_host`）解释器
  识别同一形状，经 `EffectHost::{storage_get, storage_save}` 委派；`InMemoryHost` 用按 storage 名分桶的
  内存 key→value 映射执行。设计取舍：key **显式传入**（Sophia「全部显式表达」原则，不靠 entity 字段名
  约定隐式推 key）；`save(key,value)` 返回 value（避免拉入 §16.6 仍排除的 `Result`）。HIR 名称解析
  注释更正（storage op 已落地执行，非"v0 不执行"）。新增 3 项 runtime + 4 项 semantic 测试，CLI 端到端
  实测（含 capability 的 storage action：check 通过 + `run --trace` save 42→get→返回 42）。文档同步：
  `language_implementation.md` §7.3 effect / §16.6（storage 扩展子集已落地、仍排除 Result/entity.with/
  合约证明）。设计文档 §5.3 的 CompleteTodo 全量示例（单参 save + Ok/Err）保持不变，是完整设计愿景；
  v0 storage 子集（双参 save、无 Result）见实现文档 §16.6。全工作区 303 passed / 0 failed，
  clippy（`-D warnings`）0 警告，fmt clean。至此 v0 唯一有实感的功能缺口已补齐——带持久化的真实存储型
  程序可端到端解释执行。
- 2026-05-30 — **全项目深度技术债清理**（行为不变，303 passed / 0 failed 全程绿，clippy
  `-D warnings` 0 警告，fmt clean）。经 context-gatherer 子代理系统勘察后逐项核实修复：
  ① **死代码 / 死 re-export 移除**：删除全工作区零调用的 `prompt::asset_names()`；删除仅 crate
  内部使用的 `graph-db` re-export `pub use event::GraphEvent`（类型保持 crate 私有）；删除 LSP 仅经
  `crate::convert::` 内部使用的 `pub use convert::{byte_to_position, position_to_byte, span_to_range}`
  （函数本身保留，仅去掉无谓的 re-export）。
  ② **LSP 接入 CLI**（真实缺口而非删子系统）：`sophia-lsp` 整个 crate 此前不可达、`run_stdio` 零调用
  ——新增 `sophia lsp` 子命令以 stdio 运行 Language Server（hover / diagnostics / goto definition），
  用多线程 tokio 运行时承载长驻服务。`cli/Cargo.toml` 增 `sophia-lsp` 依赖。
  ③ **冗余 Result+expect 消除**：`graph_cmd::render_design_request` 返回类型 `Result<CompletionRequest>`
  → `CompletionRequest`（内置模板 + 受控上下文，渲染失败属内部不变量，与 `CliImplementPrompts` 渲染 /
  `step_schema` 同构，以 `expect` 暴露而非伪装可恢复错误）；调用点去掉 `.expect()`。
  ④ **测试去重**：`graph-db` 的 `assessment.rs` / `decomposition.rs` 各自复制的 `snapshot` helper 抽到
  `workflow/graph-db/tests/common/mod.rs`（`#![allow(dead_code)]`，各测试二进制用子集）。
  ⑤ **陈旧文档注释更正**：`cli/src/main.rs` 头部 / `GraphCmd` doc、`graph_cmd.rs` 模块 doc 此前称工作流
  LLM 命令"随后续接入"——已如实列出确定性命令（含 `lsp`）+ LLM 命令两类；`graph_cmd` `context_files`
  留空注释改为如实指明"graph Objective ↔ 项目 action 链接尚未建模（见 §2.2），故诚实留空"；
  `core/hir/resolve.rs` 的 storage 根注释由"v0 解释器不执行"更正为"已落地解释执行"。
  核实判定为非债务、保留：`CliImplementPrompts` 渲染 `.expect()`（内置模板不变量）、decision/design/
  decompose/revise 的 `unreachable!`（API 误用守卫）、`context_files: Vec::new()`（§2.2 诚实"部分完成"
  项）、`#[allow(clippy::too_many_arguments)]`（async 函数线程化 store/client/prompts/budget/check，
  合理）、跨 crate temp-dir helper 重复（分属不同 crate 无法共享，强行抽取反增债）。无 TODO/FIXME/
  unimplemented!/todo! 残留。清理后建立本地 git 仓库（`main` 分支，无远端，见 engineering_notes Git 决策）。
- 2026-05-30 — 阶段 A（e2e 与验证缺口补全）。① **e2e 新增 G5 组（持久化 / storage）**：storage body
  操作（§16.6）落地后，补上 e2e 设计文档 §3.1 当初预留的存储型用例。G5-01 计量读数存储——
  `RecordReading(7,42)` 先 `save(meter_id, reading)` 再 `get(meter_id)` 读回返回 42，成功判据校验
  save→get 往返（验证 storage 真正经解释器 effect host 的分桶 key→value 映射执行）。据此给共享语法
  基线补一条决断性规则（顶层 `storage` 声明形状 + `save(key,value)`/`get(key)` 调用形状 +
  `DB.Read/Write("<名>")` effect 声明 + capability 绑定 + 中立 `ExampleStore`/`Remember` 示例，可泛化、
  不含任务答案）；防泄漏断言登记 G5 token（ReadingStore/MeterCapability/RecordReading/meter_id），
  baseline snapshot 同步更新。新增 `cli/examples/e2e/cases/g5_storage.rs` 并注册；批量脚本组前缀匹配
  天然支持 `g5`。e2e 设计文档同步（分组表 + G5 用例清单 + §3.1 + cases/ 树 + 变更记录）。
  ② **append-only / I9 CI 不变量测试**（落实「CI 不变量测试」工程待办）：`graph-db` 新增只读审计
  访问器 `GraphStore::raw_event_log`（按 seq 升序返回 append-only 日志的原始序列化记录，不外泄内部
  `GraphEvent` 表示），`tests/append_only.rs` 据此守护——每次写后旧日志逐字节为新日志严格前缀、
  被拒绝的非法写无副作用、重开库 replay 后历史逐字节保持。这是确定性测试、进 `cargo test` / CI
  （区别于不进 CI 的真实 LLM e2e）。③ 同步把 checklist 工程项「git 初始化」「CI 不变量测试」由
  尚未完成移至已完成。全工作区 305 passed / 0 failed（新增 2 项 append-only 测试），clippy
  （`-D warnings`）0 警告，fmt clean；prompt 防泄漏 / baseline snapshot 全绿，e2e example 编译通过、
  `--list` 含 G5-01。
- 2026-05-30 — 阶段 A2（目标树遍历的真实 LLM e2e 接线 + 子目标 binding 链路打通）。审核发现
  `run_goal_tree` 此前在 decompose 后直接递归 LLM 派生子目标，而子目标是 LLM provenance、**默认未
  绑定**（`is_bound` 仅对 human 隐式接受 / 链上有 AcceptanceEvent）——active context 为空，真实 LLM
  在子目标的 design/implement 拿不到自己的题面，无法实现（MockClient 单测因不看 prompt 内容而掩盖了
  该缺口）。这是 design 5.3「子目标须经人类接受 Decomposition 才沿 member_of 继承 binding」的真实
  能力缺口。**引擎修复**（最佳实现、非补丁）：`run_goal_tree` 新增**拆解审查者** `DecompositionReviewer`
  （人类授权检查点）——decompose 落图后、递归前回调，`Accept` → 建真实 human `AcceptanceEvent
  accepts→ Decomposition`、子目标继承 binding 进入 active context；`Reject` → 不递归 / 不伪造
  withdrawal（`GoalResolution::DecompositionRejected`）。提供 `AutoAcceptReviewer`（调用方代表人类
  授权，仍走真实 AcceptanceEvent 落图，非绕过 binding 谓词）。引擎不伪造人类授权（N4），授权权威留
  调用方。导出 `DecompositionReviewer`/`ReviewDecision`/`AutoAcceptReviewer`；`run_goal_tree` 全调用点
  同步加 reviewer 参数（单一路线）。**e2e harness 修复**：① prompt 提供者改 **focus-aware**（按 focus
  id 从 active context 取目标题面，对根焦点逐字等价、G1–G4/G3 无回归；树遍历中子目标获得自己题面）；
  ② 新增 `CaseKind::Tree` + `tree_drive`（经 `run_goal_tree` + `AutoAcceptReviewer` 驱动，合并子目标
  候选为一个程序执行）；③ 仅根焦点、尚无伪代码、Tree 类用例才把 decompose 列入 decision 候选动作。
  新增 G6 用例组（G6-01 温控面板拆成 CelsiusToScaled / FahrenheitOffset 两个独立 action 子目标）；
  防泄漏断言登记 G6 token。新增 2 项 traversal 单测（接受后子目标进入 active context / 拒绝不递归不
  伪造），既有 5 项适配 reviewer 后仍通过。文档同步：e2e 设计（G6 组 + 用例清单 + cases 树 + 变更
  记录）、architecture §8.5（拆解审查者）、engineering_notes 决策条目、本项。实现状态：引擎 binding
  链路由单测覆盖；G6-01 真实 LLM 实跑待 API key 环境（e2e 不进 CI、无 key 干净跳过）。全工作区
  307 passed / 0 failed，clippy（`-D warnings`）0 警告，fmt clean，e2e example 编译通过、`--list` 含 G6-01。
- 2026-05-30 — B 阶段：**内置 node 解释执行**（v0 node 执行子集，design §13.3.2 / impl §20.1）。此前
  `node`/`effect` 已贯通声明 + 静态检查但**不能运行**（`sophia run` 仅执行 action/transition），这是
  v0 最大的语义空洞。审定边界：语言**没有"node 实例连成图"的表层语法**，故多入/多出边调度器**无表层
  来源**——现在物化它就是为永不产生的边写死代码（违反单一路线 + 不伪造）。忠实子集落地三步：
  ① **exec-ir**：`ExecNodeKind::Node` + `from_model` 把 `model.nodes` 也建为执行节点（callable 之后、
  按名字典序）+ `ExecNode::is_node()`；node 间无表层装配边来源故不物化 node 间边。
  ② **runtime**：`EffectHost` 增 `invoke_node_effect(node, family, op, inputs)`，`InMemoryHost` 给确定性
  桩后端（Llm.Complete / Tool.Invoke / Stream.Emit 回显式映射，非伪造真实后端；不支持的 effect 族返回
  Err）；`Interpreter::run` 对 node 分流到新增 `run_node`——单输入单输出 + 恰好一个非 Pure effect 的
  node（PromptNode / ToolNode / StreamNode）经 EffectHost 分派执行，受与 callable 相同的 input/output
  validation，并在 Trace 留 span。多入/多出/Pure 结构性 node（router/aggregator）→ `RuntimeError` 诚实
  阻断（依赖 node 装配语法 + 多入/多出边调度，属后续工作）。`NodeDecl` 补 `multi_input`/`multi_output`
  标志支撑该判定。
  ③ **CLI**：无需改动——`sophia run <NodeName>` 经既有 `run_action` 路由即端到端可执行（实测
  `run ToolNode --arg text:hello --trace` 返回 `[tool:ToolNode] hello` + node span；多出边 node 失败
  退出码诚实阻断）。新增 1 项 exec-ir + 6 项 runtime 测试。文档同步：design §13.5（单 node 执行已落地、
  多入多出待装配语法）、impl §20.1、checklist core/runtime 项 + 本项、engineering_notes 决策条目。
  全工作区 314 passed / 0 failed，clippy（`-D warnings`）0 警告，fmt clean。至此 v0 声明的 node 可真正
  运行——agent-native 原语从"可声明可检查"推进到"单 node 可解释执行"。
- 2026-05-30 — **彻底移除 agent 编排 / node 构造**（方向纠偏，回归语言定位）。经审视确认：`node`
  顶层构造 + `Llm`/`Tool`/`Stream` effect + 内置节点（prompt/router/aggregator/tool/stream）+ 单 node
  解释执行，是经"stdlib 内置节点"这一未被独立论证的前提从侧门引入的 agent 编排能力，偏离 Sophia 的
  语言定位（LLM 是程序员、不是程序内置能力；编译器不调用 LLM）。按单一路线 + 不要过度设计，**彻底
  删除**（非 disable）：① grammar 删 `node_def`/`inputs_block`/`outputs_block`，ABI 15 重生成 parser.c；
  ② AST 删 `Item::Node`/`NodeDef`；③ HIR 删 `NodeKind::Node`/`resolve_node_def`，`BUILTIN_EFFECT_OPS`
  删 `Llm`/`Tool`/`Stream`（仅留 `Console`/`DB`）；④ semantic 删 `NodeDecl`/`model.nodes`/
  `check_node_contracts`/node 分析；⑤ exec-ir 删 `ExecNodeKind::Node`/`is_node`；⑥ runtime 删 `run_node`/
  `EffectHost::invoke_node_effect` 及桩；⑦ **删除整个 `sophia-stdlib` crate**（无任何消费者——编译器从
  Rust 内置表读 effect，不从 stdlib 加载；从 workspace members / dependencies 摘除）；⑧ 删除全部相关
  测试（syntax/HIR/semantic/exec-ir/runtime/stdlib）。**保留** `effect` 顶层构造 + `Family.Op(args)`
  通用引用——它与 agent 无关，是"消除 grammar 硬编码 effect、让 effect 可声明可扩展"这一独立正确的去债
  （内置 `Console`/`DB` 由 `hir::builtins` 承载，唯一真相源；用户可声明领域 effect）。文档同步：
  `language_design.md` §13 重写为「`effect` 顶层声明」+ §13.5「不引入 node 的设计决策」；`engineering_
  architecture.md` §4 重写为「内置 effect 与标准库（当前无 stdlib crate）」；`language_implementation.md`
  §20 / §7.3、`concepts.md`、README、Cargo.toml 注释全部更新。全工作区 298 passed / 0 failed，
  clippy（`-D warnings`）0 警告，fmt clean。
- 2026-05-30 — 一致性审查后的清理（文档/代码一致性 + 去除未用依赖）。审查发现并处理：
  ① **删除 6 个声明但零使用的 workspace 依赖**（违反「依赖按需引入」原则，制造技术栈假象）：
  `petgraph`（图可视化，未用）、`miette`（诊断，实际用 thiserror + 自定义类型）、`rmp-serde`
  （MessagePack，未用，属 §2.4 路线图但不应预先声明）、`bumpalo` / `slotmap`（arena/ID，AST 实际
  用自有 `ExprId(u32)`+Vec）、`pulldown-cmark`（.pseudo 解析，功能未实现）。均仅在根
  `workspace.dependencies` 声明、无任何成员 crate 引用、无源码使用；删后全工作区构建通过、
  Cargo.lock 同步清除（`bumpalo` 作为 chrono→wasm-bindgen 的传递依赖仍在 lock，属正常）。
  ② **保留 exec-ir `EdgeKind` 5 变体**：`Control` 是 v0 唯一产出，`Data`/`Stream`/`Conditional`/
  `Fallback` 为 impl §8.2 定义的边语义词汇表（设计截面，非投机依赖），在 `edge.rs` 注释标注
  「设计预留，v0 不产出」。③ 修正文档陈旧数字：成员 crate 13→**14**（删 stdlib 时 off-by-one）；
  prompt 资产 5/5→**6/6**（补 decompose 模板 + decompose_result schema，architecture §8.1 / §8.2 /
  checklist §2.1）；prompt 测试数 7→11、快照 2→4。④ architecture §9.2 把 `pseudo-* / check / audit /
  diff / verify` 等未实现子命令从「看似已实现」的 bash 块挪到明确的「路线图」小节，避免误导。
  ⑤ `pseudo_check` schema 标注「就绪、检查命令待接入」（诚实缺口，非过度设计）。全工作区
  298 passed / 0 failed，clippy（`-D warnings`）0 警告，fmt clean。
- 2026-05-30 — e2e 文档补 §5.5「中间产物全程在内存，不落盘」（明确 e2e harness 用
  `GraphStore::open_in_memory`、候选 `.sophia` 正文为内存 `Vec<(路径,正文)>`、全程不写文件系统，
  唯一可见形式是 stdout 经 `run_e2e.sh` 落盘为文本日志；与 CLI `graph` 路径落盘
  `sophia-runs/graph/artifacts/` 区分），回应"找不到 e2e 的 `.sophia` 文件"的疑问；原 §5.5
  顺延为 §5.6。无代码改动。
- 2026-05-30 — 新增 **`docs/benchmark_design.md`（基准测试设计草案，待讨论）**：定义一套横向对比
  基准——同一组小规模编程题、同一 LLM、同一隐藏验证用例下，比较「LLM 直接生成 Python/TypeScript」
  与「Sophia 工作流」的两个**核心指标——成功率 + 耗时**。两 mode：`sophia`（工作流，判定**复用
  既有 `runtime::verify`**，零新增执行能力）/ `baseline`（LLM 直接写主流语言模块，语言 Python/
  TypeScript 是 baseline 的参数而非独立 mode，需**新建**外部解释器子进程执行 + `Value↔JSON` 规约
  ——当前纯 Rust 工作区不具备，是主要工作量与依赖/安全决策点，文档诚实标注）。**关键设计**：题目
  用 **Rust `Problem` 结构 + 复用 `runtime::Value` / `runtime::verify::HiddenCase`** 表示（与 e2e
  把用例表示为 Rust `Case` 同构，不引入外部配置文件）；分级 L1–L5（能力维度）；产物只记核心两
  指标（`runs.jsonl` + 聚合表，不照搬外部 schema 的列）。与既有 e2e 目的区分（e2e 验"闭环能否
  跑通"，benchmark 做"横向对比"，两套独立入口/产物）。文档列出**待决策点**（baseline 语言范围
  倾向先 Python、入口位置倾向 `cli/examples/benchmark/`、是否复用 e2e 闭环倾向先不抽象），遵循
  "设计先行、确认后实现"纪律——**尚未实现**。无代码改动。
  - 注：初稿曾过度参考一个 TypeScript 外部原型（移植其 `task.json` 配置格式 / JSON 值 schema /
    失败枚举 / 报告列等 TS 特有形态）。因旧项目是 TypeScript、当前是 Rust 无实现可比性，已**重写**
    为上述从本工作区自身构件出发的设计——除"成功率 + 耗时"对比意图外不沿用任何外部实现形态。
- 2026-05-30 — **实现 benchmark**（`docs/benchmark_design.md` 草案 → 活文档）。落地
  `cli/examples/benchmark/`（多文件 example，与 e2e 对称，不进 cargo test）：① 题集 `problems.rs`
  L1–L4 共 6 题（abs_difference / is_even / rectangle_area / traffic_next / discounted_total /
  safe_divide），全新题目、复用 `runtime::Value` / `runtime::verify::HiddenCase`、与 e2e 用例刻意
  不重叠；② `problem.rs`：`Problem` / `EntrySig` / `NeutralTy`（Int/Bool/Record/State，按需引入）/
  `PublicBrief`——`public_brief()` 在**类型层**只暴露公开字段，hidden cases 无法流入 prompt（结构
  防线，比运行时断言更硬）；③ `value_json.rs`：`value_to_json` 把 `runtime::Value` 规约为语言中立
  JSON（State→取值名字符串），两 mode 共用；④ `sophia_mode.rs`：自带精简闭环（design→implement-loop
  →`runtime::run_hidden_cases`），**不复用 e2e harness**（先不抽象），但纪律一致（design 不注入语法
  基线、implement/repair 注入**同一份** `sophia_syntax_baseline` 资产、hidden cases 绝不进 prompt）；
  ⑤ `baseline_py.rs`：`complete_structured`（schema 恰好一个 `code` 字段）生成自包含 Python 模块，
  benchmark 拥有的 `runner.py` 夹具 `import` 候选调 `run_action(input)` 打 JSON 结局到 stdout，Rust 侧
  `judge` 与 `ExpectedOutcome` 对照（Returns→比 JSON 值、Raises→比异常类名）；受限临时目录 + 5s 硬
  超时 + `DirGuard` 用后清理（执行 LLM 生成代码的安全边界）；⑥ `report.rs`：`runs.jsonl`（append-only，
  `serde_json` 手工构造、不引 serde derive）+ `summary.md`（核心两指标聚合表 `level|task|mode|runs|
  passed|success_rate|avg_wall_time_ms`）。入口 `main.rs` 支持 `--task/--level/--mode/--runs/--label/
  --list`，无 `SOPHIA_LLM_API_KEY` 干净跳过、缺 `python3` 时 baseline mode 跳过（只跑 sophia）。
  **诚实性**：`sophia` 判定复用 `runtime::verify`（零新增执行能力），`baseline` 真正 spawn python3
  执行 + 跨语言对照（项目固有不对称，文档正视）；绝不伪造通过，子进程硬错误 / 超时如实归因。
  `python3` 仅运行期外部工具、**不进 Cargo 依赖树**。Python runner 协议经手工 smoke 验证（正常返回 +
  raise 两路径）。全工作区 **298 passed / 0 failed**，clippy(`-D warnings`) 0 警告，fmt clean；
  benchmark example 编译通过、`--list` 列出 6 题。真实 LLM 端到端实跑待 API key 环境。
- 2026-05-30 — **benchmark 首跑修正 + 真实 LLM 实跑 + 文档**。① 题目纠正：`is_even`（取模）/
  `safe_divide`（整除）超出 v0 解释器算子集（`eval_binary` 仅 `And/Or/比较/+ - *`，无除法 / 取模 /
  一元负号），会让 sophia mode 因语言不支持失败、污染对比；替换为 `within_budget`（`<=`）/
  `checked_subtract`（减法 + 比较的 error algebra），全部落在起步子集内。② 新增 `retry.rs`（有界重试
  client 包装，与 e2e RetryClient 同构、刻意不共享，两 mode 共用同一 client 容忍公网抖动）。③ 可观测性：
  `real_check` 打印每轮诊断、design 后打印伪代码字节数（与 e2e 一致）。④ 批量脚本 `scripts/run_benchmark.sh`
  （逐题各起一进程、日志落盘，与 run_e2e.sh 同构；先不批量跑、仅备用）。⑤ 逐题真实 LLM 实跑
  （deepseek-v4-flash，每题 1 次）：**baseline 6/6、sophia 4/6**；两处 sophia 失败均为真实发现（abs_difference
  模型固守一元负号 `-diff`、traffic_next 把 state `TrafficLight` 改名 `Light` 致 hidden case 运行时校验
  判负——诚实判负、未伪造），非 harness 缺陷。⑥ 凭证存 `.secrets/llm.env`（`.gitignore` 新增 `.secrets/`
  + `*.env` + `sophia-runs/benchmark/` + `sophia-runs/e2e-logs/`）。⑦ README 补 benchmark 入口与文档链接、
  INSTALL 补可选 `python3` 依赖 + benchmark 运行说明。clippy(`-D warnings`) 0 警告、fmt clean、全工作区
  298 passed / 0 failed。
- 2026-05-30 — **benchmark 两处 sophia 失败的根因分析 + 可泛化修复**（非补丁；语言层问题补设计）。
  ① **abs_difference（语言层缺口）**：grammar `unary_expr` 只有 `not`、无算术一元取负，模型反复写
  `-diff` 语法报错未收敛（等价 `0 - diff` 在子集内但模型不易发现）。判定为起步子集**意外缺漏**
  （§16.5 列"整数算术"却无取负、无排除理由）。**修复**：一元取负 `-x`（Int→Int）补进全链路——
  grammar 增 `-` 分支 → tree-sitter CLI 0.26.9 重生成 `parser.c`（ABI 15）→ AST `Expr::Neg` → lower 按
  op 分派 → 语义类型层（操作数 Int、结果 Int）→ 解释器（`-i`）；仍刻意**无除法 / 取模**。共享语法
  基线显式列出取负 + 标注无除法 / 取模。新增 runtime `unary_negation` 回归测试。`language_implementation.md`
  §16.5 更新。② **traffic_next（脚手架 / 命名保真）**：模型把题面给定的 state `TrafficLight` 泛化成
  `Light`，过 check 但 hidden case 运行时校验判负。**修复**：共享语法基线加"命名保真"规则——题面显式
  给出的名字必须逐字照用（防泄漏安全：只约束已在公开题面里的名字、不泄漏 hidden case；e2e 同样受益）；
  基线用中立名 `WidgetKind` 举例，防泄漏断言登记 benchmark 题集 token，snapshot 更新。③ **验证**：两处
  修复后逐题实跑确认转 PASS（abs_difference 用 `-diff` 一次修复收敛；traffic_next 保留 `TrafficLight`
  命名、hidden case 全过），sophia 由 4/6 → 6/6（baseline 始终 6/6）。两处均改**共享资产**（语言 grammar
  + 单一语法基线），非针对单题补丁。e2e §2.2 同步记命名保真规则。全工作区 **299 passed / 0 failed**
  （+1 unary_negation），clippy(`-D warnings`) 0 警告，fmt clean。
- 2026-05-30 — **benchmark 选题策略澄清 + L5 组合题 + 判定对称性修复**。明确两套测试的选题哲学：
  **e2e 重覆盖面**（coverage / 正确性回归，按正交能力维度铺开）vs **benchmark 重表现力**（与 baseline
  横向对比，题目按**单调递增难度阶梯** L1→L5 组织、每级累积叠加机制）；唯一共享的是底座（同一份可
  泛化防作弊的 `sophia_syntax_baseline` + 防泄漏纪律），题目刻意不重叠。代码：`problems.rs` 模块文档
  与分级注释改为阶梯叙述，新增 **L5 `checkout_limit`**（组合 entity 入参 + 跨调用 + 错误代数 + 标量
  算术，起步子集内，手工解 check 通过）。实跑 L5 暴露**判定口径**问题（非能力差距）：两级错误
  `error CreditError { variant OverLimit }`，baseline 首跑抛错误**类型**名 `CreditError`、而 hidden case
  按**变体**名 `OverLimit` 判定 → 判负。**修复**（baseline 契约澄清、防泄漏安全、可泛化）：两级错误时
  Python 异常类名取**最具体的 variant 名**（variant 名在公开题面、不泄漏；中立例 PaymentError/
  CardDeclined），两 mode 按同一具体失败身份对称判定，无双重接受 fallback。修复后 L5 两 mode 均 PASS
  （当前 sophia 7/7、baseline 7/7）。anti-leak 断言登记 L5 题集 token（OrderLine/LineAmount/CreditError/
  OverLimit/Checkout）。文档：benchmark_design §1.3/§3/§3.2/§5.1/§九 重写、e2e_test_design §3 标注覆盖面
  选题。clippy(`-D warnings`) 0 警告、fmt clean、全工作区 299 passed / 0 failed。
- 2026-05-30 — **进入 v1：阶段定位文档校准**（仅文档，无 v1 代码）。明确项目两个目标且主次分明：
  ① 主——做一门真正可用的、面向 LLM 无人介入的编程语言与工具链（严肃语言工程，**WASM codegen 是把
  玩具变严肃语言的既定必经步骤**）；② 次——发论文证明价值（依附目标 1，基准"成功率/耗时"只是可运行性
  证据之一、非中心价值）。据此校准 v1 路线为**两条并行工作流**：A WASM codegen（执行后端从解释器扩展
  到可部署 artifact；解释器转为等价 oracle / 差测试基线 + strip-assist artifact 字节比对）+ B 语言 /
  标准库扩充（`Result<T,E>` / error handle / `task` 执行 / `entity.with` / 跨 domain intent 数据流 /
  合约证明，按机器可证价值准入，支撑严肃程序与基准阶梯 L6+）。文档改动：`language_design.md` §1.1 新增
  两目标；`engineering_architecture.md` §14.2 重写为两工作流 + §14.3 纳入演化能力（edit transition /
  跨 domain / 更强 strip-assist，对应技术报告 S2–S4）；`language_implementation.md` §19.1 新增 v1 构建
  顺序；`benchmark_design.md` §3.1 标注阶梯随 v1 延伸至 L6+；`engineering_notes.md` 决策日志记此校准；
  本概述改为"v0 收尾 / v1 启动"。无代码改动，全工作区仍 299 passed / 0 failed。
