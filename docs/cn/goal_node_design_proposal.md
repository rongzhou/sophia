# Sophia 目标节点设计建议

本文档是 v0.3 启发式目标图工作流的设计说明，用于讨论 Sophia 如何从单目标线性生成，扩展到可分解、可阶段推进、可接收人类变更的 Development Graph。它不是 v0.2 已实现能力，也不是完整项目管理规范；其中最小可验证子集已经作为 v0.3 milestone 落地，整体设计仍保留未来扩展空间。

本文分为两部分：

- **整体设计**：描述真实复杂项目需要的目标、阶段、变更、影响分析、验收和跨系统复杂性处理模型。
- **最小可验证子集**：收敛出近期可以尽快落地实现的节点、动作和验证任务。

## 1. 问题背景

当前 workflow 的最小闭环是：

```text
GoalNode
  -> PseudocodeNode
  -> CodeNode
  -> CheckResultNode / AuditNode
  -> RepairCode / ReviseDesign
  -> SelectionNode
  -> MaterializeNode
```

这个闭环适合小任务和单次生成，但不足以支撑复杂目标。复杂目标通常有额外特征：

- 目标无法一次性完整定义。
- 目标必须拆成阶段目标和子目标。
- 人类会在中途提出变更、反馈或验收意见。
- 看似局部的需求可能牵动多个系统。
- 旧行为和旧约束必须在变更后继续被保护。

如果这些信息只保存在聊天历史中，Sophia 的 append-only graph 就失去关键价值：后续 LLM 无法可靠区分原始目标、AI 派生子目标、阶段范围、人类变更、已接受约束、废弃分支和当前基线。

因此，目标演化必须成为 graph 中的一等事件。

## Part A：整体设计

整体设计回答的问题是：如果 Sophia 面向真实长期项目，目标、变更和阶段应该如何进入 Development Graph。

### 2. 设计原则

核心原则：

- 人类不直接修改历史节点。人类输入必须生成新节点。
- 人类创建的目标节点和 AI 派生的目标节点必须显式区分，不能只靠边关系或 summary 推断来源。
- 人类可以定义目标、约束、优先级、反馈和验收，但不直接参与程序实现、repair 或 gate 判断。
- LLM 可以分析目标、分解目标、制定计划和执行实现，但不能把未授权的新需求偷偷写入代码。
- 目标变更必须先经过影响分析，再进入设计或实现。
- 旧目标是否仍然有效必须显式记录，不能依赖聊天上下文。
- `.sophia` 仍是唯一可执行语义源；目标节点只约束探索和验收，不直接改变 runtime。

更准确的定位是：

```text
Sophia 目标是无人类参与具体编程与修复；
人类可以作为目标、约束、反馈、验收和变更事件的来源。
```

### 3. 来源与权威性

所有目标类节点都必须记录来源和权威性：

```json
{
  "origin": "human | ai",
  "authority": "authoritative | proposed | derived"
}
```

约定：

- `origin: human` 表示节点内容来自人类输入或人类验收。
- `origin: ai` 表示节点内容由 LLM 分析、分解或建议生成。
- `authority: authoritative` 表示可作为当前 workflow 的正式约束。
- `authority: proposed` 表示只是候选，需要人类接受或确定性规则提升后才能约束实现。
- `authority: derived` 表示由已授权目标机械或语义派生出来，不能引入新需求。

注意：对 `ChangeRequestNode` 和 `AcceptanceNode` 这类人类事件节点，`authority: authoritative` 只表示“这个事件确实来自人类输入并应被记录”，不表示变更内容已经被采纳为实现约束。变更是否进入当前目标范围仍由 `status` 和后续接受动作表达。

实现上可以先用同一 `ObjectiveNode` 类型加 `origin/authority` 字段；如果实践中仍然容易混淆，再拆成 `UserObjectiveNode` 和 `DerivedObjectiveNode`。第一阶段不建议过早拆类型，但必须在 schema 和 prompt 中强制来源字段。

### 4. 整体节点模型

整体设计中，目标演化可由以下节点表达。不是所有节点都需要第一阶段实现。

| 节点                  | 作用                                   | 第一阶段是否实现              |
| --------------------- | -------------------------------------- | ----------------------------- |
| `ObjectiveNode`       | 可追踪的目标单元，可来自用户或 AI 分解 | 是                            |
| `MilestoneNode`       | 阶段目标，定义当前工作范围和验收边界   | 是                            |
| `ChangeRequestNode`   | 人类中途提出的新需求、反馈或约束变化   | 是                            |
| `ImpactAnalysisNode`  | LLM 对变更影响面的结构化分析           | 是                            |
| `AcceptanceNode`      | 人类对阶段、候选或变更的接受/拒绝结论  | 是                            |
| `TransitionPlanNode`  | 从旧目标/设计迁移到新目标/设计的计划   | 暂缓                          |
| `ArchitectureNode`    | 复杂目标的系统结构和模块边界           | 暂缓                          |
| `BacklogNode`         | 延后处理的目标、阶段或变更             | 暂缓                          |
| `SpikeNode`           | 不可 materialize 的实验性探索分支      | 暂缓，可先用 tags 表达        |
| `RegressionScopeNode` | 需要保持不变的旧目标和旧行为集合       | 暂缓，可先嵌入 ImpactAnalysis |

第一阶段的重点不是完整项目管理，而是证明目标、阶段、变更、影响分析和验收可以进入图，并能约束后续设计与实现。

### 5. 核心节点语义

#### 5.1 ObjectiveNode

表示一个可追踪的目标单元。它可以来自原始用户目标，也可以来自 AI 分解后的子目标。

建议字段：

```json
{
  "origin": "human | ai",
  "authority": "authoritative | proposed | derived",
  "title": "Playable platformer prototype",
  "description": "Build a small playable 2D platformer prototype.",
  "constraints": ["Use keyboard input", "Keep physics deterministic"],
  "acceptance": ["Player can move and jump", "One level can be completed"],
  "parent_objective": "N0001 or null",
  "status": "open | active | satisfied | superseded | abandoned"
}
```

重要约束：AI 生成的子 `ObjectiveNode` 默认不能是 `authoritative`。`decompose_objective` 只能生成 `origin: ai, authority: proposed | derived` 的子目标。只有经过人类接受，或经过明确的 deterministic promotion rule，子目标才能成为当前工作范围的正式约束。

#### 5.2 MilestoneNode

表示阶段目标，而不是具体实现任务。它把复杂目标变成可验收的阶段。

建议字段：

```json
{
  "origin": "human | ai",
  "authority": "authoritative | proposed | derived",
  "name": "vertical_slice",
  "scope": ["player movement", "tile collision", "one test level"],
  "out_of_scope": ["sound", "menus", "multiple worlds"],
  "acceptance": ["Prototype is playable from start to finish"],
  "status": "planned | active | accepted | rejected | superseded"
}
```

AI 生成的 `MilestoneNode` 默认只是候选阶段，不能自动成为 active milestone。阶段划分会强烈影响实现范围，因此必须经过显式接受。

#### 5.3 ChangeRequestNode

表示人类中途提出的新需求、反馈或约束变化。

建议字段：

```json
{
  "origin": "human",
  "authority": "authoritative",
  "kind": "new_requirement | correction | preference | rejection | constraint_change",
  "request": "Add double jump and make enemies split when stomped.",
  "applies_to": ["current_milestone", "player movement", "enemy behavior"],
  "priority": "must | should | could",
  "status": "proposed | accepted | deferred | rejected"
}
```

`ChangeRequestNode` 本身不进入 implementation prompt，除非它已经被接受，并且影响分析已经确定当前目标应吸收该变更。

#### 5.4 ImpactAnalysisNode

表示 LLM 对目标变更的结构化影响分析。它不能修改目标或代码，只能提出影响范围、风险和推荐动作。

建议字段：

```json
{
  "origin": "ai",
  "authority": "proposed",
  "change_request": "N0102",
  "affected_objectives": ["N0003"],
  "affected_milestones": ["N0005"],
  "affected_artifacts": ["PlayerMovement", "EnemyBehavior"],
  "preserved_constraints": ["Keep physics deterministic"],
  "possibly_invalidated_acceptance": ["Enemy disappears when stomped"],
  "recommended_action": "revise_design | branch_design | decompose_objective | defer_change",
  "risk": "low | medium | high"
}
```

对于跨系统变更，`ImpactAnalysisNode` 应额外记录复杂度和阶段建议：

```json
{
  "blast_radius": "local | module | subsystem | cross_system | product_scale",
  "affected_systems": ["input", "animation", "physics", "save", "level_design"],
  "unknowns": ["mount animation constraints", "collision rules while riding"],
  "recommended_strategy": "direct_change | vertical_slice | staged_rollout | spike | reject_as_too_large",
  "first_slice": {
    "scope": ["mount state", "basic movement", "manual dismount"],
    "out_of_scope": ["combat while riding", "network sync", "advanced animation"],
    "acceptance": ["Player can mount a horse", "Mounted player can move", "Player can dismount"]
  },
  "regression_constraints": [
    "Walking without horse still works",
    "Existing jump behavior unchanged"
  ]
}
```

#### 5.5 AcceptanceNode

表示某个阶段、候选、分解或变更被人类接受、拒绝或要求修改。

建议字段：

```json
{
  "origin": "human",
  "authority": "authoritative",
  "target": "N0020",
  "decision": "accepted | rejected | accepted_with_changes",
  "notes": "Jump is too floaty; keep prototype but adjust jump physics next.",
  "creates_change_request": "N0021 or null"
}
```

### 6. 整体动作模型

整体动作可分为四组。第一阶段只实现其中必要子集。

#### 6.1 目标与阶段动作

- `decompose_objective`：把目标拆成 AI 派生候选子目标。
- `accept_objective_decomposition`：把 AI 建议的子目标提升为可执行范围。
- `invalidate_decomposition`：标记一次目标拆解不再作为当前工作范围。
- `redecompose_objective`：在同一父目标下重新生成一组候选子目标。
- `accept_milestone`：接受候选阶段边界。
- `activate_milestone`：设置当前探索焦点。
- `abandon_branch`：放弃失败或越界分支。

#### 6.2 变更动作

- `record_change_request`：把人类输入记录为变更事件。
- `analyze_change_impact`：生成影响分析。
- `accept_change_request`：明确变更被采纳。
- `defer_change_request`：推迟变更。
- `reject_change_request`：拒绝变更。

#### 6.3 跨系统复杂性动作

- `plan_vertical_slice`：把大变更切成最小可验证阶段。
- `run_spike`：创建不可 materialize 的实验性探索分支。
- `define_regression_scope`：列出必须保持不变的旧行为。

#### 6.4 设计与实现动作

这些动作复用现有 workflow，但输入上下文从裸 `GoalNode` 扩展为 active Objective/Milestone：

- `design_solution`
- `revise_design`
- `branch_design`
- `implement_design`
- `repair_code`
- `check_code`
- `audit_code`
- `select`
- `materialize_code`

### 7. 生命周期

#### 7.1 目标生命周期

```text
ObjectiveNode(origin=human, authority=authoritative)
  -> decompose_objective
  -> ObjectiveNode(child, origin=ai, authority=proposed|derived)*
  -> accept_objective_decomposition
  -> MilestoneNode(origin=human|ai, authority=proposed|derived)
  -> accept_milestone
  -> activate_milestone
  -> PseudocodeNode
  -> CodeNode
  -> Check/Audit/Verify
  -> AcceptanceNode
```

其中 `PseudocodeNode` 和 `CodeNode` 不直接挂在自由文本 goal 下，而是挂在当前 active Objective/Milestone 约束下。这样 implementation prompt 可以明确知道：

- 当前做哪个目标。
- 当前阶段范围是什么。
- 哪些约束必须保留。
- 哪些需求明确不在当前阶段。

#### 7.2 变更生命周期

```text
Human input
  -> ChangeRequestNode
  -> ImpactAnalysisNode
  -> accept_change_request / defer_change_request / reject_change_request
  -> revise_design / branch_design / decompose_objective / plan_vertical_slice
  -> Check/Audit/Regression
  -> AcceptanceNode
```

关键约束：`ChangeRequestNode` 不能直接驱动 implementation。它必须先经过影响分析和接受动作。

#### 7.3 跨系统变更生命周期

以“让游戏中的马可以载人”为例，看似简单的需求可能影响角色状态机、输入、动画、碰撞、相机、AI、存档、网络同步、关卡设计和 UI。

Sophia 不应承诺消除这种复杂性。正确目标是让复杂性像人类团队一样可解：

- 先发现影响面，而不是直接实现。
- 先切出最小可验证 vertical slice，而不是一次性完成全部系统。
- 先声明哪些系统不在本阶段处理。
- 每个阶段都有明确验收条件和回归约束。
- 每次变更都有可回滚的图节点，而不是覆盖历史设计。
- 当影响面过大时，系统应主动拒绝“一步完成”的计划。

复杂度策略：

```text
local_change         -> direct revise/repair/implement
module_change        -> objective decomposition + regression scope
subsystem_change     -> milestone + vertical slice
cross_system_change  -> impact analysis + vertical slice or spike
product_scale_change -> refuse direct implementation; require human milestone confirmation
```

系统的价值不在于把 3-6 个月压缩成一次 LLM 调用，而在于：

- 能发现它为什么大。
- 能把它拆成阶段。
- 能保留旧行为约束。
- 能逐步推进并记录每个决策。
- 能在目标变化时知道哪些地方需要重做。

#### 7.4 错误拆解与重新拆解

目标拆解本身是启发式结果，必然可能错误。复杂项目中，这种错误不应被视为异常路径，而应是 Development Graph 的常规操作。

典型场景：

```text
ObjectiveNode(parent)
  -> Decomposition A
      -> ObjectiveNode(A1)
      -> ObjectiveNode(A2)
      -> MilestoneNode(A)
  -> invalidate_decomposition(A)
  -> Decomposition B
      -> ObjectiveNode(B1)
      -> ObjectiveNode(B2)
      -> ObjectiveNode(B3)
      -> MilestoneNode(B)
```

旧拆解子节点必须保留，但不能继续污染当前工作上下文。需要区分三件事：

- 历史保留：旧子目标和旧 milestone 仍在 graph 中，可审计、可复盘。
- 当前有效性：被 invalidated 的拆解不再进入 active objective context。
- 下游影响：旧拆解下已经产生的 PseudocodeNode、CodeNode、CheckResultNode 仍保留，但默认不再作为当前候选，除非被显式迁移或重新选择。

因此，重新拆解不是删除旧节点，而是创建新的 decomposition version。父 `ObjectiveNode` 可以拥有多组拆解候选，但同一时间只能有一组或一个明确集合处于 active/accepted 状态。

建议补充一个轻量关系模型：

```json
{
  "decomposition_id": "D0002",
  "parent_objective": "N0001",
  "children": ["N0101", "N0102", "N0103"],
  "status": "proposed | accepted | invalidated | superseded",
  "invalidates": "D0001 or null",
  "reason": "Previous split mixed rendering concerns into gameplay logic."
}
```

第一阶段可以不新增 `DecompositionNode`，而是在父 `ObjectiveNode` 到子 `ObjectiveNode` 的边或 artifact 中记录 `decomposition_id`。如果后续需要比较多组拆解，再把它提升为一等节点。

理论上，这个机制足以支持复杂项目中的错误拆解恢复，因为它满足四个要求：

- 不覆盖历史。
- 能明确当前有效拆解。
- 能从同一父目标重新生成候选拆解。
- 能防止旧拆解的下游产物进入当前 prompt。

它解决的是恢复能力和上下文隔离问题，不是自动正确性问题。新的拆解仍然可能错误，必须继续通过后续设计、实现、检查、验收和必要时再次 invalidation 来收敛。这个模型与人类复杂项目的工作方式一致：错误计划不会消失，但会被版本化、废弃、替换，并从当前执行上下文中移除。

但它还不能自动判断“哪个拆解更好”。这仍然需要 graph decision、确定性检查、验收信号、影响分析和后续实现结果共同提供证据。

## Part B：最小可验证子集

最小可验证子集回答的问题是：下一步怎样以最小实现成本验证上述设计方向。

### 8. MVP 目标

MVP 不尝试解决完整复杂项目管理，也不尝试真实游戏 benchmark。它只验证五件事：

- 人类目标和 AI 派生目标可以被显式区分。
- AI 分解出的子目标不会自动成为正式实现约束。
- 错误拆解可以被 invalidated，并在同一父目标下重新拆解。
- 人类变更必须通过 `ChangeRequestNode -> ImpactAnalysisNode -> accept` 进入 workflow。
- active milestone 可以限制 design/implementation 的上下文。
- regression scope 可以保护旧验收条件。

### 9. MVP 节点

第一阶段只实现五类节点：

```text
ObjectiveNode
MilestoneNode
ChangeRequestNode
ImpactAnalysisNode
AcceptanceNode
```

这些节点先只保存 JSON artifact，不影响 checker、codegen 或 materialize。

#### 9.1 MVP schema 约束

所有五类节点必须包含：

```json
{
  "origin": "human | ai",
  "authority": "authoritative | proposed | derived",
  "status": "..."
}
```

MVP 可以先不实现复杂的状态机，但必须保证：

- AI 创建的 `ObjectiveNode` 和 `MilestoneNode` 默认不是 `authoritative`。
- `ChangeRequestNode` 必须是 `origin: human`。
- `ImpactAnalysisNode` 必须是 `origin: ai, authority: proposed`。
- `AcceptanceNode` 必须是 `origin: human`。

### 10. MVP 动作

第一阶段建议只实现十二个动作：

| 动作                             | 输入                                        | 输出                              | 说明                                     |
| -------------------------------- | ------------------------------------------- | --------------------------------- | ---------------------------------------- |
| `create_objective`               | 人类文本                                    | `ObjectiveNode(origin=human)`     | 创建权威目标                             |
| `decompose_objective`            | `ObjectiveNode`                             | 候选子 `ObjectiveNode(origin=ai)` | AI 分解，不能直接实现                    |
| `accept_objective_decomposition` | 父目标和候选子目标                          | 子目标变为 accepted/derived       | 第一版可要求人类显式接受                 |
| `invalidate_decomposition`       | 父目标和 decomposition id                   | 旧拆解标记为 invalidated          | 保留历史但移出当前上下文                 |
| `redecompose_objective`          | `ObjectiveNode`                             | 新一组候选子目标                  | 重新拆解，不删除旧子目标                 |
| `create_milestone`               | 人类文本或 AI 建议                          | `MilestoneNode`                   | 候选阶段                                 |
| `accept_milestone`               | 候选 `MilestoneNode`                        | milestone 变为 accepted/derived   | 防止 AI 阶段建议直接激活                 |
| `activate_milestone`             | accepted milestone                          | 当前 active milestone             | 限制上下文                               |
| `record_change_request`          | 人类文本                                    | `ChangeRequestNode`               | 记录变更事件                             |
| `analyze_change_impact`          | `ChangeRequestNode`                         | `ImpactAnalysisNode`              | AI 分析影响面                            |
| `accept_change_request`          | `ChangeRequestNode` 和 `ImpactAnalysisNode` | 变更进入目标上下文                | 未接受变更不能影响 implementation prompt |
| `record_acceptance`              | 人类验收文本                                | `AcceptanceNode`                  | 记录接受/拒绝                            |

暂缓实现：

- `plan_vertical_slice`
- `run_spike`
- `define_regression_scope` 独立节点
- `TransitionPlanNode`
- `ArchitectureNode`
- 自动 promotion rule

这些可以先作为 `ImpactAnalysisNode` 的字段或人工审查内容存在。

### 11. MVP CLI 草案

```bash
graph objective create
graph objective decompose
graph objective accept-decomposition
graph objective invalidate-decomposition
graph objective redecompose
graph milestone create
graph milestone accept
graph milestone activate
graph change record
graph change analyze
graph change accept
graph acceptance record
```

这些命令先创建节点和边。v0.3 已将目标图 workflow scenario 接入现有 experiment runner，并归入主干 L4/L5 benchmark ladder；它不是 `full` 固定流水线的附属 benchmark。

### 12. MVP 与现有 workflow 的接入点

第一阶段只需要让 graph decision 和 prompt 构造能读取目标上下文：

- 当前 active milestone。
- 当前 objective ancestry。
- 已接受 change requests。
- 被 defer/reject 的 change requests。
- 当前阶段 out-of-scope 列表。
- regression constraints。

然后再让 `design_solution` 和 `implement_design` 从 Objective/Milestone 上下文构造 prompt。

MVP 不要求 `sophia check` 理解这些节点；它们暂时只影响启发式探索层。

### 13. MVP 验证任务

#### 13.1 多阶段 Todo 任务

```text
阶段 1：实现一个 Todo 条目创建和列出流程。
阶段 2：人类变更：新增 priority 字段，但不要改变已存在 title 行为。
阶段 3：人类反馈：priority 必须限制为 Low/Normal/High。
阶段 4：验收：旧 title 行为仍通过，新 priority 行为通过。
```

验证点：

- Objective 分解。
- Milestone 激活。
- ChangeRequest 进入图。
- ImpactAnalysis 判断受影响节点。
- 旧约束保留。
- 变更后 regression 仍通过。

#### 13.2 错误拆解重试任务

```text
阶段 1：创建一个目标：实现 Todo 条目创建、列出和优先级标签。
阶段 2：AI 第一次错误拆解：把 priority 当作显示样式，而不是数据字段。
阶段 3：invalidate_decomposition 标记旧拆解无效。
阶段 4：redecompose_objective 重新拆成 data model、create action、list action、priority validation。
阶段 5：旧拆解子节点仍保留，但 active objective context 只包含新拆解。
```

验证点：

- 旧拆解不被删除。
- 新拆解可以和旧拆解共存。
- active context 不包含 invalidated decomposition。
- 下游 design/implementation 只读取新拆解。

#### 13.3 简化 Mount 任务

这个任务模拟“看似简单但跨系统”的变更，但不需要真实游戏引擎。

```text
阶段 1：实现一个简化 game entity 状态模型：Player、Mount、Position。
阶段 2：人类变更：Player 可以 mount。
阶段 3：ImpactAnalysis 必须识别状态、输入、移动规则和保存状态受影响。
阶段 4：第一版只实现 mount / move / dismount，不实现 combat、animation、network。
阶段 5：旧的 Player 单独移动行为仍通过 regression。
```

验证点：

- blast radius 不等于 local。
- ImpactAnalysis 能列出 out-of-scope。
- 第一阶段目标小于完整需求。
- regression constraints 能保护旧行为。

### 14. 实现顺序建议

建议按以下顺序落地：

1. 扩展 graph node type schema，加入五类 MVP 节点。
2. 为五类节点添加 JSON artifact schema 和创建 helper。
3. 增加 CLI 创建命令，只写节点和边。
4. 增加 graph report 对目标节点的统计。
5. 增加 `buildGoalContext`，为 prompt 提供 active milestone、objective ancestry 和 accepted changes。
6. 让 `design_solution` prompt 可选接收目标上下文。
7. 用 Todo 多阶段任务做第一轮手动 graph run。
8. 做错误拆解重试任务，验证 invalidation 和 redecomposition。
9. 再做简化 Mount 任务，验证跨系统变更字段。

### 15. 当前建议结论

整体设计需要承认真实复杂项目的目标演化、阶段推进、跨系统影响和人类验收。最小实现不应一次性实现完整模型。

建议第一阶段只落地：

```text
ObjectiveNode
MilestoneNode
ChangeRequestNode
ImpactAnalysisNode
AcceptanceNode
```

并且只验证：

- 来源和权威性区分。
- AI 派生目标必须被接受后才能约束实现。
- 错误拆解可以保留历史并重新拆解。
- 人类变更必须经过影响分析。
- active milestone 限制当前工作范围。
- regression constraints 保护旧行为。

这个子集足够小，可以较快接入现有 graph store 和 CLI；也足够表达真实项目演化中的关键事件。后续如果需要，再从 `ImpactAnalysisNode` 中拆出 `TransitionPlanNode` 和 `RegressionScopeNode`，从 `ObjectiveNode` 中拆出更细的 `TaskNode`。
