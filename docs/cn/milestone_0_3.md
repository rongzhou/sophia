# Sophia v0.3 Milestone：启发式目标图工作流

v0.3 是 workflow milestone，不是 Sophia-Core 语言语法 milestone。它建立在 v0.2 的固定生成、检查、修复、审计和 benchmark runner 之上，目标是让 Development Graph 能处理多阶段目标、错误拆解回溯、重新拆解和中途需求变更。

## 目标

- 将目标演化表达为 graph 节点，而不是聊天上下文。
- 区分 human authoritative 目标和 AI proposed/derived 子目标。
- 支持 `ObjectiveNode`、`MilestoneNode`、`ChangeRequestNode`、`ImpactAnalysisNode` 和 `AcceptanceNode`。
- 支持 decomposition invalidation 和 redecomposition，保留旧分支但排除 active context。
- 构造 active goal context，向 decision prompt 暴露当前目标、阶段、已接受变更、out-of-scope 和 regression constraints。
- 将目标图 workflow scenario 接入现有 experiment runner，作为 v0.3 主评估路径。
- 将 benchmark ladder 从 L1-L3 语言/单目标任务连续扩展到 L4-L5 workflow 任务。

## 非目标

- 不扩展 `.sophia` core 语法。
- 不声称已经能开发真实复杂游戏或大型产品。
- 不把 scenario-level graph verification 等同于完整 executable coding benchmark。
- 不实现完整项目管理系统、长期 backlog、architecture planning 或真实人类协作 UI。

## 已完成范围

- 目标类节点 schema 和 payload validation。
- 目标分解、接受、失效、重新拆解、阶段激活、变更记录、影响分析、变更接受、验收记录。
- `buildGoalContext` active context 裁剪。
- graph decision 候选动作、失败状态和 budget guard。
- `graph objective`、`graph milestone`、`graph change`、`graph acceptance` 和 `graph scenario materialize` CLI。
- graph report 中的 `goal_workflow` 区域。
- 三个主干 benchmark 场景：
  - `benchmarks/L4/multistage_todo/scenario.json`
  - `benchmarks/L4/wrong_decomposition_retry/scenario.json`
  - `benchmarks/L5/mount_change/scenario.json`
- v0.3 评估记录，包含 action path、decomposition versions、invalidated branches、accepted changes、deterministic decision baseline 和 goal graph metrics。

## 验收标准

- 多阶段 Todo scenario 能证明阶段目标和变更约束进入 active context。
- 错误拆解 retry scenario 能证明 invalidated branch 保留但不污染 active context。
- Mount change scenario 能证明跨 state/input/movement/save-data 的影响分析、first slice、out-of-scope 和 regression constraints 被记录。
- `experiment run-suite --mode goal-graph --suite benchmarks` 能生成包含 L4/L5 workflow task 的 v0.3 JSONL 结果和 summary。
- `full` mode 仅作为 v0.2 固定流水线历史参照保留；v0.3 的主评估对象是目标图工作流。
- deterministic decision baseline 只用于解释候选 action space 和可回放路径，不作为与 v0.3 并列的独立 benchmark。

## 论文表述边界

可以声称：

- Sophia v0.3 原型支持可审计的目标图工作流。
- 错误拆解可以通过 invalidation/redecomposition 恢复，旧分支不会进入 active context。
- 人类中途需求变更可以被记录、分析、接受，并转化为 regression constraints 和 scoped first slice。

不能声称：

- v0.3 已经证明无人类参与完成真实复杂项目。
- v0.3 已经实现完整自动项目规划。
- v0.3 的 scenario-level verification 等价于大规模 executable benchmark 成功率。
