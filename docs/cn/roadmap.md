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

## 5. 非 Toy 里程碑

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

### S2：Edit Transition 与语义漂移

目标：让 development graph 不只是 append-only log，而是能把编辑表达为语义产物之间的 typed transition。

候选工作：

- 增加显式描述目标语义变化的 edit node。
- 比较编辑前后的 ASG summary 和生成 artifact。
- 检测 entity/action/capability 边界中的职责漂移。

停止条件：repair/edit workflow 能在无人审查下拒绝未经授权的 semantic drift。

### S3：跨 Domain 与 Library 边界

目标：走出单 domain toy project。

候选工作：

- 通过显式 ASG/library manifest 做 cross-domain import。
- 跨 domain 检查 capability/effect boundary。
- 跨 library API 检查 intent compatibility。

停止条件：一个 multi-domain task 中，closure、checks 和 codegen 仍保持确定、有限、可复现。

## 6. 评估原则

- 当前 benchmark 结果只视为 pilot signal，除非用固定 seed、记录 prompt 和可比 baseline 重复运行。
- 优先选择能暴露 Sophia 特有保证的小而尖任务，而不是扩大算法题覆盖面。
- 报告 success、failure type、wall time、LLM calls、repair attempts 和 deterministic gate failures。
- 区分语言价值和 workflow hygiene。Redaction、anti-cheat rules、scaffold validation 和 repair loop 是有用工程实践，但语言本身仍必须提供传统栈缺少的机器可证保证。

## 7. Parking Lot

这些想法可能有价值，但不是当前优先级：

- Dict 和更丰富的 string operation。
- 更宽的 L4/L5 算法 benchmark。
- 超出最小有效性检查的多模型矩阵。
- 独立 `sophia strip-assist` CLI。
- 超出当前 TypeScript harness 的完整 runtime library packaging。
- 真实外部 DB / network / filesystem effect。

## 8. 已收缩或废弃的旧计划

以下旧方向已经废弃或收窄：

- 把扩展算法题 benchmark 当作主要价值证明。
- 把“小模型也能编程”当作主论点。
- 在 v0.2 完成稳定 benchmark report 之前实现 `task`、`transition`、storage operation 和 Evolution Boundary。
- 把论文草稿或实验日志当成实现事实源。
