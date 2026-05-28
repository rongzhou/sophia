# Graph `action_used` 命名边界

`GraphNode.action_used` 是节点来源动作的审计字段。它不是 `DecisionAction` 的别名。

- `DecisionAction`：图决策系统允许 LLM 或 baseline 选择的下一步动作。
- `action_used`：节点实际如何被创建，包含人工入口、CLI 命令、确定性检查、LLM 调用和 workflow 状态事件。

两者会有交集，例如 `design_solution`、`implement_design`、`repair_code`；但不能假设完全相同。使用 `action_used` 做统计时，必须明确它统计的是节点创建历史，而不是决策候选空间。

## 当前合法值

### 图入口与决策

- `start`：创建原始 `GoalNode`。
- `llm_decide`：创建 LLM `DecisionNode`。

### 伪代码与设计

- `add_pseudo`：从已有 `.pseudo` 文件创建 `PseudocodeNode`。
- `design_solution`：从 goal/objective/milestone 设计伪代码。
- `pseudo_check`：创建 `PseudocodeCheckNode`。
- `revise_design`：根据伪代码诊断修订设计，或记录设计修订预算耗尽。

### 代码、检查、审计和物化

- `implement_design`：从 `PseudocodeNode` 创建候选 `CodeNode`。
- `check_code`：创建 deterministic `CheckResultNode`。
- `constraint_audit`：创建约束审计 `AuditNode`。
- `artifact_diff`：创建修复前后 diff 节点。
- `repair_code`：根据 check/audit 诊断创建修复后的 `CodeNode`。
- `select_code`：创建 `SelectionNode`。
- `materialize_code`：创建 `MaterializeNode`。

### 目标图 workflow

- `create_objective`
- `decompose_objective`
- `accept_objective_decomposition`
- `invalidate_decomposition`
- `redecompose_objective`
- `create_milestone`
- `record_change_request`
- `analyze_change_impact`
- `record_acceptance`

`accept_milestone`、`activate_milestone` 和 `accept_change_request` 当前主要更新已有节点 payload/status，并通过 event artifact 记录，不创建新的 transition node。因此它们可能作为事件名出现，但通常不应作为新节点的 `action_used`。

## 当前收紧规则

`action_used` 已由 `NodeActionSchema` 约束，不再接受任意字符串。测试和 workflow 都不能把 edge type（如 `checks`、`audits`、`diffs`）写成 node action。

新增节点动作必须先进入 `NodeActionSchema`，并说明它和 `DecisionAction` 的关系。历史 graph JSON 中的未知 `action_used` 会解析失败；当前设计不保留旧脏值兼容。

## Graph edge 类型

`GraphEdge.type` 已由 `GraphEdgeTypeSchema` 约束，不再接受任意字符串。当前合法边类型是：

- `designs_solution`
- `decides`
- `implements_design`
- `repairs`
- `revises`
- `checks`
- `audits`
- `diffs`
- `selects`
- `materializes`
- `applies`
- `defines_objective`
- `decomposes_to`
- `decomposes_to_milestone`
- `accepts_decomposition`
- `invalidates_decomposition`
- `defines_milestone`
- `requests_change`
- `analyzes_change`
- `records_acceptance`

新增边类型必须先进入 `GraphEdgeTypeSchema`。历史 graph JSON 中的未知边类型会解析失败。
