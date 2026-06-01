# Sophia 语言设计

> 本文档定义 Sophia 的语言概念和工作流概念，是面向 LLM 的"大语言"层设计。
> 实现细节（AST、IR、类型推导算法、检查器流水线）见 `language_implementation.md`。
> 工具链、目录结构、CLI 见 `engineering_architecture.md`。

---

## 一、定位

Sophia 是一门 **LLM-native 的确定性语义编程语言**，面向无人监管下的 LLM 自动编程。它要回答的核心问题是：

> 如果一个 LLM 没有大量代码预训练、并不擅长传统语法和惯例，但具备较强的自然语言语义理解能力，是否可以通过专门为它设计的语言、检查器和工作流，让它在没有人工审查兜底的条件下稳定完成自主编程？

Sophia 的回答是：

- 让 LLM 负责语义理解、任务分解、结构化表达和修复建议；
- 让语言、编译器和工具链负责确定性、边界、类型、副作用、错误和能力约束。

LLM 可以生成源码，但源码行为只能由形式语言和编译器决定。

Sophia 不是自然语言编程，也不是 prompt DSL，而是一门可编译语言。所有设计取舍优先服务 LLM 在无人监管自动编程中的成功率、可修复性、上下文裁剪和约束保持；人类阅读、手写、审查或运维便利不是设计目标。

### 1.1 项目的两个目标（主次分明）

本项目有两个目标，**第一个是严肃工程目标，第二个依附其上**：

1. **（主）做一门真正可用的、面向 LLM 的无人介入编程语言与工具链。** 这是严肃的语言工程，不是为
   发论文搭的玩具。"严肃语言"意味着它最终要有**真实的代码生成与可部署执行后端**——这正是 v1 引入
   **WASM codegen** 的意义：把 Sophia 从"只能解释执行的原型"推进为"可编译、可嵌入多种 host 运行"
   的语言。WASM 是既定的、必经的一步，不是可选项。
2. **（次）发表论文，证明其可用性与价值。** 论文的价值论证（代码预训练之外的 LLM 编程路线、
   intent/capability/effect 的 accept/reject 实验、图工作流的可回放可审计）是对目标 1 成果的**呈现**，
   而非目标 1 的替代。基准测试中"成功率/耗时"只是可运行性证据之一，不是项目的中心价值。

因此路线优先级以**目标 1（让语言真正可用）**为准：v1 同时推进 ① WASM codegen（执行后端从解释器
扩展到可部署 artifact）与 ② 语言能力 / 标准库扩充（支撑更复杂、更有说服力的程序与基准）。二者都是
"把玩具变成严肃语言"的组成部分。详细路线见 `engineering_architecture.md` §14。

---

## 二、两层系统

Sophia 分为两层，边界清晰且不可越界：

| 层               | 性质                     | 产物                                                                                     | 职责                                                  |
| ---------------- | ------------------------ | ---------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| **启发式探索层** | 非确定、可分叉、可失败   | Development Graph 中的各类节点（目标、决策、伪代码、代码、诊断、选择、物化等）           | 让 LLM 在受控空间提出候选方案，并保留版本和失败路径   |
| **确定性编译层** | 确定、可复现、可测试     | `.sophia` 源码、ASG index、diagnostics、解释器执行结果（v1 起增加 WASM artifact） | 解析、检查、审计、生成、构建和运行正式源码            |

两个铁律：

1. **探索过程可以非确定，正式源码和编译结果必须确定。**
2. **编译器不调用 LLM**：所有 LLM 调用只发生在工作流层；语言核心保持纯确定性。

LLM 的不可替代职责：

- 生成结构化 `.pseudo`；
- 把 `.pseudo` 实现为可通过确定性检查的 `.sophia` 候选；
- 进行 Development Graph 上的下一步动作选择；
- 参与 goal 分析与 repair。

LLM 不参与的职责：

- `sophia check`、`build`、`run` 的正确性判断；
- constraint audit、artifact diff、materialize preflight 的判定。

---

## 三、设计原则

| 原则                                 | 要求                                                           |
| ------------------------------------ | -------------------------------------------------------------- |
| LLM-native 表面                      | 语法、诊断、上下文、graph artifact 优先供 LLM 与工具消费       |
| Unattended Automation                | 自动检查、修复、物化门禁不得依赖人工审查作为安全网             |
| 确定性核心                           | 所有可执行行为必须由形式语法决定                               |
| 自然语言辅助而非语义                 | 自然语言字段只能辅助 LLM 理解，不能影响编译产物                |
| 全部显式表达                         | 输入、输出、错误、副作用、能力、状态转换、前后置条件必须显式   |
| 语义可恢复                           | 源码块必须让 LLM 能从局部上下文恢复语义                        |
| 文件系统 ASG                         | ASG（抽象语义图）是语义模型，物理上由目录和文件实现           |
| 同源同产物                           | 同源、同编译器、同 target 输出一致                             |
| 语义直观 · 无省略 · 不惧繁琐         | 语法优先语义直观、显式无省略；不仿造人类高级语言的抽象机制（泛型系统 / 模板 / 宏 / trait 等） |

核心哲学：**把所有需要"记忆"的东西，变成需要"表达"的东西**。LLM 最擅长局部表达，最不擅长跨上下文长期记忆。

> **语法设计准则（LLM-native，2026-05-30 补充）**：Sophia **不仿造人类编程语言**为压缩书写量、复用
> 抽象而生的机制——**不设计泛型系统、模板、宏、trait/typeclass、操作符重载、隐式转换**这类"语义弱、
> 靠记忆规则展开"的特征。取而代之，**优先语义直观、无省略、显式繁琐**的语法：宁可多写几行结构清晰、
> 局部即可读懂的代码，也不要一行需要跨上下文规则才能理解的"聪明"写法。理由直击 LLM 的能力画像——
> **语义强、记忆弱**：LLM 擅长把意图就地表达成结构，不擅长记住并正确套用一套抽象展开规则（如类型
> 推导、宏卫生、隐式 trait 解析）。繁琐对 LLM 不是成本（它不嫌写得多），但隐式 / 省略是真实成本
> （它要靠记忆补全被省略的部分，易错）。**注意区分**：已有的**封闭内置类型构造集**（`list of T` /
> `one of { ... }` / `schema of T` / Intent wrapper）**不是**用户可扩展的泛型系统——它们是固定的、语义
> 直观的一等构造，逐一内建、不可由用户参数化新建，符合本准则。`<>` 形式**专属** Intent wrapper；
> 结构类型一律用 `of` 关键字族（见 `docs/type_system.md`）。

### 3.1 LLM-native 特性准入规则

Sophia 是全新设计，不继承传统语言为人类手写、阅读、审查、IDE 习惯、生态惯例或历史兼容形成的设计包袱。任何特性进入语言核心，必须满足全部条件：

1. **LLM-consumable**：能进入 deterministic context，或能减少 LLM 需要记住/猜测的状态。
2. **Machine-checkable**：能由 parser、checker、audit、runtime validation 或 graph gate 检查。
3. **Closure-friendly**：依赖关系能形成明确 ASG edge，支持从 action/task root 计算最小语义闭包。
4. **Repair-guiding**：失败时能产生结构化 diagnostic 指导 LLM 修复。
5. **No human fallback**：不把 correctness、safety、merge、materialize 或 evolution decision 交给人工兜底。
6. **No legacy convenience**：如果主要收益是让传统程序员更熟悉、更短、更像现有语言，则默认拒绝。

### 3.2 关键设计取舍对照

| Sophia 选择                                | LLM-native 理由                                                                  | 拒绝的传统包袱                            |
| ------------------------------------------ | -------------------------------------------------------------------------------- | ----------------------------------------- |
| 语义化节点（entity、action、capability…） | LLM 从局部上下文恢复角色，不必从语法惯例猜测职责                                 | class/member、模块导入惯例、靠目录暗示    |
| 不追求简洁                                 | 简洁依赖隐式上下文与读者经验，会增加 LLM 的记忆负担                              | 短语法、隐式默认值、约定优于配置          |
| 语义直观 · 无省略 · 不惧繁琐               | LLM 语义强、记忆弱：就地显式表达不易错，省略 / 隐式展开要靠记忆补全、易错        | 泛型系统 / 模板 / 宏 / trait / 操作符重载 / 隐式转换 |
| 基于图而非线性文本                         | 工具从 root 计算确定性语义邻域，把最小闭包交给 LLM                               | 线性源码阅读、IDE symbol jumping          |
| 一个顶层语义节点一个文件                   | 文件边界即 ASG node 边界，prompt/diff/repair/materialize 都能稳定操作            | 多概念混在一个文件                        |
| 显式 effect / capability                   | 把隐式权限变成机器可拒绝的声明                                                   | ambient API、未声明 throw、隐式权限       |
| Intent Types                               | 把"数据经历过什么"变成类型，而不是对话记忆                                       | 注释、命名约定、开发者脑内数据流          |
| Error algebra                              | 错误是可枚举、可传播、可检查的节点                                               | ad hoc string errors、异常惯例            |
| Semantic Assist 不决定语义                 | 自然语言只能辅助生成与修复，不能改变行为                                         | 注释驱动行为、prompt DSL                  |
| `.pseudo → .sophia` 两阶段                 | LLM 先稳定算法语义，再降到形式 core                                              | 直接从自然语言生成代码                    |

---

## 四、两阶段编程：.pseudo 与 .sophia

```text
用户目标
  ↓ 设计
.pseudo            # 结构化伪代码，不可执行、不可编译
  ↓ 实现
.sophia            # 确定性 Sophia-Core 候选源码
  ↓ sophia check
检查 / 修复 / 修订 # 生成新节点，旧节点保持不变
  ↓ select
  ↓ materialize
  ↓ build / run
```

### 4.1 `.pseudo` 的职责与格式

`.pseudo` 在工作流中的唯一职责是稳定任务的算法意图，消除 LLM 在 implementation 阶段的语义歧义。

`.pseudo` 采用 **paper 风格 Markdown 伪代码**。理由是：

- `.pseudo → .sophia` 的转换者是 LLM，不是编译器。LLM 理解 paper 风格伪代码的能力比解析结构化字段更强。
- 强制结构化格式是在用机器的方式解决语义沟通问题。真正的 gate 在 `sophia check`，不在 `pseudocode_check`。

`.pseudo` 必须包含六个固定 heading；heading 下内容完全自由：

```markdown
<!-- sophia-pseudo: v1 -->

## Purpose
任意自然语言描述任务目标

## Inputs
描述输入名称和语义，格式自由

## Outputs
描述期望输出，格式自由

## Algorithm
用 paper 风格伪代码描述算法步骤。
可混合自然语言和伪代码符号，不要求可执行。

  for each item in list:
    if item satisfies condition:
      append transformed(item) to result
  return result

## Constraints
实现必须保留的语义约束

## Forbidden
不得引入的行为或副作用
```

`pseudocode_check` 的职责收窄为：解析 Markdown，验证六个 heading 是否存在。这是纯确定性检查，不调用 LLM。**heading 存在性是结构完整性保证，不是语义质量验证**；语义质量是写作纪律问题，由 DecisionNode 的评分字段（`pseudocode_clarity`）在工作流层面约束。

`.pseudo` 文件以 `<!-- sophia-pseudo: v1 -->` 形式记录 schema 版本。版本不匹配时 `pseudocode_check` 给出明确的迁移提示。

### 4.2 `.pseudo` 与 `.sophia` 的边界

| 内容         | `.pseudo`                              | `.sophia`                    |
| ------------ | -------------------------------------- | ---------------------------- |
| 任务意图     | 必须写清                               | 可在 `meaning` 中辅助说明    |
| 输入输出语义 | 必须写清，可不写完整类型               | 必须写完整 formal type       |
| 算法步骤     | 必须逐步写清                           | 必须转成 body 语句           |
| 循环与分支   | 必须写清次数/条件和状态更新           | 必须使用正式控制结构         |
| 副作用       | 写意图，例如打印、读库、写库           | 必须写 formal effect         |
| 错误路径     | 写语义分支（缺失、已完成、非法输入）   | 必须写 error algebra variant |
| 能力边界     | 写禁止事项和必要能力提示               | 必须写 capability allow/deny |
| 类型与约束   | 可粗略或留空                           | 必须完整、可检查             |
| 可执行性     | 不可执行、不可编译                     | 唯一可编译输入               |

铁律：

- 编译器只扫描 `.sophia`。`.pseudo` 只能存在于 `sophia-runs/graph` 或实验输入中。
- 如果 `.pseudo` 与 `.sophia` 不一致，**以 `.sophia` 为唯一程序语义**。
- 不应放入 `.pseudo` 的内容：完整 Sophia-Core 类型签名、formal effect 名、scaffold contract、capability/error algebra 细节、body 语法、`program { ... }` 这类伪 DSL、"handle properly"等模糊句子。

---

## 五、Formal Core 与 ASG

### 5.1 Formal Core vs Semantic Assist

| 层              | 元素                                                                                                                                | 是否决定语义或确定性工具行为 |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ---------------------------- |
| Formal Core     | `domain`、`entity`、`state`、`transition`、`error`、`capability`、`action`、`task`，以及其中的 field、body、requires、ensures、effects 等 | 是                           |
| Semantic Assist | `meaning`、`purpose`、`not`、`because`、`examples`、`anti_patterns`、`plan`、`repair_notes`                                         | 否                           |

**Strip-assist 等价**是硬约束：移除所有 Semantic Assist 字段后，Formal Core、IR 和（v1 起）codegen 结果必须完全不变。这条约束由 `sophia check` 在 IR 层强制执行；v1 起 `sophia build` 在 WASM artifact 层增加字节级比对。

### 5.2 ASG（抽象语义图）

Sophia 的语义模型是 ASG（Abstract Semantic Graph），物理上用 domain-first 文件布局实现：一个语义节点一个文件，顶层按 domain 聚合。

顶层结构不是传统 OOP 的 class/member 树，而是 domain 内的语义图。Entity、Action、Capability、Error、State 在文件系统中**平级**——这种平级不表示它们承担相同职责，也不表示 action 属于 entity。平级是为了让工具链能稳定索引、裁剪和组合语义上下文。

| 概念         | 角色                             | 不应承担                                            |
| ------------ | -------------------------------- | --------------------------------------------------- |
| `domain`     | 领域聚合边界和命名空间           | 不承载业务执行逻辑                                  |
| `entity`     | 领域概念、字段、不变量、语义身份 | 不执行 IO，不拥有 workflow，不隐藏 effect           |
| `transition` | 纯状态转换                       | 不访问 file、time、network、secret                  |
| `action`     | 可执行用例和 runtime entry       | 不隐式获得能力，不把 capability/effect 藏在 body 中 |
| `capability` | 能力沙箱和 effect 权限边界       | 不表达业务算法                                      |
| `error`      | 封闭错误代数                     | 不作为字符串约定散落在 body 中                      |
| `task`       | LLM 工作单位和 closure root      | 不是运行时单位，不改变程序行为                      |

> 文件 / 网络 / 数据库等 I/O 能力**不是**顶层节点，而是**标准库**（effect 族 + host），用 `effects` /
> capability 声明使用（见 `stdlib_design.md`）。

主要图边：

| 起点         | 边                 | 终点                                     | 含义                                                          |
| ------------ | ------------------ | ---------------------------------------- | ------------------------------------------------------------- |
| `action`     | `uses_type`        | `entity` / `state` / scalar              | action input/output/body 使用该类型                           |
| `action`     | `binds_capability` | `capability`                             | action 只能使用该 capability policy 允许且未 deny 的 effect   |
| `action`     | `declares_effect`  | `effect`                                 | action body 允许产生的副作用                                  |
| `action`     | `raises`           | `error.variant`                          | action 可显式抛出的领域错误                                   |
| `action`     | `calls`            | `transition` / `action`                  | 复用纯转换或受限调用其他 action                               |
| `transition` | `uses_type`        | `entity` / `state`                       | transition 输入输出的状态形状                                 |
| `entity`     | `has_field`        | `field`                                  | entity 的形式数据结构                                         |
| `entity`     | `has_invariant`    | `invariant`                              | entity 必须保持的语义约束                                     |
| `task`       | `includes`         | ASG node                                 | LLM 当前工作需要的语义闭包入口                                |
| `task`       | `excludes`         | capability/effect/network/secret         | LLM 当前工作禁止引入的能力边界                                |

### 5.3 ASG 节点示例

**Entity**：

```sophia
entity Todo {
  meaning: "A Todo is a user-created task item."

  not:
    "A Todo is not a calendar event."
    "Todo 不得包含认证数据。"

  fields {
    id { type: Uuid }
    title { type: Sanitized<Text> }
    status { type: TodoStatus }
    created_at { type: Time }
    completed_at { type: one of { Time, Null } }
  }

  invariants {
    TitleNotEmpty {
      require { self.title.length > 0 }
    }
    DoneHasCompletionTime {
      when { self.status == TodoStatus.Done }
      require { self.completed_at != Null }
    }
  }
}
```

**State**：

```sophia
state TodoStatus {
  value Pending { meaning: "这个 Todo 尚未完成。" }
  value Done    { meaning: "这个 Todo 已完成。" }
}
```

**Transition**（纯函数，禁止访问 file/network/secret）：

```sophia
transition CompleteTodoTransition {
  input  { todo: Todo where todo.status == TodoStatus.Pending; completed_time: Time }
  output { todo: Todo where todo.status == TodoStatus.Done }
  effects { Pure }
  body {
    return todo.with {
      status = TodoStatus.Done
      completed_at = completed_time
    }
  }
  ensures {
    output.todo.status == TodoStatus.Done
    output.todo.completed_at != Null
  }
}
```

**Error**：

```sophia
error TodoError {
  variant TodoAlreadyDone { id: Uuid; done_at: Time }
}
```

**Capability**（`deny` 优先于 `allow`）：

```sophia
capability TodoCapability {
  allow { Console.Write }
}
```

**Action**：

```sophia
action CompleteTodo {
  meaning: "Complete an existing pending Todo."
  capability: TodoCapability
  input  { todo: Todo; completed_time: Time }
  output { todo: Todo where todo.status == TodoStatus.Done }
  effects { Console.Write }
  errors  { TodoAlreadyDone }
  body {
    match todo.status {
      TodoStatus.Done    => raise TodoAlreadyDone { id = todo.id; done_at = todo.created_at }
      TodoStatus.Pending => {
        let updated = CompleteTodoTransition { todo = todo; completed_time = completed_time }
        print "completed todo"
        return updated
      }
    }
  }
  ensures {
    output.todo.status == TodoStatus.Done
    output.todo.completed_at != Null
  }
}
```

**Task**（LLM 工作单位，非 runtime 单位）：

```sophia
task ImplementCompleteTodo {
  goal: "Implement and verify CompleteTodo."
  include {
    entity Todo; state TodoStatus; error TodoError
    capability TodoCapability; transition CompleteTodoTransition; action CompleteTodo
  }
  exclude { Http.Get }
}
```

> 持久化 / 文件 / 网络等 I/O 不是语言节点，而是标准库（见 `stdlib_design.md`）。上例只用语言核心
> （entity / state / transition / error / capability / action / task）+ 内置 `Console.Write`；若需"把
> 结果落盘 / 取回外部数据"，用 `File` / `Http` 库（`effects { File.Write }` 等）。

---

## 六、类型系统

### 6.1 渐进类型

Sophia-Core 支持渐进类型。LLM 输出天然半结构化，强制完整类型标注会在生成阶段产生过高错误率。`Unknown` 类型在运行时退化为动态检查。

引入 `schema of T` 作为一等类型，表示"结构符合 schema T 的 LLM 输出"。类型不匹配在 Execution Graph IR 层触发 fallback 边，而不是 runtime panic。

### 6.2 Intent Types

Intent Type 描述数据经历过的语义转换，把"数据经历过什么"从对话记忆变成类型系统职责：

| Intent Type     | 含义                             |
| --------------- | -------------------------------- |
| `Raw<T>`        | 外部原始输入，未验证、未清洗     |
| `Parsed<T>`     | 已解析成结构化值                 |
| `Validated<T>`  | 格式或业务规则已验证             |
| `Sanitized<T>`  | 已清洗，可进入安全存储或展示路径 |
| `Verified<T>`   | 所有权、身份或外部事实已验证     |
| `Authorized<T>` | 权限已验证                       |
| `Secret<T>`     | 敏感值，不可普通输出             |
| `Redacted<T>`   | 已脱敏值                         |

赋值规则使用**严格相等**：`Raw<Text>` 不能赋给 `Sanitized<Text>`；`Sanitized<Text>` 也不能隐式降级为 `Text`。intent 转换必须通过显式的 `intent_conversion: true` action 完成（一入一出、同 inner type、不同 intent、无 effect、body 直接 `return`）。

输出边界例：`Console.Write` 只能输出字面量、`Sanitized<T>` 或 `Redacted<T>`。标准库的写出操作同理（如 `File.Write` 要求 `Sanitized<Text>`，见 `file_lib.md`）。

### 6.3 Effect 系统

采用代数效应（Algebraic Effects）模型，而非 capability 传染式模型。效应在调用边界显式标注，checker 验证传播完整性。

内置 effect 仅一个——`Console.Write`（输出原语，见 §13）；文件 / 网络 / 数据库等 I/O effect 族由**标准库**提供（`File` / `Http` / 未来 `DB`，见 `stdlib_design.md`），用 `effects` / capability 声明使用，与内置 effect 同一套三元组机制。

| Effect                | 含义                         |
| --------------------- | ---------------------------- |
| `Pure`                | 无副作用；与其他 effect 互斥 |
| `Console.Write`       | 写 stdout（内置） |
| `File.Read` / `File.Write` | 本地文件读 / 写（标准库 `File`，见 `file_lib.md`） |
| `Http.Get`            | 网络 GET 取回响应体（返回不可信 `Raw<Text>`，标准库 `Http`，见 `http_lib.md`） |

规则：

- Action body 内使用的所有 effect 必须包含在 `action.effects` 中。
- 被调用 action 的 observable effects 必须是调用方 effects 的子集；`Pure` 不要求调用方重复声明。
- 未声明的 effect 编译失败。

### 6.4 Capability

Capability 是能力沙箱。Action 必须绑定 capability，且 effects 必须被 capability allow 且未命中 deny。`deny` 优先于 `allow`。Capability 只描述能力边界，不描述业务算法。

### 6.5 Error Algebra

Error 是封闭代数类型：

- Action 必须显式声明可能 raise 的 error variant。
- `match` 必须显式穷尽；**Sophia 永久禁止 `_` catch-all**，避免新增状态、错误或分支时被默认分支静默吞掉。
- 外部 IO 错误必须显式映射为领域错误。
- 被调用 action 的 errors 必须由调用方继续声明（除非有 error handle）。

### 6.6 执行边的类型化

执行图的边在语言层面是一等概念，不是 retry / cancellation 语义的附属物：边可以携带类型化数据、流式传输、纯控制流、带谓词的路由或在节点失败时触发 fallback。`schema of T` 类型不匹配触发 fallback 边而不是 runtime panic。

完整的边类型枚举与运行时语义见 `language_implementation.md` 第八.2 节。

---

## 七、Body 子语言

Body 子语言故意受限，让低代码预训练 LLM 能稳定生成、检查和修复。

| 允许                                                               | 禁止                                |
| ------------------------------------------------------------------ | ----------------------------------- |
| `let`、`set`、`return`、`raise`、`if/else`、`match`、`repeat N times` | `while`、`for`、递归                |
| 变量、字面量、字段访问、完整 entity 构造                          | lambda、closure、高阶函数           |
| 比较、布尔表达式                                                  | operator overload、隐式转换         |
| `print`                                                            | 线程、async/await、共享可变全局状态 |

| 结构                                | 行为                                                                         |
| ----------------------------------- | ---------------------------------------------------------------------------- |
| `let name = expr`                   | 不可重新赋值                                                                 |
| `let mutable name = expr`           | 允许后续 `set`                                                               |
| `set name = expr`                   | 只能修改 mutable 局部变量                                                    |
| `return expr`                       | 必须与 action output 兼容；非 Unit action 全路径必须 `return` 或 `raise`     |
| `if cond { ... } else { ... }`      | cond 必须推断为 `Bool`                                                       |
| `match expr { Pattern { ... } }`    | expr 必须是 `Bool`、state 或 `one of { ... }`；case 必须穷尽（无 `_`）       |
| `repeat N times { ... }`            | `N` 必须是静态整数或已验证 bounded 值                                        |
| `print expr`                        | 需要 `Console.Write` effect 和 capability                                    |
| `EntityName { field = expr, ... }`  | 必须完整覆盖 entity 字段，且字段类型匹配                                     |
| `raise Variant { field = expr }`    | variant 必须在 action `errors` 中声明                                        |

作用域：

- action input 是 body 根作用域变量。
- `let` / `let mutable` 声明 block-scoped local；`if` / `repeat` body 创建子作用域。
- 子作用域可读取外层变量，可 `set` 外层 mutable 变量。
- block 内声明的变量不会泄漏到 block 外。
- `match` 的类型 pattern（`Int name =>`）/ variant pattern（`V { field } =>`）绑定的 name 仅在该 case body 内可见。
- **禁止 shadow 可见变量名**，避免 LLM repair 中出现同名局部造成语义漂移。

---

## 八、Task Closure 与 Semantic Paging

Sophia 工具链不让 LLM 读取整项目再推断相关性，而是从 root 计算确定性的语义邻域：把当前任务的最小闭包交给 LLM。

### 8.1 Action-rooted semantic context

`sophia context --action <ActionName>` 从 action root 出发：

1. 加入 root action；
2. 加入绑定的 capability；
3. 加入 input/output、entity fields、error variant fields 引用到的 entity 与 state；
4. 加入 `errors` 引用到的 error file；
5. 递归加入 body 中调用的 action；
6. 加入涉及的 domain file；
7. 输出显式 edge（`binds_capability`、`calls`、`declares_effect`、`allows_effect`、`denies_effect`、`raises`、`uses_type` 等），说明每个文件为何进入 context；
8. 输出 `sources`，按 `files` 同序携带闭包内源码内容；
9. 输出按路径、节点和 edge 排序，保证稳定。

### 8.2 Task closure

`sophia context --task <TaskName>` 是更大颗粒的语义邻域：

1. 从 `task.include` 节点出发；
2. 加入 formal dependencies；
3. 加入引用到的类型、错误、effect、capability、transition；
4. 加入所涉及 entity 的 invariants；
5. 应用 `task.exclude`；如果 formal 依赖被 exclude 命中则报错，不静默删除；
6. 输出按节点类型和名称排序。

Semantic Paging 是 task closure 的工具链升级：从 task 出发沿 ASG 做图邻域遍历，而不是依赖向量相似度 RAG。目标是降低 LLM 的 attention diffusion。

---

## 九、语义熵与演化边界

Sophia 不只关注当次编译正确，也关注长期迭代后的语义稳定。

### 9.1 Semantic Identity

Entity 可以声明语义身份，用于检测长期职责漂移：

```sophia
entity Todo {
  semantic_identity {
    core_capability: [
      "task.lifecycle.management",
      "user.intent.capture",
      "completion.state.tracking",
    ]
    forbidden_drift: [
      "user.authentication",
      "notification.delivery",
      "analytics.reporting",
    ]
    drift_tolerance: 0.15
  }
}
```

Entropy Detection 是工具链检查，不参与运行时语义。它发现"实体名称仍在、类型仍通过，但职责被多轮修改侵蚀"的情况。

### 9.2 Evolution Boundary

声明实体允许、禁止和需要门禁升级的演化方向：

```sophia
entity Todo {
  evolution {
    allowed:        [ "improve title validation precision", "add metadata fields" ]
    forbidden:      [ "add routing or scheduling logic", "introduce network side effects" ]
    requires_gate:  [ "adding new top-level fields", "changing status transition graph" ]
  }
}
```

Evolution Boundary 是前瞻性约束，Semantic Entropy 是回顾性监测。前者阻止明显越界，后者发现渐进漂移。

---

## 十、启发式工作流

工作流围绕 Sophia-Core 构建，是探索、生成、检查、修复、选择和物化的协议。它不是语言语义，而是 LLM 在 Development Graph 上的操作模型。

### 10.1 Development Graph

工作流不是线性的 `goal → pseudo → code → check → repair`，而是 append-only 的 Development Graph：

```text
ObjectiveNode
  └─ DecisionNode
       ├─ PseudocodeNode
       ├─ CodeNode
       ├─ DiagnosticNode
       ├─ repair_code  → CodeNode(v+1)
       ├─ revise_design → PseudocodeNode(v+1)
       └─ backtrack    → ancestor / sibling

Accepted CodeNode
  ↓
SelectionNode → MaterializeNode → domains/<Domain>/...
```

图规则：

- **节点不可变**：修改必须产生新节点。
- **失败路径不删除**：只标记为 failed、abandoned 或 superseded。
- **revise/repair/merge 都生成新节点**，并通过边连接来源。
- **选择由 SelectionNode 表达**，物化由 MaterializeNode 表达。
- `domains/` 只保存已选中且通过 gate 的正式源码。
- `sophia-runs/graph/` 保存探索过程。

### 10.2 节点本体（维度模型）

每个节点显式暴露三个维度，加一个由查询推导的第四维：

- **provenance**：节点内容的产生者（`human` / `llm` / `deterministic`）。由创建路径强制，schema 自身无法伪造。一旦写入即不可变。
- **role**：节点在本体中的角色（节点类型）。
- **versioning**：通过 `supersedes` 边在版本链中的位置。
- **binding**（推导）：节点是否进入 active context，由版本链 + 接受 / 撤销事件推导。**不是字段**。

provenance × role 的硬约束矩阵（哪些 role 允许哪些 provenance）见 `workflow_graph_spec.md` 第二节。其作用：

- 由 LLM 创建的 ObjectiveNode / MilestoneNode 的 provenance 必然是 `llm`；
- 它要进入 active context，必须存在一个 AcceptanceEventNode 指向它的版本链。

不存在 "derived" / "proposed" 这种隐式状态字段；committed = 链上能找到对应的 AcceptanceEventNode；其他即为 proposed。

### 10.3 节点目录

#### 目标簇

| 节点                      | 作用                                                                       |
| ------------------------- | -------------------------------------------------------------------------- |
| `ObjectiveNode`           | 单一可追踪目标                                                             |
| `ConstraintNode`          | 单条约束（kind: invariant / out_of_scope / preference / forbidden）        |
| `AcceptanceCriterionNode` | 单条验收条件                                                               |
| `DecompositionNode`       | 一次目标拆解事件，承载分解理由与候选评分                                   |
| `MilestoneNode`           | 阶段范围，作为一组 Objective 与 Constraint 的容器                          |

#### 变更簇

| 节点                | 作用                                                                |
| ------------------- | ------------------------------------------------------------------- |
| `ChangeRequestNode` | 人类提出的变更事件（kind: new_requirement / correction / 等）     |
| `AssessmentNode`    | 对变更或目标的结构化评估（risk / blast_radius / recommended_strategy） |
| `FirstSliceNode`    | 评估推荐的第一切片，结构同 MilestoneNode 的子集                     |

#### 事件簇（lifecycle 推进的唯一载体）

| 节点                  | 作用                                         |
| --------------------- | -------------------------------------------- |
| `AcceptanceEventNode` | 人类对一组节点的接受事件（驱动 binding）     |
| `WithdrawalEventNode` | 人类对一组节点的撤销事件（驱动 unbinding）   |
| `ActivationEventNode` | 把一个 bound milestone 变为 active           |
| `ClarificationNode`   | LLM 提问 / 人类回答的成对事件（kind 区分）   |

#### 推理与执行簇

| 节点                  | 作用                                                              |
| --------------------- | ----------------------------------------------------------------- |
| `ContextSnapshotNode` | 一次 LLM 调用所看到的 active context 视图（包含 SHA-256 digest） |
| `DecisionNode`        | 一次 LLM 决策或确定性 baseline 决策                               |
| `PseudocodeNode`      | 一份结构化 `.pseudo` 候选                                          |
| `CodeNode`            | 一份候选 `.sophia` 文件集                                          |
| `DiagnosticNode`      | 确定性检查结果（kind: pseudo_check / code_check / constraint_audit / artifact_diff / regression_gate） |
| `SelectionNode`       | 选择一个 CodeNode 作为 materialize 候选                            |
| `MaterializeNode`     | 把选中候选写入 `domains/` 的事件                                   |
| `RawLlmNode`          | LLM 调用失败的兜底节点（始终 creation_status=failed）              |

### 10.4 边目录（概览）

边的种类被刻意限制：每种边只允许特定 `(from.role, to.role)` 组合，工厂层强制校验。完整校验表（含 `T*` 多 role 端、附加约束）见 `workflow_graph_spec.md` 第六节。

按用途分组：

| 用途             | 主要边                                                                                |
| ---------------- | ------------------------------------------------------------------------------------- |
| 版本             | `supersedes`                                                                          |
| 拆解 / 阶段      | `decomposes`、`member_of`、`groups`                                                   |
| 约束 / 验收      | `constrained_by`、`requires`、`excludes`、`validated_by`                              |
| 变更 / 评估      | `targets`、`assesses`、`affects`、`proposes`                                          |
| 人类授权事件     | `accepts`、`withdraws`、`activates`                                                   |
| 澄清             | `answers`、`asks_about`                                                               |
| 推理来源         | `consumed`、`considers`、`addresses`                                                  |
| 修订与修复       | `revises`、`implements`、`repairs`                                                    |
| 检查 / 选择 / 物化 | `checks`、`selects`、`materializes`                                                  |
| 失败兜底         | `attempted`                                                                           |

### 10.5 Append-only 不变量

- **N1**：节点内容不可变。
- **N2**：边集合只增不减。
- **N3**：状态变更通过 successor 节点 + `supersedes` 边表达。
- **N4**：人类授权事件由专用节点承载（AcceptanceEventNode、WithdrawalEventNode、ActivationEventNode）。
- **N5**：`active` / `bound` / `accepted` 是查询，不是字段。
- **N6**：provenance 由创建路径强制，schema 自身无法伪造。

进一步的 schema-level 不变量（`(role, provenance)` 校验、悬空引用、CI diff 检测等）见 `workflow_graph_spec.md` 第三节。GraphStore 接口约束见 `engineering_architecture.md` 第六节。

### 10.6 撤销 vs Supersedes

- **supersedes**："我替换它"——双方在同一版本链中，新节点接管语义。
- **withdraws**："它不再有效但没有替代品"——失败的拆解、放弃的 spike、被否决的变更。

撤销不删除任何旧节点；它只让 binding 查询返回 false。

### 10.7 Active Context

Active context 是确定性管线根据图当前状态计算出的视图，喂给 `ContextSnapshotNode` 与下游 LLM 调用。它**不存任何字段，每次重新计算**。

binding 谓词的核心：

```text
N is bound iff
  N is the head of its version chain at T, AND
  ( provenance(N) == human  OR
    exists AcceptanceEventNode a such that
      a accepts→ some y in chainOf(N) ),
  AND there does NOT exist a later WithdrawalEventNode w such that
      w withdraws→ some y in chainOf(N).
```

`provenance == human` 隐式视为已接受。`milestone` 的 active 状态额外要求链上存在最新的 `ActivationEventNode`。binding 沿 `member_of` / `groups` / `requires` 单向、显式继承（DecompositionNode → 子 Objective、MilestoneNode → groups 下游目标和 requires 不变量）。

完整的推导算法、ActiveContext 序列化形状、SHA-256 digest 规范见 `workflow_graph_spec.md` 第五节。

每个 LLM-provenance 节点（DecisionNode / PseudocodeNode / CodeNode / AssessmentNode / DecompositionNode）必须有一条 `consumed→ ContextSnapshotNode` 边，保证：

- 任何 LLM 输出都能 100% 复现它当时看到的 context；
- anti-cheat 审计：snapshot 内容是否包含不该出现的 hidden case 数据；
- 可比较 snapshot：发现两次调用看到的 context 不同导致不同输出。

### 10.8 决策与动作选择

LLM 决策必须基于 **action-space scaffold**，不是自由聊天。prompt 只提供当前节点摘要、祖先链、相关诊断、预算和 action-rooted semantic context，**不提供 validation-only hidden expected output**。

scaffold 的职责是：缩小安全动作空间、降低无关记忆负担、提供可校验 JSON 形状。它**不替 LLM 选择下一步**——动作选择必须由 LLM 产生 DecisionNode。

DecisionNode 的 `state_assessment` 是 discriminated union（按 kind 分别 schema 化），避免把代码层评估字段强加给目标层决策：

| state_assessment_kind | 字段                                                                                                          |
| --------------------- | ------------------------------------------------------------------------------------------------------------- |
| `goal`                | goal_size, decomposition_pressure, active_milestone_present, outstanding_clarifications                       |
| `code`                | has_pseudocode, has_code, compile_status, error_type, repair_attempts                                          |
| `change`              | blast_radius, risk, affects_active_milestone                                                                   |

核心动作：

| 动作               | 用途                                |
| ------------------ | ----------------------------------- |
| `design_solution`  | 先写结构化 `.pseudo`                |
| `implement_design` | 把 `.pseudo` 转换为 `.sophia`       |
| `repair_code`      | 根据结构化 diagnostics 修复候选代码 |
| `revise_design`    | 当错误反映概念问题时重写伪代码      |
| `decompose`        | 目标过大时拆成子目标                |
| `backtrack`        | 当前路径超预算或违反父约束时回退    |
| `select`           | 选择通过 gate 的候选                |
| `materialize`      | 把选中候选写入 `domains/`           |

动作选择和动作执行必须分离：先生成 DecisionNode，再执行动作。

参考决策原则（非确定性节点选择器）：

1. CodeNode 通过 check/verify → `select`
2. CodeNode 错误 local 且未超预算 → `repair_code`
3. CodeNode 错误 conceptual → `revise_design`
4. 有清晰 `.pseudo` 但无 CodeNode → `implement_design`
5. 无 `.pseudo` 且目标 small/medium → `design_solution`
6. 目标 large 或跨多个 domain → `decompose`
7. 超预算或父约束被违反 → `backtrack`

放弃 LLM 节点选择能力会让 Sophia 退化为固定流程执行器，无法处理信息不足、概念性错误、预算取舍和回退路径。只运行 action-space scaffold 或 baseline 的结果不得计入"LLM 能进行启发式节点选择"的实验结论。

### 10.9 预算与评分

```text
budget {
  max_depth: 6
  max_children_per_node: 3
  max_repair_attempts_per_code_node: 2
  max_pseudocode_versions_per_goal: 3
  max_total_nodes_per_goal: 40
}

score {
  compile: 0.0..1.0
  tests: 0.0..1.0
  constraints: 0.0..1.0
  simplicity: 0.0..1.0
  locality: 0.0..1.0
  capability_minimality: 0.0..1.0
  pseudocode_clarity: 0.0..1.0
  overall: weighted_sum
}
```

如果 `compile = 0`，`overall` 最高不得超过 `0.49`，防止"语义合理但不可编译"的候选被选中。

**评分不是图节点**：`workflow_graph_spec.md` 第二节的 role 目录**没有 `Score` 角色**，选择只由
`SelectionNode { rationale }` 表达。因此 `score` 是确定性管线的**内存选择启发式**，用于在多候选间
排序，不持久化进图：编排层据排名选出 winner，再为它建一个 `SelectionNode`（rationale 记评分摘要，
可审计），单候选时退化。诚实性要求：compile / tests / constraints 取自确定性 gate 报告的真实信号，
simplicity / locality / capability_minimality 由候选源码可度量属性按明确公式计算，pseudocode_clarity
无信号时取中性值、不在代码侧伪造。若未来要把评分图化，应引入新 role 而非给 `SelectionNode` 加字段
（与 schema 演进策略一致）。实现位于 `tools/materialize`（`score_candidate` / `rank_candidates`）+
engine（`run_ranked_selection`），落地详情见 `dev_checklist_v0.md`。

**预算分层**：`max_repair_attempts_per_code_node` 由 implement-loop（implement→check→repair 闭环）
就地强制；`max_depth` / `max_pseudocode_versions_per_goal` / `max_total_nodes_per_goal` 由 goal 推进
调度器（spine）强制。调度器是"**动作选择 + 执行委派**"的薄层：每轮取一个 LLM `DecisionNode`，对
`design_solution` / `implement_design` / `revise_design` 直接委派执行；`needs_clarification` emit 一个
`Clarification(Question)` 后让位。这保证"动作选择必须由 LLM 产生"（10.8）的同时，不让调度器变成
塞满分支语义的大杂烩。

**目标树遍历层（decompose / backtrack）**：`decompose`（动作 6）/ `backtrack`（动作 7）是**非线性树
操作**，spine 刻意不内联处理而是让位；由 spine **之上**的独立遍历层承接，保持分层纯净。设计要点：

- **decompose**：遍历层据 LLM 给出的拆解结构建 `Decomposition` + 子 `Objective`（`parent
  decomposes→ Decomposition`、子目标 `member_of→ Decomposition`），再**递归**把 spine 驱动到每个
  子目标。`Decomposition` 是该动作的 **LLM 执行产物节点**（承载 LLM 生成的 rationale 与拆解结构），
  与 `Pseudocode` / `Code` / `Assessment` 同属 LLM 输出，故它**自身** `consumed→ ContextSnapshot`
  （I6）——锚定在"产出这次拆解的 LLM 调用"的 snapshot 上，区别于触发它的 `DecisionNode`（那是另一次
  "该不该拆"的决策调用，动作选择与执行分离）。子 `Objective` 是结构性派生节点（类比 assessment 协议的
  FirstSlice / Constraint），经 `member_of` 间接锚定。binding 不伪造：LLM 派生子目标默认未绑定，
  人类接受该 `Decomposition` 后才沿 `member_of` 继承（5.3）。
- **backtrack**：遍历层**放弃当前分支**。append-only 图保留被放弃的子树，**不伪造 `WithdrawalEvent`**
  （撤销是人类权威，N4），也不臆造"自动改道恢复"。

遍历层有自己的树预算（限 decompose 嵌套深度与目标总数）防递归爆炸；动作选择仍由 spine 内的
`DecisionNode` 产生。分层实现见 `engineering_architecture.md` §8.5。

**Prompt 在调用时刻渲染（硬性要求）**：调度器每一步的 prompt **必须在该步即将调用 LLM 时、据当前图
状态重新渲染**，与该步建立的 `ContextSnapshot` **同源**（10.7）——prompt 是 LLM 看到的全部世界，
必须等于 snapshot 所审计的那一份。不得预渲染一次后跨轮复用（会导致状态不演进、snapshot 失真、实现步骤
拿不到刚 design 的伪代码）。工程实现（`StepPrompts` 提供者）见 `engineering_architecture.md` §8.4。

### 10.10 Materialize Gate

`graph materialize` 是唯一可以把候选 `.sophia` 写入 `domains/` 的 graph 命令，必须满足：

- 候选 CodeNode 已被 SelectionNode 选中；
- 最近的 DiagnosticNode (kind=code_check) 为 pass；
- Constraint audit（DiagnosticNode kind=constraint_audit）通过；
- Strip-assist / artifact diff gate（DiagnosticNode kind=artifact_diff）通过；
- 起步阶段：解释器对该候选的全套 runtime input/output validation 通过，hidden verifier 不泄漏 prompt；
- v1 起：候选的 WASM build 与 host 侧 preflight 也通过。

materialize 必须是原子操作：先写临时目录，preflight 通过后再替换目标文件。Materialize 顺序由编译期类型状态保证（参见实现文档）。

**gate 跨进程重跑**：编译期类型状态证明（候选已过全部 gate）**无法序列化跨进程持久化**，而 `select`
与 `materialize` 是两个独立 CLI 进程。故各自从候选 artifacts（`sophia-runs/graph/artifacts/`，未物化
正文）**重新加载并重跑全部 gate** 重建证明——对"唯一不可逆写盘"这是更稳妥的姿态：写盘依据是
materialize 时刻的 gate 结果，而非可能过期的历史 select 结论。候选正文存于 artifacts 而非图节点
（`CodeNode` 只记路径），保持图轻量不可变。类型状态模式细节见 `language_implementation.md` 第十五节、
`engineering_architecture.md` §9.2。

**hidden verifier 不泄漏（防答案泄漏，最要紧）**：constraint_audit gate 的 regression 由 bound
invariant 的 hidden case 驱动；hidden case 的「期望输入 / 输出」是 **validation-only** 数据，绝不能让
被验证的 LLM 看见（10.8）。三层隔离保证它结构性不可泄漏：① 图节点只存不透明引用 `verifier.ref`，
不存用例正文；② active context 的 `ConstraintView` **整体剔除** `verifier` 字段（连引用名都不投影给
LLM）；③ 用例正文存于**图外的隐藏存储**（`sophia-runs/verifiers/hidden.json`），与 Development Graph
物理隔离，只有确定性 gate 在 materialize 时按 `ref` 取用、在候选上**真正执行**并与期望比对。若声明了
`HiddenCase` verifier 但隐藏存储缺对应 `ref`（或运行器未接入），gate **诚实阻断**（硬错误），绝不
伪造通过。完整 schema 与 gate 流程见 `workflow_graph_spec.md` 五A 节。

---

## 十一、不变量清单

实现层应当用单元测试或 schema-level 静态分析覆盖：

- **I1**：每个节点都通过 strict schema 校验（meta + payload，多余字段拒绝）。
- **I2**：`provenance` 与 `role` 的组合在允许集合中。
- **I3**：边的 `(from.role, to.role, type)` 组合在允许集合中。
- **I4**：`supersedes` 链不成环且两端 role 相同。
- **I5**：每个被指向的 NodeId 必须存在，不允许悬空引用。
- **I6**：每个 LLM-provenance 节点必须有 `consumed→ ContextSnapshotNode` 边。
- **I7**：ActivationEventNode 的 to 必须是 bound MilestoneNode。
- **I8**：`creation_status=failed` 仅出现在 RawLlmNode 上。
- **I9**：节点和边一旦写入即只读；CI 中的 diff 检测测试守护。
- **I10**：active context 推导仅依赖图的当前状态，不依赖任何节点 mutable 字段。

---

## 十二、Non-goals

- 不追求人类手写简洁性、阅读友好性或审查友好性。
- 不把 human-in-the-loop 当作 correctness、safety 或 repair 的依赖条件。
- 不允许自然语言决定行为。
- 编译器不调用 LLM。
- 不做复杂泛型、trait/typeclass、宏、反射、动态 eval、async、线程或分布式事务。
- **不仿造人类高级语言的抽象 / 压缩机制**：不设计用户可扩展的泛型系统、模板、宏、操作符重载、隐式
  转换、省略式语法糖（如 `?` 错误传播）。语法优先语义直观、显式无省略，繁琐对 LLM 不是成本、隐式才是
  （见 §3 语法设计准则）。封闭内置类型构造集（`list of`/`one of`/`schema of`/Intent）是固定一等构造，
  不构成泛型系统，不在此列。
- 不暴露动态 SQL、原始网络（socket/TCP/TLS）、随机数和复杂 runtime。文件 / 网络等 I/O 由**标准库**
  经 effect + capability 受控提供（功能层、非协议栈，见 `stdlib_design.md`），**无 ambient authority**
  ——不开任何"隐式可访问文件系统 / 网络"的后门。
- v0 不做任何 codegen；v1 仅做 WASM；JIT、native lowering、TS / Python 等具名语言 emit 都不在默认路线上。
- 不实现分布式执行（checkpoint/resume 语义在 IR 层定义，但不跨进程）。
- 不追求与 LangChain、LangGraph 等框架的兼容性。
- `pseudocode_check` 不做语义质量判断，只验证结构完整性。

---

## 十三、`effect` 顶层声明

> 本节定义 `effect` 顶层构造。它把"有哪些 effect 族 / 操作 / 参数形状"作为**可声明、可被名称
> 解析校验**的一等语义事实，而非埋进解析层。遵循 Formal Core 既有纪律（显式 effect / capability、
> 封闭代数、强类型、确定性）。各层实现见 `language_implementation.md` 第二十节。

### 13.1 设计动机

把 effect 做成可声明的顶层构造（而非把固定的 effect 集合内建进语法），出于两点：

1. **可扩展**：领域 effect（如某项目自己的 `Payment.Charge`）可由用户声明，无需改动语言语法；
2. **语义不埋进语法**：「有哪些 effect 族 / 操作 / 参数形状」是语义事实，应由声明决定、可被名称
   解析校验，而不应由解析器分支隐式暗示（呼应§3「语义由声明决定、不由语法惯例暗示」）。

`effect` 顶层构造声明一个 effect 族及其操作，使 `Family.Op(args)` 成为可被 capability `allow`/`deny`、
可被 `effects` 块引用、可被名称解析校验的一等 effect，而非硬编码。内置的 `Console` 族（输出原语）契约
本质上就是"声明一个 effect 族及其操作"；文件 / 网络等 I/O effect 族由标准库提供（同一机制）。

### 13.2 语法

```sophia
effect Console {
  meaning: "标准输出副作用族。"
  operation Write
}

effect Payment {
  meaning: "示例：领域自定义 effect 族。"
  operation Charge { param amount: Int }
}
```

- `effect <Family> { <operation>... }`：声明一个 effect 族及其操作。`<Family>` 为 PascalCase
  标识符，全局唯一（与其他顶层节点同命名空间，禁止重名）。
- `operation <Op> { param <name>: <Type> ... }`：声明一个 effect 操作及其参数形状（0..N 个
  `param`）。`<Op>` 在所属 family 内唯一。参数类型用与字段相同的 `type` 语法。
- 允许 `meaning` / `purpose` 等 Semantic Assist 字段（不决定语义，strip-assist 移除后不变）。
- effect 是**纯声明**，无 body、无实现——实现由运行时 `EffectHost` 提供。

### 13.3 effect 引用语法

引用一个 effect 操作的语法统一为 **`Family.Op` 或 `Family.Op(args)`**（带参数时实参为字面量或
绑定名），出现在三处：capability 的 `allow`/`deny`、callable 的 `effects` 块、task 的 `exclude`。

```sophia
effects { File.Read; Console.Write }
allow   { File.Read; File.Write; Console.Write }
```

- `Pure` 是保留字，表示空 effect 集（与任何具体 effect 互斥），不是某个 family 的操作。
- 参数限定为标量字面量（`Text` / `Int` / `Bool`）或当前作用域内可见的绑定名；参数的类型
  必须与 `operation` 声明的 `param` 类型相容。

### 13.4 effect 相等与子集语义

effect 的规范化表示是 **`(family, op, args)` 三元组**：

- **相等**：family、op、全部 args 都相等才算同一 effect（`File.Read ≠ Http.Get`，带参 effect 如
  `Payment.Charge(1) ≠ Payment.Charge(2)`）。`Console.Write` 即 `(Console, Write, [])`。
- **子集 / 传播**：`used ⊆ declared`、"被调用方 effect ⊆ 调用方 effect"、capability `allow`/`deny`
  匹配（deny 优先）等规则见 §6.3 / §6.4——比较对象是三元组。
- effect 集合不含 `Pure`（空集即纯）。

effect 族来自两处、并入同一符号表：内置 `Console` 族 + 标准库 effect 族（`File` / `Http`，见
`stdlib_design.md`）由编译器内置表（`hir::builtins`）预置（`core` 零 IO，不自举解析源文件），用户
`effect` 顶层声明并入。相等 / 子集 / capability 匹配算法只比较三元组，与来源无关。

### 13.5 不提供 `node` 顶层构造与 agent 编排（设计边界）

Sophia **不提供**用于 agent 编排的 `node` 顶层构造（无 body 的内置节点接口契约），也不内置
`Llm` / `Tool` / `Stream` 等面向"程序调用外部 LLM / 工具"的 effect 族。这是一条明确的设计边界：

- **属于语言定位之外**：Sophia 的定位（§1）是 **LLM-native 的确定性语义编程语言**——LLM 是
  **程序员**（在工作流层写 `.sophia`，由不调用 LLM 的编译器检查）。"让程序本身调用 LLM / 编排
  agent 流水线"是另一个方向（agent-native），不是本语言的设计目标。
- **缺乏内聚的执行模型**：一个无 body 的节点构造只有在"被装配进执行图、由调度驱动"时才有意义，
  而这需要一套"把节点连成图"的装配语法与多入 / 多出边调度。在没有该装配模型时，单独引入节点
  构造会得到一个既不能被 `action` 调用、又无图可入的悬空构造——没有内聚的执行语义。

`effect` 顶层构造（§13.1–13.4）**不在此边界内**：它与 agent 无关，解决的是"让 effect 可声明、
可扩展，而非硬编码进语法"。用户用它声明领域 effect 族，内置 `Console` 族 + 标准库 effect 族
（`File` / `Http`）由编译器内置表承载。

若未来确需 agent 编排能力，应作为**显式的语言方向决策**整体设计（节点装配语法、执行图多入 / 多出
边调度、与确定性核心的边界），而非作为标准库副产品零散引入。

### 13.6 范围与非目标

- effect 参数当前限标量字面量与绑定名；不引入 effect 多态、effect handler、effect 推断。
- effect 仅声明契约；其运行时实现由 `EffectHost` 提供（可执行的可观测 effect 为内置 `Console.Write`
  与标准库 `File.Read/Write`、`Http.Get`〔mock host；真实文件 / 网络见各库 host〕，见
  `language_implementation.md` / `stdlib_implementation.md`）。

---

## 十四、范围之外

本设计刻意不处理：

- 跨图（多 workspace）合并；
- 节点的删除、垃圾回收、归档（append-only 不允许节点删除；归档由外部脚本 + 索引快照完成）；
- 节点 payload 加密或权限控制；
- 节点的 evolutionary schema 升级（当前假设 schema 演进通过引入新 role 而非升级旧 role 解决）。
