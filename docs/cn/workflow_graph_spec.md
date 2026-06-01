# Sophia 工作流图规范

> 本文档是工作流图（Development Graph）的**规范层**：节点 payload 严格 schema、边的 `(from.role, to.role, type)` 校验集合、Append-only 不变量、Active Context 推导算法。
> 节点本体的概念（四维度模型、节点角色目录、动作选择原则、预算与评分、Materialize Gate）见 `language_design.md` 第十节。
> Rust 端实现细节、GraphStore 接口约束见 `language_implementation.md`、`engineering_architecture.md`。

本规范是 schema 校验代码、prompt 模板里 JSON 形状描述、CI 不变量测试的 source of truth。设计文档不重复本规范的字段细节；本规范不重复设计文档的概念解释。

---

## 一、通用结构

### 1.1 标识与枚举

```rust
pub struct NodeId(pub u32);

pub enum Provenance {
    Human,
    Llm,
    Deterministic,
}

pub enum NodeRole {
    Objective,
    Constraint,
    AcceptanceCriterion,
    Decomposition,
    Milestone,
    ChangeRequest,
    Assessment,
    FirstSlice,
    AcceptanceEvent,
    WithdrawalEvent,
    ActivationEvent,
    Clarification,
    ContextSnapshot,
    Decision,
    Pseudocode,
    Code,
    Diagnostic,
    Selection,
    Materialize,
    RawLlm,
}

pub enum NodeCreationStatus {
    Ok,
    Failed, // 仅 RawLlmNode 允许
}
```

`NodeId` 在生成事件时分配；推荐格式 `N{4 位以上零填充十进制}`，例如 `N0001`。

### 1.2 NodeMeta

每个节点都是 `meta + payload`。`meta` 在所有节点类型上一致，承载维度信息；`payload` 由节点 role 决定。

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeMeta {
    pub id: NodeId,
    pub role: NodeRole,
    pub provenance: Provenance,
    pub creation_status: NodeCreationStatus,
    pub created_at: DateTime<Utc>,
    pub summary: String,             // 非空
    pub tags: Vec<String>,           // 默认空
    pub model: Option<String>,       // provenance == Llm 时可选
    pub prompt_artifact: Option<String>,
    pub response_artifact: Option<String>,
}

pub struct Node<P> {
    pub meta: NodeMeta,
    pub payload: P,
}
```

约束：

- 所有 payload 结构体必须使用 `#[serde(deny_unknown_fields)]`，对应 strict 模式：多余字段必须拒绝。
- `summary` 非空字符串。
- provenance 一旦写入即不可变；这是来源不可伪造性的最低保障。
- provenance 由创建路径强制：`Human` 必须由 CLI 显式人类输入或 scenario 文件创建；`Llm` 必须经过 LLM 调用 helper；`Deterministic` 必须由确定性 helper 生成。schema 自身不能伪造。
- `creation_status == Failed` 仅在 `RawLlmNode` 上允许；其他角色一律 `Ok`。

### 1.3 LLM 输出的 strict 模式

凡是 LLM 直接产生 JSON 的接口（DecisionNode payload、AssessmentNode 输出包等），必须满足两条：

1. JSON Schema 标记 `additionalProperties: false`；
2. 服务端使用同一个 schema 在收到响应后再校验一次。

不接受任何"宽松解析+事后过滤"的方案。失败一律走 `RawLlmNode` 兜底（4.4.8 节）。

---

## 二、Provenance × Role 矩阵

每类节点对应一个 role；role 与 provenance 之间存在硬约束，工厂函数层强制：

| Role                  | 允许的 Provenance                            |
| --------------------- | -------------------------------------------- |
| `Objective`           | `Human`、`Llm`                               |
| `Constraint`          | `Human`、`Llm`                               |
| `AcceptanceCriterion` | `Human`、`Llm`                               |
| `Decomposition`       | `Llm`                                        |
| `Milestone`           | `Human`、`Llm`                               |
| `ChangeRequest`       | `Human`                                      |
| `Assessment`          | `Llm`                                        |
| `FirstSlice`          | `Llm`                                        |
| `AcceptanceEvent`     | `Human`                                      |
| `WithdrawalEvent`     | `Human`                                      |
| `ActivationEvent`     | `Human`                                      |
| `Clarification`       | `Llm`（kind=question）/ `Human`（kind=answer） |
| `ContextSnapshot`     | `Deterministic`                              |
| `Decision`            | `Llm` 或 `Deterministic`（baseline）         |
| `Pseudocode`          | `Llm`                                        |
| `Code`                | `Llm`                                        |
| `Diagnostic`          | `Deterministic`                              |
| `Selection`           | `Deterministic`                              |
| `Materialize`         | `Deterministic`                              |
| `RawLlm`              | `Llm`（始终带 `creation_status: Failed`）   |

这套约束保证 provenance 不可伪造。其作用：

- 由 LLM 创建的 `Objective` / `Milestone` 的 provenance 必为 `Llm`；
- 它要进入 active context，必须存在指向其版本链的 `AcceptanceEvent`（第五节）。

---

## 三、Append-only 不变量

| 编号  | 不变量                                                                                       |
| ----- | -------------------------------------------------------------------------------------------- |
| **N1** | 节点内容不可变。                                                                            |
| **N2** | 边集合只增不减。                                                                            |
| **N3** | 状态变更通过 successor 节点 + `supersedes` 边表达。                                          |
| **N4** | 人类授权事件由专用节点承载（`AcceptanceEvent` / `WithdrawalEvent` / `ActivationEvent`）。   |
| **N5** | `active` / `bound` / `accepted` 是查询，不是字段。                                          |
| **N6** | provenance 由创建路径强制，schema 自身无法伪造。                                            |

进一步的 schema-level 不变量（CI 守护）：

| 编号   | 不变量                                                                                  |
| ------ | --------------------------------------------------------------------------------------- |
| **I1** | 每个节点都通过 strict schema 校验（meta + payload，多余字段拒绝）。                     |
| **I2** | `(role, provenance)` 在 第二节 矩阵中。                                                    |
| **I3** | 每条边的 `(from.role, to.role, type)` 在 第六节 允许集合中。                              |
| **I4** | `supersedes` 链不成环，且两端 role 相同。                                              |
| **I5** | 每个被指向的 `NodeId` 必须在图中存在；不允许悬空引用。                                  |
| **I6** | 每个 LLM-provenance 节点（`Decision` / `Pseudocode` / `Code` / `Assessment` / `Decomposition`）必须有 `consumed→ ContextSnapshot` 边。 |
| **I7** | `ActivationEvent` 的 to 必须是 bound `Milestone`。                                      |
| **I8** | `creation_status == Failed` 仅出现在 `RawLlmNode` 上。                                  |
| **I9** | 节点和边一旦写入即只读（CI 用 diff 检测守护）。                                         |
| **I10**| Active context 推导仅依赖图当前状态，不依赖任何节点 mutable 字段。                      |

工程层后果：

- `GraphStore` 不暴露任何对 payload 的写权限；`update_node` 只能调整不影响 schema 校验或 active context 推导的元数据，且这种调整本身不应进入正常代码路径。
- `append_edge` 在写入前必须做 `(from.role, to.role, type)` 校验。
- 节点不持有 `*_status` 字段；状态变化通过新增节点 + 事件边表达。`creation_status` 是唯一例外，且只在 `RawLlmNode` 上为 `Failed`。

---

## 四、节点 Payload Schema

按概念簇组织。每个 schema 必须 `deny_unknown_fields`。

### 4.1 目标簇

#### 4.1.1 ObjectiveNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectivePayload {
    pub title: String,        // 非空
    pub description: String,  // 非空
}
```

`Objective` 的 lifecycle 状态全部通过边和事件计算：

- 父子关系：`member_of` 边指向 `Decomposition`；
- 约束：`constrained_by` 边指向 `Constraint`；
- 验收条件：`validated_by` 边指向 `AcceptanceCriterion`；
- 是否被接受 / 撤销：通过 binding 查询计算（第五节）；
- 是否完成：链上是否存在 `decision: Satisfied` 的 `AcceptanceEvent`。

"目标内容修订" = 创建 `Objective` v2 + `supersedes` v1。旧描述完整保留。

#### 4.1.2 ConstraintNode

```rust
pub enum ConstraintKind {
    Invariant,    // 必须保持的旧行为，driving regression gate
    OutOfScope,   // 显式排除的范围
    Preference,   // 软约束
    Forbidden,    // 禁止行为
}

pub enum VerifierKind {
    HiddenCase,
    AuditRule,
    Manual,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verifier {
    pub kind: VerifierKind,
    pub r#ref: String, // 非空
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstraintPayload {
    pub kind: ConstraintKind,
    pub statement: String,        // 非空
    pub verifier: Option<Verifier>, // 可选；默认 None
}
```

每条约束是独立节点，可单独被引用、单独被撤销、单独被一个 verifier 实际执行。

`verifier` 可选：指向 hidden case 或 audit rule 时，确定性管线可驱动 regression gate；为 `Manual` 或 `None` 时仅作 prompt 上下文。

#### 4.1.3 AcceptanceCriterionNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceCriterionPayload {
    pub statement: String,
    pub verifier: Option<Verifier>,
}
```

每条验收条件是独立节点，与 `ConstraintNode` 形状一致但角色不同：约束描述"必须保持的语义"，验收条件描述"必须验证的结果"。

#### 4.1.4 DecompositionNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecompositionPayload {
    pub rationale: String,                  // 非空
    pub proposed_count: u32,                // 冗余字段，便于查询；创建时一次性写入
}
```

一次拆解 = 一个 `Decomposition`。`Decomposition` 是 `decompose` 动作的 LLM 执行产物节点（承载 LLM 生成的 rationale 与拆解结构），与 `Pseudocode` / `Code` / `Assessment` 同属 LLM 输出，故其内容可复现性由 `consumed→ ContextSnapshot` 锚定（I6）——这是产出本次拆解的那一次 LLM 调用的快照，区别于触发它的 `DecisionNode`（"该不该拆"的决策调用）。

主要边：

- `consumed→ ContextSnapshot`：产出本次拆解的 LLM 调用快照（I6）；
- `decomposes← parent_objective`：父目标到该 Decomposition；
- `member_of→ child_objective | child_milestone`：成员关系；
- `supersedes→ previous_decomposition`：重新拆解。

is-accepted：链上存在 `AcceptanceEvent accepts→ Decomposition`。
is-invalidated：通过 supersedes 链，或 `WithdrawalEvent withdraws→ Decomposition`。

#### 4.1.5 MilestoneNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MilestonePayload {
    pub name: String,
    pub summary: String,
}
```

payload 仅承载 `name` / `summary`。阶段范围通过边表达：

- `groups→ Objective`：阶段包含哪些目标；
- `requires→ Constraint(Invariant)`：阶段需保持的不变量；
- `excludes→ Constraint(OutOfScope)`：阶段显式排除范围；
- `validated_by→ AcceptanceCriterion`：阶段验收条件。

active 条件：链上存在 `ActivationEvent activates→` 该 milestone，且没有更新的 `ActivationEvent` 激活其他 milestone（同一目标域同时只能有一个 active milestone）。

### 4.2 变更簇

#### 4.2.1 ChangeRequestNode

```rust
pub enum ChangeRequestKind {
    NewRequirement,
    Correction,
    Preference,
    Rejection,
    ConstraintChange,
}

pub enum ChangePriority {
    Must,
    Should,
    Could,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeRequestPayload {
    pub kind: ChangeRequestKind,
    pub request: String, // 非空
    pub priority: ChangePriority,
}
```

provenance 强制 `Human`。

- 关联目标 / 阶段 / 约束：`targets→` 边；
- 是否被采纳：链上 `AcceptanceEvent` 查询。

#### 4.2.2 AssessmentNode

```rust
pub enum Risk { Low, Medium, High }

pub enum BlastRadius {
    Local,
    Module,
    Subsystem,
    CrossSystem,
    ProductScale,
}

pub enum RecommendedStrategy {
    DirectChange,
    VerticalSlice,
    StagedRollout,
    Spike,
    RejectAsTooLarge,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentPayload {
    pub risk: Risk,
    pub blast_radius: BlastRadius,
    pub recommended_strategy: RecommendedStrategy,
    pub affected_systems: Vec<String>, // 默认空
    pub unknowns: Vec<String>,         // 默认空
    pub notes: String,                 // 默认空字符串
}
```

设计要点：`AssessmentNode` 只承载评估头部信息。`first_slice` / `regression_constraints` / `recommended_action` 等派生信息一律拆为独立节点：

- `assesses→ ChangeRequest | Objective`：被评估对象；
- `affects→ Objective | Milestone | Code`：影响面；
- `proposes→ FirstSlice`：评估给出的下一阶段切片（可选）；
- `proposes→ Constraint(Invariant)`：评估推导出的 regression 约束（每条独立）；
- `proposes→ Decision`：评估推荐的下一步决策。

LLM 输出契约（仅作为 prompt response 形态，**不直接构造图边**；由确定性 helper 拆为以上多节点）：

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentLlmOutput {
    #[serde(flatten)]
    pub head: AssessmentPayload,

    pub proposed_first_slice: Option<FirstSlicePayload>,
    pub proposed_invariants: Vec<ConstraintPayload>, // 默认空
    pub proposed_recommended_action: DecisionAction, // 见 4.4.2 节

    pub self_check: AssessmentSelfCheck,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentSelfCheck {
    pub affects_only_visible_targets: bool,
    pub no_hidden_answers: bool,
    pub no_pseudocode_or_code: bool,
}
```

#### 4.2.3 FirstSliceNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirstSlicePayload {
    pub purpose: String, // 非空
}
```

形状极简，因为 scope / out_of_scope / acceptance 全部走边：

- `groups→ Objective`：第一阶段目标；
- `requires→ Constraint(Invariant)`：第一阶段不变量；
- `excludes→ Constraint(OutOfScope)`：第一阶段显式排除；
- `validated_by→ AcceptanceCriterion`：第一阶段验收条件。

接受一个 `FirstSlice`（`AcceptanceEvent accepts→ FirstSlice`）等于把它升格为下一阶段的 `Milestone` 候选：新建 `Milestone` 节点，由它 `supersedes→ FirstSlice` 接管边集合（语义继承；结构上仍是新节点 + 新边集，旧 FirstSlice 与旧边保留）。

`FirstSlice` 与 `Milestone` 形状相似但分两个角色，是为了在类型层强制"AI 派生 milestone 不能直接 active"——只有经过 `AcceptanceEvent` 升格成 `Milestone` 后才允许 `ActivationEvent` 指向。

### 4.3 事件簇

#### 4.3.1 AcceptanceEventNode

```rust
pub enum AcceptanceDecision {
    Accepted,
    AcceptedWithChanges,
    Satisfied, // 用于阶段完成事件：表示验收条件已被验证通过
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptancePayload {
    pub decision: AcceptanceDecision,
    pub notes: String, // 默认空
}
```

事件本身只有 `decision + notes`。它接受的对象通过 `accepts→` 边表达，可指向多个节点。"接受一个 milestone 等同于接受其下一组目标 / 约束 / 验收条件"在图上是一次事件挂 N 条边，而不是 N 个 AcceptanceEvent。

provenance 强制 `Human`。`Satisfied` 是人类授权的 lifecycle 推进信号，不能由 LLM 触发。

#### 4.3.2 WithdrawalEventNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WithdrawalPayload {
    pub reason: String, // 非空
}
```

通过 `withdraws→` 边指向被撤销的节点（可多个）。撤销不删除任何旧节点，只让 binding 查询返回 false。

撤销 vs supersedes：

- `supersedes`：双方在同一版本链中，新节点接管语义；
- `withdraws`：节点不再有效但没有替代品（失败的拆解、放弃的 spike、被否决的变更）。

#### 4.3.3 ActivationEventNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationPayload {
    pub reason: String, // 默认空
}
```

通过 `activates→` 边激活一个 bound 但未 active 的 `Milestone`。

- 同一目标域同时只能有一个 active milestone：当一个新的 `ActivationEvent` 激活 M2 时，旧的 M1 自动失活（不需要额外节点；推导查"最新 ActivationEvent 指向的 milestone"）；
- 试图 activate 一个 unbound milestone 必须在工厂层拒绝（不变量 I7）。

#### 4.3.4 ClarificationNode

```rust
pub enum ClarificationKind {
    Question,
    Answer,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClarificationPayload {
    pub kind: ClarificationKind,
    pub body: String, // 非空
}
```

provenance 由 kind 决定：`Question` 必为 `Llm`；`Answer` 必为 `Human`。

LLM 在 design / implement / assess 时返回 `needs_clarification` 等同于 emit 一个 `Clarification(Question)`。人类回答 emit 一个 `Clarification(Answer)` + 一条 `answers→ question` 边。

未被回答的 question 会出现在 active context 的 `outstanding_questions` 列表中（第五节）。

### 4.4 推理与执行簇

#### 4.4.1 ContextSnapshotNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSnapshotPayload {
    pub schema_version: u32, // 当前为 1
    pub snapshot: ActiveContext, // 见 第五节
    pub digest: String,      // 64 位 lower-case hex（SHA-256）
}
```

每次 LLM 调用前，确定性 helper 计算 active context（第五节）、序列化、做 SHA-256 digest，写入一个 `ContextSnapshot`。后续创建的 `Decision` / `Pseudocode` / `Code` / `Assessment` / `Decomposition` 通过 `consumed→ ContextSnapshot` 边引用它（不变量 I6）。

价值：

- LLM 输出 100% 复现性；
- anti-cheat 审计：snapshot 内容是否包含不该出现的 hidden case 数据；
- 跨调用比较：发现两次调用看到的 context 不同导致不同输出。

#### 4.4.2 DecisionNode

```rust
pub enum DecisionAction {
    DesignSolution,
    ImplementDesign,
    RepairCode,
    ReviseDesign,
    Decompose,
    Backtrack,
    Select,
    Materialize,
    NeedsClarification,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "kind")]
pub enum StateAssessment {
    Goal {
        goal_size: GoalSize,
        decomposition_pressure: Pressure,
        active_milestone_present: bool,
        outstanding_clarifications: u32,
    },
    Code {
        has_pseudocode: bool,
        has_code: bool,
        compile_status: CompileStatus,
        error_type: ErrorType,
        repair_attempts: u32,
    },
    Change {
        blast_radius: BlastRadius,
        risk: Risk,
        affects_active_milestone: bool,
    },
}

pub enum GoalSize { Tiny, Small, Medium, Large }
pub enum Pressure { Low, Medium, High }
pub enum CompileStatus { NotChecked, Pass, Fail }
pub enum ErrorType { None, Local, Conceptual, Integration }

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionPayload {
    pub selected_action: DecisionAction,
    pub confidence: f32,         // [0.0, 1.0]
    pub rationale: String,       // 非空
    pub state_assessment: StateAssessment,
}
```

`StateAssessment` 是 discriminated union（按 `kind` 标签分别 schema 化），避免把代码层评估字段（has_pseudocode / compile_status）强加给目标层决策。

`considers→` 边表达决策当前焦点节点；`consumed→ ContextSnapshot` 边强制（I6）。

本规范不在 `DecisionPayload` 中外置候选动作集合；prompt 模板的 candidate_actions 只作为 LLM 输入语境，不进入图。如未来需要把候选动作图化，引入新角色而不是修改本 schema。

#### 4.4.3 PseudocodeNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PseudocodePayload {
    pub purpose: String,                  // 非空
    pub artifact_path: String,            // 必须为 "content.pseudo"
}
```

边：

- `addresses→ Objective | Milestone | FirstSlice`：明确为哪个目标域工作；
- `consumed→ ContextSnapshot`；
- `revises→ Pseudocode`：在 diagnostic 反馈下的修订（非 supersedes，因为 revise 可能基于 diagnostic 而不是简单替换）。

#### 4.4.4 CodeNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodePayload {
    pub files: Vec<String>, // 非空，至少一个候选文件路径
}
```

边：

- `addresses→ Objective | Milestone | FirstSlice`；
- `consumed→ ContextSnapshot`；
- `implements→ Pseudocode`：一份代码实现一份伪代码；
- `repairs→ Code`：修复链。

#### 4.4.5 DiagnosticNode

合并所有确定性检查器输出（伪代码 / 代码 / 约束审计 / artifact diff / regression gate）为一类节点，按 `kind` 区分。

```rust
pub enum DiagnosticKind {
    PseudoCheck,
    CodeCheck,
    ConstraintAudit,
    ArtifactDiff,
    RegressionGate,
}

pub enum DiagnosticSeverity { Info, Warning, Error }

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticItem {
    pub code: String,             // 非空
    pub severity: DiagnosticSeverity,
    pub problem: String,          // 非空
    pub location: Option<String>, // 可选
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticPayload {
    pub kind: DiagnosticKind,
    pub ok: bool,
    pub diagnostics: Vec<DiagnosticItem>, // 默认空
}
```

provenance 强制 `Deterministic`。`creation_status: Ok` 表示检查器跑完了（即使 `ok == false` 也是 `creation_status: Ok`，因为节点本身合法）。

边：

- `checks→ Pseudocode | Code | Milestone | Constraint`：被检查对象。`RegressionGate` 类的诊断通过指向 `Constraint` 表达"是哪条 invariant 失败了"。

#### 4.4.6 SelectionNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionPayload {
    pub rationale: String, // 非空
}
```

provenance 强制 `Deterministic`。`selects→ Code` 表达选中候选。

#### 4.4.7 MaterializeNode

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializePayload {
    pub target_root: String, // 非空，例如 "domains"
    pub files: Vec<String>,  // 非空
}
```

provenance 强制 `Deterministic`。`materializes→ Selection` 表达物化事件。

工程层用 Rust 类型状态模式保证 `Materialize` 只能在通过所有 gate 的 `Code` 上被构造（详见 `language_implementation.md` 第十五节）。

#### 4.4.8 RawLlmNode

```rust
pub enum RawLlmFailureKind {
    ExecutionError,
    ParseError,
    ValidationError,
    SelfCheckFailure,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLlmPayload {
    pub failure_kind: RawLlmFailureKind,
    pub operation: String,     // 非空
    pub error_summary: String, // 非空
}
```

强制 `creation_status: Failed`。这是唯一在创建时即 failed 的节点（不变量 I8）。

通过 `attempted→` 边指向意图执行的目标节点，便于审计哪个目标域下出现了多少次失败。

---

## 五、Active Context 推导

Active context 是确定性管线根据图当前状态计算出的视图，喂给 `ContextSnapshotNode` 与下游 LLM 调用。它不存任何字段，每次重新计算（不变量 I10）。

### 5.1 类型

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveContext {
    pub bound_objectives: Vec<ObjectiveView>,
    pub active_milestone: Option<MilestoneView>,
    pub bound_constraints: Vec<ConstraintView>,
    pub bound_acceptance_criteria: Vec<AcceptanceCriterionView>,
    pub open_change_requests: Vec<ChangeRequestView>,
    pub outstanding_questions: Vec<ClarificationView>,
    pub digest: String, // 64 位 lower-case hex，SHA-256
}
```

每个 `*View` 类型只暴露字段子集，**不暴露 `NodeMeta` 全量**，避免给 LLM 提示词注入无关 metadata（例如 prompt_artifact 路径、created_at 时间戳）。

### 5.2 binding 谓词

```text
N is bound at time T iff
  N is the head of its version chain at T, AND
  ( provenance(N) == Human  OR
    ∃ AcceptanceEvent a such that a accepts→ y for some y ∈ chainOf(N) ),
  AND ¬∃ later WithdrawalEvent w such that w withdraws→ y for some y ∈ chainOf(N)
        with timestamp(w) > timestamp(latest_acceptance_of(N)).
```

`provenance == Human` 隐式视为已接受（不需要再为自己签发 AcceptanceEvent）。除此之外没有其他 provenance 享受这一豁免。

`Milestone` 的 active 状态额外要求链上存在 `ActivationEvent`（5.4 节 步骤 5）。

### 5.3 binding 通过边继承

`member_of` 与 `groups` 边继承 binding：

- 若 `Decomposition D` bound，则所有 `member_of D` 的 `Objective` 也 bound；
- 若 `Milestone M` bound，则所有 `M groups→ Objective` 中的目标也 bound；
- 若 `M requires→ Constraint C` 且 M bound，则 C 也 bound。

继承单向且显式：bound 不会从子节点反向传给容器节点。

### 5.4 推导算法

```text
fn derive_active_context(graph: &Graph, t: Time) -> ActiveContext:

    // 步骤 1：链头集合
    heads = { n ∈ graph.nodes | ¬∃ edge supersedes from any m to n }

    // 步骤 2：接受查询
    bound_heads = ∅
    for h in heads:
        if provenance(h) == Human:
            bound_heads.insert(h)
        else:
            chain = chain_of(h)  // 包括 h 自身和所有 h 通过 supersedes 间接覆盖的节点
            if ∃ a in graph.nodes where role(a) == AcceptanceEvent
                                    and a accepts→ y for some y ∈ chain:
                bound_heads.insert(h)

    // 步骤 3：撤销查询
    for h in copy(bound_heads):
        chain = chain_of(h)
        latest_acc_t = max { ts(a) | a accepts→ y, y ∈ chain }
        latest_wd_t  = max { ts(w) | w withdraws→ y, y ∈ chain }
        if latest_wd_t exists and (latest_acc_t is None or latest_wd_t > latest_acc_t):
            bound_heads.remove(h)

    // 步骤 4：继承传播
    for d in bound_heads where role(d) == Decomposition:
        for o where o member_of→ d:
            bound_heads.insert(head_of_chain(o))
    for m in bound_heads where role(m) == Milestone:
        for o where m groups→ o:
            bound_heads.insert(head_of_chain(o))
        for c where m requires→ c:
            bound_heads.insert(head_of_chain(c))

    // 步骤 5：active milestone（同一目标域只能有一个）
    active_ms_candidates = bound_heads ∩ { n | role(n) == Milestone }
    activations = [ act in graph.nodes | role(act) == ActivationEvent
                                       and target(act) ∈ active_ms_candidates ]
    active_milestone =
        if activations is empty: None
        else: target_of(argmax_by_ts(activations))

    // 步骤 6：约束聚合
    bound_constraints = (
        { c | c ∈ bound_heads, role(c) == Constraint, active_milestone? requires/excludes→ c }
        ∪
        { c | c ∈ bound_heads, role(c) == Constraint,
              ∃ o ∈ bound_heads, role(o) == Objective, o constrained_by→ c }
    )

    // 步骤 7：open change requests
    open_change_requests = {
        cr | cr ∈ heads, role(cr) == ChangeRequest,
             ¬∃ AcceptanceEvent a where a accepts→ y, y ∈ chain_of(cr),
             ¬∃ WithdrawalEvent w where w withdraws→ y, y ∈ chain_of(cr)
    }

    // 步骤 8：outstanding questions
    outstanding_questions = {
        q | q ∈ heads, role(q) == Clarification, kind(q) == Question,
             ¬∃ Clarification a where kind(a) == Answer and a answers→ q
    }

    // 步骤 9：digest
    snapshot = serialize_stable({
        bound_objectives, active_milestone, bound_constraints,
        bound_acceptance_criteria, open_change_requests, outstanding_questions
    })
    digest = sha256(snapshot)

    return ActiveContext { ..., digest }
```

注意：

- 序列化必须稳定：所有集合按节点 ID 排序，所有字段按 schema 中固定顺序输出，时间戳使用 RFC 3339 UTC。
- digest 是 SHA-256 的 lower-case hex；任何与该格式不一致的写入必须被拒绝。
- 推导过程不得读写 mutable 字段。

### 5.6 验证用例对 LLM 的不可见性（anti-cheat 投影纪律）

`ConstraintView` / `AcceptanceCriterionView`（5.1 的 `*View`）**只投影 `id` / `kind` / `statement`，绝不投影 `verifier` 字段**。这是结构化的 anti-cheat 防线：

- `ConstraintPayload.verifier`（`Verifier { kind, ref }`）存在于**图节点 payload** 中（4.1.2），可被确定性 gate 读取；
- 但 active context（喂给 LLM 的视图）的 `ConstraintView` 把 `verifier` **整体剔除**——LLM 连"这条约束有没有 hidden case""hidden case 的引用名是什么"都看不到，遑论其内容；
- `verifier.ref` 只是一个**不透明引用名**（如 `"hc:add_one_increments"`），它本身不含期望输入 / 输出；真正的用例正文存于图外的隐藏存储（见下节），engine 推导 active context 时根本不接触该存储。

因此「hidden case 数据出现在 snapshot 里」（10.7 anti-cheat 审计要查的事）在结构上不可能发生：snapshot 来自 `ActiveContext` 序列化，而 `ActiveContext` 的类型里没有任何承载 verifier 正文的字段。

---

## 五A、隐藏验证用例存储（Hidden Verifier Store）

regression gate 的 hidden case「期望输入 / 输出」是**只用于验证、绝不能让被验证的 LLM 看见**的数据（10.8：prompt 不提供 validation-only hidden expected output）。本节规定它存在哪里、如何被 gate 取用、如何与图解耦。

### 5A.1 为什么不进图、不进 active context

- **不进 active context**：active context 是喂 LLM 的视图，放进去即泄漏答案（违反第一原则）。
- **也不进图节点 payload**：图是 append-only 事件溯源存储，其内容会被 dump / 审计 / 在 `graph nodes` 等命令里展示；把期望输出放进任何节点 payload 都让它有机会经由图被读到（即便 `*View` 不投影，原始节点仍可被直接查询）。图里**只保留不透明引用** `verifier.ref`，不保留用例正文。
- **结论**：hidden case 正文存于**图外的独立存储**，由 `ref` 索引，只有确定性 gate（materialize 时）按 `ref` 取用。

### 5A.2 存储形态

隐藏存储是一份 `ref → HiddenCase` 映射，存于项目内**不进 Development Graph、不进 active context** 的位置：

```
sophia-runs/verifiers/hidden.json     # ref → 用例正文；与 dev_graph.sqlite 物理隔离
```

存储条目直接复用 `runtime` 的值模型（**单一值模型**，不另设镜像类型——hidden case 的实参 / 期望最终要喂解释器，复用 `runtime::Value` 避免双向转换与漂移）：

```rust
/// = runtime::HiddenCase（加 serde 派生）。绝不喂 LLM；只供确定性 gate 按 ref 加载取用。
pub struct HiddenCase {
    pub r#ref: String,            // = ConstraintPayload.verifier.ref（不透明键）
    pub entry_action: String,     // 入口 action / transition 名
    pub args: Vec<Value>,         // 实参（按 input 顺序），runtime::Value
    pub expected: ExpectedOutcome, // Returns(Value) | Raises(variant)
}
```

`Value` 是 `runtime::Value`（serde 外部标签判别联合，如 `{"Int":42}`）。`hidden.json` 是 `HiddenCase`
数组，反序列化后按 `ref` 建映射。

约束：

- **键唯一性**：`hidden.json` 内 `ref` 唯一；与图中各 `verifier.ref` 通过该键关联。
- **覆盖性校验（gate 时）**：若某 bound invariant 的 `verifier.kind == HiddenCase` 但 `hidden.json` 缺对应 `ref`，gate **诚实硬错误阻断**——协调层不为该 verifier 注入 `VerifierOutcome`，`audit_constraints` 据此触发 `MissingVerifierOutcome` 硬错误（与"声明了 verifier 却无结果"同等对待，绝不当通过）。
- **来源**：`hidden.json` 由项目维护者 / 出题方手工或工具写入，**不由 LLM 产生**（LLM 产生的是被验证的代码，不是验证它的标准答案）。写入路径与生成代码的路径物理隔离，CI / 审计可单独核查它从不被注入任何 prompt。

### 5A.3 gate 取用流程（materialize 时）

```text
graph select / materialize（确定性 gate，§七 接入点 4 的 constraint_audit gate）：
  1. 加载 hidden.json（缺文件 = 空存储）；推导 active context，取 bound invariants；
  2. 对每条 bound constraint：从 ConstraintNode 原始 payload（非 ConstraintView，后者不投影
     verifier）读 verifier，投影为 audit::Constraint；
  3. 对带 verifier.kind=HiddenCase 的 invariant：
       a. 按 ref 从 hidden.json 取 HiddenCase（缺 → 不注入 outcome → 步骤 4 硬错误阻断）；
       b. 在候选源码上 parse + resolve + analyze 构建 SemanticModel；
       c. runtime::run_hidden_cases（v0 解释器真正执行候选）→ VerificationResult；
       d. 零损耗映射为 audit::VerifierOutcome（passed + detail），注入审计；
  4. audit_constraints(constraints, outcomes) 判定 → DiagnosticNode(kind=ConstraintAudit
     / RegressionGate)，checks→ Code；任一 invariant fail 即阻断（不伪造通过）。
```

分层守恒（与既有「注入报告」模式一致）：

- **执行**属 `runtime`（`run_hidden_cases`）；
- **判定**属 `tools/audit`（`audit_constraints` 消费注入的 `VerifierOutcome`）；
- **加载隐藏存储 + 构候选模型 + 串联执行与判定 + emit 图节点**属**协调层**（CLI `graph_cmd` 的
  `run_constraint_audit` / `run_hidden_verifiers`）——`tools` / `runtime` 都不感知 `hidden.json` 与图。

### 5A.4 与 e2e harness 的关系

e2e harness 里"答案只存于 harness 内部期望、不喂 LLM"（`e2e_test.md` §三 防答案泄漏）正是本机制的**测试态对应物**：harness 的 `Case.expect` 等价于一条 hidden case 正文，harness 充当临时隐藏存储。把它沉淀为 `hidden.json` + 图中 `verifier.ref`，使 regression gate 在**真实工作流**（非仅 e2e）中也能由 hidden case 驱动，且复用同一条防泄漏纪律。

---


## 六、边目录与硬约束

边的种类被刻意限制：每种边只允许特定 `(from.role, to.role)` 组合。`append_edge` 在每次写入前检查；不在表中的组合一律拒绝。

`T*` 列出现的节点表示该边允许多个 to-role；每条具体边仍只有单一 `from` 与单一 `to`。

| 边类型           | from role                                                        | to role                                                              | 语义                       |
| ---------------- | ---------------------------------------------------------------- | -------------------------------------------------------------------- | -------------------------- |
| `supersedes`     | T                                                                | T（同 role）                                                         | 版本链                     |
| `decomposes`     | Objective                                                        | Decomposition                                                        | 该目标提出此次拆解         |
| `member_of`      | Objective \| Milestone                                            | Decomposition \| Milestone \| FirstSlice                              | 成员关系                   |
| `groups`         | Milestone \| FirstSlice                                           | Objective                                                            | 阶段包含哪些目标           |
| `constrained_by` | Objective \| Milestone \| FirstSlice                              | Constraint                                                           | 节点关联约束               |
| `requires`       | Milestone \| FirstSlice                                           | Constraint（kind=Invariant）                                          | 阶段保持的不变量           |
| `excludes`       | Milestone \| FirstSlice                                           | Constraint（kind=OutOfScope）                                         | 阶段排除范围               |
| `validated_by`   | Objective \| Milestone \| FirstSlice                              | AcceptanceCriterion                                                  | 验收条件                   |
| `targets`        | ChangeRequest                                                    | Objective \| Milestone \| Constraint                                  | 变更针对的对象             |
| `assesses`       | Assessment                                                       | ChangeRequest \| Objective                                            | 评估对象                   |
| `affects`        | Assessment                                                       | Objective \| Milestone \| Code                                        | 评估指出的影响面           |
| `proposes`       | Assessment                                                       | FirstSlice \| Constraint \| Decision                                  | 评估派生提案               |
| `accepts`        | AcceptanceEvent                                                  | T*（Objective \| Constraint \| Milestone \| FirstSlice \| Decomposition \| ChangeRequest \| AcceptanceCriterion） | 接受目标列表（多条边）     |
| `withdraws`      | WithdrawalEvent                                                  | T*（同 accepts 允许集合 + Decomposition、Code）                       | 撤销目标列表               |
| `activates`      | ActivationEvent                                                  | Milestone                                                            | 激活某 milestone           |
| `answers`        | Clarification（Answer）                                           | Clarification（Question）                                             | 回答指向问题               |
| `asks_about`     | Clarification（Question）                                         | Objective \| Milestone \| ChangeRequest                               | 问题指向的语境节点         |
| `consumed`       | Decision \| Pseudocode \| Code \| Assessment \| Decomposition     | ContextSnapshot                                                      | 创建时使用的上下文快照     |
| `considers`      | Decision                                                         | Objective \| Code \| ChangeRequest \| Milestone                       | 决策的当前焦点节点         |
| `addresses`      | Pseudocode \| Code                                                | Objective \| Milestone \| FirstSlice                                  | 这份产出在为哪个目标域工作 |
| `revises`        | Pseudocode                                                       | Pseudocode                                                           | 在 diagnostic 反馈下的修订 |
| `implements`     | Code                                                             | Pseudocode                                                           | 一份代码实现一份伪代码     |
| `repairs`        | Code                                                             | Code                                                                 | 修复链                     |
| `checks`         | Diagnostic                                                       | Pseudocode \| Code \| Milestone \| Constraint                         | 被检查对象                 |
| `selects`        | Selection                                                        | Code                                                                 | 选中候选                   |
| `materializes`   | Materialize                                                      | Selection                                                            | 物化事件                   |
| `attempted`      | RawLlm                                                           | T*（任意被尝试构造的目标节点）                                        | 失败调用尝试针对的目标     |

### 6.1 额外校验

- `supersedes`：链不成环，两端 role 相同；一个节点最多一条出向 `supersedes` 边。
- `accepts` / `withdraws` / `activates`：to 节点的 role 必须支持被该事件作用（`ActivationEvent` 只能 activate `Milestone` 等）。
- `proposes`：from 必须是 `Assessment`；to 必须是 `FirstSlice` / `Constraint` / `Decision` 三者之一。
- `answers`：from 必须是 `Clarification(Answer)`；to 必须是 `Clarification(Question)`。
- `consumed`：每个 LLM-provenance 节点必须至少有一条出向 `consumed→ ContextSnapshot` 边（不变量 I6）。

### 6.2 边的不可变性

边没有 `status` 字段。一条边的存在本身就是事实。"撤销一条 accepts 边"通过创建一个 `WithdrawalEvent` 实现，而不是删除边。

### 6.3 多边并存

多条 `supersedes` 边指向同一旧节点是允许的（罕见，但符合 fork 场景）。binding 查询会把"任何到达活跃链头的路径"算作 binding。这是为未来 spike / branch 探索预留空间；实现初期可以拒绝这种情况以简化。

---

## 七、与工作流的接入点

工作流之上的 prompt 模板与 candidate action 表是另一份文档的内容。本规范定义工作流必须使用的接入点：

1. **任何 LLM 调用前必须先创建 `ContextSnapshotNode`**。下游 LLM-provenance 节点必须 `consumed→` 该 snapshot（不变量 I6）。
2. **任何 lifecycle 推进只能由 `AcceptanceEvent` / `WithdrawalEvent` / `ActivationEvent` 承载**；CLI 不允许直接修改节点字段。
3. **任何"拆解错了再来一遍"必须通过 `WithdrawalEvent` 撤销旧 `Decomposition` + 新建一个 `Decomposition` + `supersedes` 边**表达。状态变化不能放在节点字段里。
4. **任何 regression 约束的检查由 `Diagnostic(RegressionGate)` 承载**；它通过 `checks→ Constraint` 表达对哪条约束做了 gate。

### 7.1 实现反哺：产物落盘与图的关系

实现阶段固化以下与本规范一致、但原先未明写的接入点约定：

5. **节点不存正文，产物落盘在图外**。`Pseudocode.artifact_path` 固定 `"content.pseudo"`、`Code.files`
   只记路径（4.4.3 / 4.4.4）。`.pseudo` 文本与候选 `.sophia` 文件正文落盘到 `sophia-runs/graph/
   artifacts/`（未物化），由编排层（CLI）写入与回读。这保持图节点轻量、不可变、可比对；正文是可重建的
   工件而非图状态。
6. **emit 失败兜底节点是编排层职责**。LLM 调用失败时 emit `RawLlmNode`（`attempted→` 目标）+ 调用前
   已建的 `ContextSnapshot`，由 `workflow/engine` 的 `run_llm_step` 统一固化（绝不伪造成功 CodeNode）。
7. **Selection / Materialize 重跑 gate**。物化前的 gate（code_check / constraint_audit / artifact_diff
   / runtime validation）由 `select` 与 `materialize` 各自重跑——编译期类型状态证明不可跨进程持久化
   （详见 `language_design.md` 10.10）。各 gate 结果 emit 对应 `Diagnostic` 并 `checks→ Code`。
8. **评分不入图**。候选排序是确定性管线的内存启发式，本规范无 `Score` role；选择只由
   `SelectionNode { rationale }` 表达（详见 `language_design.md` 10.9）。

---

## 八、范围之外

本规范刻意不处理：

- **跨图（多 workspace）合并**：不在当前图模型范围；
- **节点的删除、垃圾回收、归档**：append-only 不允许节点删除；归档由外部脚本 + 索引快照完成；
- **节点 payload 加密或权限控制**：由 GraphStore 实现层处理，不进入 schema；
- **Schema 演进**：当前假设新需求通过引入新 role 而非升级旧 role 解决。如果某个 role 必须演进 payload，应作为新 role 与旧 role 并存，由 supersedes 链桥接。

---

## 九、实现 checklist

工厂层与 GraphStore 实现必须覆盖：

- [ ] 每个 payload 结构体使用 `#[serde(deny_unknown_fields)]`；
- [ ] LLM 输出 schema 在 prompt 与服务端各校验一次；
- [ ] `(role, provenance)` 校验（第二节 矩阵）；
- [ ] `(from.role, to.role, type)` 校验（第六节 表）；
- [ ] `supersedes` 反环 + 同-role 校验；
- [ ] 悬空引用检测；
- [ ] LLM-provenance 节点的 `consumed→ ContextSnapshot` 必填校验；
- [ ] `ActivationEvent` 的 to 节点 bound 校验；
- [ ] `creation_status: Failed` 仅 `RawLlmNode` 校验；
- [ ] 节点 / 边只读测试（CI diff 检测）；
- [ ] Active context 推导稳定排序与 SHA-256 digest 测试。
