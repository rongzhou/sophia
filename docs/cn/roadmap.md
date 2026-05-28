# Sophia 路线图

本文档是 Sophia 唯一有效的当前路线图。旧规划笔记和论文草稿保留在 `../archive/`，仅作为历史上下文。

## 1. 核心论点

Sophia 是一门面向无人监管自动编程的 LLM-native 编程语言和工作流。它把编程纪律外部化为带类型的语义产物、确定性检查、capability/effect 边界、intent type、action-rooted context 和版本化修复循环。

Sophia 不是为人类手写、审查或 IDE 习惯优化的语言。LLM 和人类有不同的优势与失败模式：LLM 擅长局部语义理解、任务分解和重复显式结构，但不擅长长上下文记忆、隐式约定追踪、ambient authority 和无人审查下的约束保持。因此，为 LLM 构建的语言不应简单复用人类优先的语言设计。Sophia 的长期价值在于探索一种编程 substrate：它的语法、节点模型、诊断、上下文闭包和修复门禁都围绕 LLM 的这些特殊取舍设计。

当前工作流不试图放弃 LLM 能力。LLM 仍负责结构化伪代码设计、可编译候选源码生成和探索图启发式节点选择；scaffold、context、diagnostics 和 gate 只负责降低负荷、限制危险自由度并提供确定性反馈。scaffold 只能承载显式 v0 类型、公开 override、路径、命名和 effect/state 等明确结构，不能通过关键词从自然语言描述中推断类型或业务 contract。

新的 Sophia 语言特性必须证明自己能提供传统语言、linter、测试或 prompt discipline 不具备，或不足够机器可证的能力。只有当一个特性能减少 LLM 记忆/猜测负担、创建明确 ASG edge、支持确定性检查，或改善自动 repair / materialize gate 时，它才应进入 Sophia-Core。

## 2. 当前 v0.2 边界

v0.2 是一个可承诺、可测试的原型边界，不是完整 Sophia 语言。

已实现的语言 core：

- 顶层 ASG 节点：`domain`、`entity`、`state`、`storage`、`error`、`capability`、`action`。
- Body statement：`let`、`let mutable`、`set`、`return`、`raise`、`if/else`、`match`、`repeat N times`、`print`。
- 类型：`Unit`、`Bool`、`Int`、`Text`、`List<Int>`、`List<Text>`、`Optional<T>`、已声明 entity/state 类型和 intent wrapper。
- 表达式：字面量、变量、字段访问、算术、比较、布尔操作、Text/List 拼接、`to_text(Int)`、`Some`、`None`、完整 entity 构造和直接 action-call 表达式。
- 作用域和控制流：block-scoped local，不允许 shadow 可见变量，子 block 可更新外层 mutable 变量，`match` 对 Bool/state/Optional 执行穷尽分支检查，非 Unit action 的所有路径必须以 `return` 或 `raise` 终止。

已实现的检查和 gate：

- 文件布局、顶层命名、重复声明和支持类型检查。
- Intent wrapper assignability、显式 `intent_conversion: true` action contract、Console boundary 和 DB.Write storage value boundary。
- Effect、capability、error、raise、action-call、递归和 block-scope 检查。
- Strip-assist TypeScript artifact 等价门禁。
- 基于生成 metadata 的 runtime input/output validation。
- Action-rooted semantic context，包含 files、source payload、nodes、edges、summary 和 diagnostics。

已实现的 workflow/tooling：

- `.pseudo` 结构检查、outline、repair context 和 LLM-facing scaffold。
- Implementation / repair prompt 已接入 action-rooted semantic context。
- Append-only graph workflow：design、implementation、repair、LLM decision、audit、diff、verify、select、materialize。
- TypeScript backend、`run`、`smoke`、hidden benchmark verifier 和 benchmark suite runner。

## 3. 明确不属于当前范围

以下内容仍是 future language design，不能描述成 v0.2 已实现能力：

- `task` 顶层语法和 `context --task`。
- `transition` 顶层语法。
- error handle / error 穷尽性。
- Body 级 storage operation。
- `requires` / `ensures` 证明。
- `invariants`。
- `Result<T,E>` / `Ok` / `Err`。
- `entity.with`。
- 跨 domain library protocol。
- 跨 domain / library intent compatibility。
- Evolution Boundary enforcement。
- 独立 Sophia IR backend 和 IR hash。

## 4. 近期优先级

P0 优先级是让 v0.2 更可辩护，而不是扩张语法。

1. 生成可重复 benchmark report，不再依赖历史实验笔记。
2. 把 intent-safety checker fixture 扩展成 adversarial benchmark suite：Raw to Sanitized、Secret to Redacted、DB.Write mismatch、Console boundary mismatch。
3. 继续把新增 v0.2 成功标准转成聚焦 regression test。
4. 每次语言变更都同步 syntax guide、language design、diagnostic docs 和 tests。
5. 审计 prompt 输入，确保 implementation / repair 只接收确定性 closure/context payload，不接收 validation-only expected output。

## 5. v0.3 Milestone：启发式目标/节点工作流

v0.3 是 workflow milestone，不是 Sophia-Core 语法扩张。v0.2 证明了两阶段生成、确定性检查、repair loop、audit gate 和 benchmark runner 已经能端到端运行。但 v0.2 workflow 仍主要是固定流水线加 append-only log，尚不足以证明 Sophia 的第二个核心主张：LLM 可以在图结构中进行启发式节点选择、目标分解、回溯、重新拆解和阶段推进。

因此，v0.3 的首要研发突破口不是继续扩张 Sophia-Core 语法，而是实现一个可验证的启发式目标/节点工作流最小子集。设计说明见 `goal_node_design_proposal.md`，milestone 边界见 `milestone_0_3.md`。

目标：让 workflow 能处理比当前 L1-L3 单目标 benchmark 更复杂的目标，同时仍保持可审计、可回放、可裁剪上下文和确定性 gate。v0.3 把 benchmark ladder 连续扩展到 L4/L5：L4 关注多阶段目标和错误拆解恢复，L5 关注跨系统变更、影响分析和 regression constraints。

### 5.1 MVP 节点 checklist

- [x] 增加 `ObjectiveNode`：表示可追踪目标单元，支持 `origin`、`authority`、`status`、constraints 和 acceptance。
- [x] 增加 `MilestoneNode`：表示阶段目标，包含 scope、out_of_scope 和 acceptance。
- [x] 增加 `ChangeRequestNode`：记录人类提出的新需求、反馈或约束变化。
- [x] 增加 `ImpactAnalysisNode`：记录 AI 对变更影响面的结构化分析。
- [x] 增加 `AcceptanceNode`：记录人类对目标、阶段、分解或候选的接受/拒绝。
- [x] 在 schema 中强制 `origin: human | ai` 和 `authority: authoritative | proposed | derived`。
- [x] 确保 AI 派生的 `ObjectiveNode` / `MilestoneNode` 默认不能成为 authoritative。

### 5.2 MVP 动作 checklist

- [x] `create_objective`：从人类目标创建 authoritative `ObjectiveNode`。
- [x] `decompose_objective`：从 `ObjectiveNode` 生成 AI 派生候选子目标。
- [x] `accept_objective_decomposition`：把被接受的 AI 子目标提升为当前可执行范围。
- [x] `invalidate_decomposition`：标记错误拆解不再进入 active context，同时保留历史节点。
- [x] `redecompose_objective`：在同一父目标下重新生成一组候选子目标。
- [x] `create_milestone`：创建候选阶段目标。
- [x] `accept_milestone`：接受阶段边界。
- [x] `activate_milestone`：设置当前探索焦点。
- [x] `record_change_request`：把人类中途输入记录为 graph 事件。
- [x] `analyze_change_impact`：生成结构化影响分析。
- [x] `accept_change_request`：明确变更进入目标上下文。
- [x] `record_acceptance`：记录阶段或候选验收结论。

### 5.3 Active context checklist

- [x] 实现 `buildGoalContext`，从 graph 中计算当前 objective ancestry。
- [x] 计算当前 active milestone。
- [x] 只纳入 accepted/derived 的 Objective 和 Milestone。
- [x] 排除 invalidated、abandoned、superseded 分支及其默认下游产物。
- [x] 纳入已接受 ChangeRequest。
- [x] 排除 deferred/rejected ChangeRequest。
- [x] 纳入当前 milestone 的 out_of_scope，防止 LLM 越界实现。
- [x] 纳入 regression constraints，保护旧验收条件。
- [x] 在 prompt 中明确区分 human authoritative 目标和 AI proposed/derived 子目标。

### 5.4 Graph decision checklist

- [x] 让 LLM decision prompt 能看到 goal context，而不是只看当前节点摘要。
- [x] 为目标类节点增加候选动作：decompose、accept、invalidate、redecompose、activate、analyze impact。
- [x] 增加 action budget：decomposition attempts、redecomposition attempts、repair attempts、design revisions。
- [x] 检测重复失败模式，避免 repair/revise/redecompose 死循环。
- [x] 在无合法下一步时生成明确失败状态：`no_valid_action`、`requires_redecomposition`、`requires_human_scope_confirmation`、`budget_exhausted`。
- [x] 保持 baseline decision 与 LLM decision 分离，baseline 只作为对照，不作为“LLM 启发式选择能力”的证据。

### 5.5 CLI / report checklist

- [x] 增加 `graph objective create`。
- [x] 增加 `graph objective decompose`。
- [x] 增加 `graph objective accept-decomposition`。
- [x] 增加 `graph objective invalidate-decomposition`。
- [x] 增加 `graph objective redecompose`。
- [x] 增加 `graph milestone create`。
- [x] 增加 `graph milestone accept`。
- [x] 增加 `graph milestone activate`。
- [x] 增加 `graph change record`。
- [x] 增加 `graph change analyze`。
- [x] 增加 `graph change accept`。
- [x] 增加 `graph acceptance record`。
- [x] 扩展 graph report，统计目标节点、active context、invalidated decomposition、accepted changes 和 abandoned branches。

### 5.6 最小验证任务 checklist

- [x] 多阶段 Todo 任务：先实现 title 行为，再加入 priority 字段，要求旧 title 行为继续通过。
- [x] 错误拆解重试任务：第一次 AI 拆解错误，执行 invalidate/redecompose，确认旧拆解保留但不进入 active context。
- [x] 简化 Mount 任务：用 `Player`、`Mount`、`Position` 模拟跨系统变更，要求 ImpactAnalysis 识别状态、输入、移动规则和保存状态影响。
- [x] 为每个任务记录 graph JSON、prompt、LLM response、active context、最终 check/audit/verify 结果。
- [x] 在进入 benchmark suite 前，先用手动 graph run 验证节点和上下文行为。

### 5.7 Benchmark 接入 checklist

- [x] 将目标图工作流纳入现有 experiment runner，作为 v0.3 主评估路径。
- [x] 将 workflow benchmark 归入主干 L4/L5 层级，而不是旁支 benchmark suite。
- [x] 保留当前 `full` mode 作为 v0.2 固定流水线历史参照，而不是 v0.3 的并列主 benchmark。
- [x] 在 v0.3 评估记录中同时保存 deterministic decision baseline，用于解释 action space，不作为独立成功率 benchmark。
- [x] benchmark 记录 action path、decomposition versions、invalidated branches、accepted changes、repair/redecomposition attempts。
- [x] 报告不只统计 final pass，还统计 graph search 是否避免错误拆解、是否保留旧约束、是否正确排除 invalidated context。

### 5.8 停止条件

该阶段完成的最低标准：

- [x] 一个多阶段任务能通过目标节点、阶段节点和变更节点完成端到端 workflow。
- [x] 一个错误拆解任务能证明旧拆解保留但不污染 active context。
- [x] 一个简化跨系统任务能证明 ImpactAnalysis、out_of_scope 和 regression constraints 进入 prompt。
- [x] `goal-graph` workflow 的运行记录足以作为 v0.3 主评估证据，并可回看 v0.2 固定 `full` workflow 的限制。
- [x] 结果进入主干 L4/L5 benchmark suite，作为 v0.3 工作流能力实验。

## 6. 非 Toy 里程碑

Sophia 不应通过追逐为既有语言生态设计的大型 coding benchmark 来证明自己。更强的主张是定性的：应存在某些代码类别，传统 TypeScript pipeline 会接受，但 Sophia 会因为违背语言内建纪律而确定性拒绝。

### S1：Intent 安全任务

目标：证明 intent type 能拒绝 TypeScript 类型检查、普通 lint 和单元测试可能漏掉的数据流错误。

候选模式：

- 外部 Raw input 写入要求 `Sanitized<T>` 的位置。
- `Secret<T>` 未经 `Redacted<T>` 直接输出到 Console。
- 跨 action 边界跳过 authorization / validation conversion。
- Storage value intent mismatch。

停止条件：一个小型 benchmark suite 中，Sophia 能静态拒绝不安全 candidate，而 direct TypeScript baseline 可以 typecheck 并通过非对抗性测试。

当前状态：checker-level regression fixture 已覆盖 Raw/Secret 显式 conversion、Console boundary、DB.Write storage value mismatch、capability deny、undeclared raise 和 called-error propagation。下一步是把这些静态 fixture 转成可重复实验记录，而不是继续扩展 v0.2 语言面。

### S2：启发式目标/节点工作流

目标：让 development graph 不只是 append-only log，而是能进行可验证的目标分解、阶段推进、错误拆解回溯、重新拆解、变更影响分析和 active context 裁剪。

候选工作：

- 实现 `ObjectiveNode`、`MilestoneNode`、`ChangeRequestNode`、`ImpactAnalysisNode` 和 `AcceptanceNode`。
- 实现 decomposition invalidation 和 redecomposition。
- 实现 active goal context，排除 invalidated/abandoned/superseded 分支。
- 将 goal context 接入 design / implementation / decision prompt。
- 构造多阶段 Todo、错误拆解重试和简化 Mount 任务。

停止条件：一个小型 workflow benchmark 中，LLM goal-graph decision 能在错误拆解、阶段变更或跨系统影响分析场景下产生可审计、可恢复、可验证的路径，并证明这些目标图能力不是 v0.2 固定流水线能够表达的主评估对象。

### S3：Edit Transition 与语义漂移

目标：让 development graph 不只是 append-only log，而是能把编辑表达为语义产物之间的 typed transition。

候选工作：

- 增加显式描述目标语义变化的 edit node。
- 比较编辑前后的 ASG summary 和生成 artifact。
- 检测 entity/action/capability 边界中的职责漂移。

停止条件：repair/edit workflow 能在无人审查下拒绝未经授权的 semantic drift。

### S4：跨 Domain 与 Library 边界

目标：走出单 domain toy project。

候选工作：

- 通过显式 ASG/library manifest 做 cross-domain import。
- 跨 domain 检查 capability/effect boundary。
- 跨 library API 检查 intent compatibility。

停止条件：一个 multi-domain task 中，closure、checks 和 codegen 仍保持确定、有限、可复现。

## 7. 评估原则

- 当前 benchmark 结果只视为 pilot signal，除非用固定 seed、记录 prompt 和可比 baseline 重复运行。
- 优先选择能暴露 Sophia 特有保证的小而尖任务，而不是扩大算法题覆盖面。
- 报告 success、failure type、wall time、LLM calls、repair attempts 和 deterministic gate failures。
- 区分语言价值和 workflow hygiene。Redaction、anti-cheat rules、scaffold validation 和 repair loop 是有用工程实践，但语言本身仍必须提供传统栈缺少的机器可证保证。

## 8. Parking Lot

这些想法可能有价值，但不是当前优先级：

- Dict 和更丰富的 string operation。
- 更宽的 L4/L5 算法 benchmark；v0.3 的 L4/L5 当前专指 workflow benchmark，不恢复旧算法扩列计划。
- 超出最小有效性检查的多模型矩阵。
- 独立 `sophia strip-assist` CLI。
- 超出当前 TypeScript harness 的完整 runtime library packaging。
- 真实外部 DB / network / filesystem effect。

## 9. 已收缩或废弃的旧计划

以下旧方向已经废弃或收窄：

- 把扩展算法题 benchmark 当作主要价值证明。
- 把“小模型也能编程”当作主论点。
- 在 v0.2 完成稳定 benchmark report 之前实现 `task`、`transition`、storage operation 和 Evolution Boundary。
- 把论文草稿或实验日志当成实现事实源。
