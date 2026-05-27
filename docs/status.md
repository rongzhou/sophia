# Sophia 当前状态

当前边界：v0.2 原型。

本文档是当前实现状态的简短事实快照。语言语义见 `docs/sophia_language_design.md`，工作流规范见 `docs/heuristic_workflow.md`，后续方向见 `docs/roadmap.md`。

## 当前论点

Sophia 探索的是：LLM-native 语言能否通过把编程纪律外部化为确定性产物，让无人监管 LLM 自动编程更可靠。这些产物包括 formal node、类型、intent wrapper、capability/effect 边界、结构化诊断、action-rooted context 和 graph-based repair gate。

Sophia 的设计有意不优化人类优先的编程语言惯例。它假设相比短语法或生态熟悉度，LLM 更需要显式语义节点、稳定文件边界、机器可读 closure 和冗长但明确的 contract。scaffold、diagnostics 和 gate 是 LLM 辅助设施，用来降低负荷并约束输出，不是替代 LLM 的伪代码设计、可编译代码生成或启发式节点选择能力。scaffold 只能固定显式 v0 类型、公开 override、路径、命名和 effect/state 等明确结构；它不再通过关键词从自然语言描述中推断类型或业务 contract。

## 已实现语言面

顶层节点：

- `domain`
- `entity`
- `state`
- `storage`
- `error`
- `capability`
- `action`

类型：

- `Unit`、`Bool`、`Int`、`Text`
- `to_text(Int)` 显式转换为 `Text`
- `List<Int>`、`List<Text>`
- `Optional<T>`，包含 `Some(expr)` 和 `None`
- 已声明 entity 和 state 类型
- Intent wrapper：`Raw`、`Parsed`、`Validated`、`Sanitized`、`Verified`、`Authorized`、`Persisted`、`Secret`、`Redacted`

Body 子集：

- `let`、`let mutable`、`set`
- `return`、`raise`
- `if/else`、`match`
- `repeat N times`
- `print`
- 完整 entity construction
- 直接 action-call expression

## 已实现确定性检查

- 支持的文件布局和每个文件一个顶层 node。
- PascalCase 顶层命名和 path/name 一致性。
- 重复声明检测。
- Action input/output、entity field、storage value 和 error field 的支持类型检查。
- Block-scoped local variable、禁止 shadow 可见变量、mutable reassignment 规则。
- Return type checking，以及非 Unit action 的全路径 return/raise checking。
- Entity construction 字段完整性和类型兼容检查。
- Action-call input、effect、error propagation 和 recursion 检查。
- 最小 error algebra：声明 variant 并检查 `raise`。
- Intent assignability、显式 conversion action contract、Console boundary 和 DB.Write storage value boundary。
- Effect/capability allow/deny 检查。
- 对常见 LLM 生成错误的 unsupported syntax diagnostics。

## 已实现工具链

- `.pseudo` 检查、JSON 结构化伪代码 outline、repair context 和 LLM-facing scaffold generation。
- 基于 Ollama 的 design、implementation、repair 和 graph decision 命令，带 JSON validation。
- Implementation 和 repair prompt 已包含 action-rooted semantic context。
- Append-only graph workflow：design、check、repair、audit、diff、verify、select、materialize。
- 确定性 `context --action` 输出，包含 files、sources、nodes、edges、summary 和 diagnostics。
- 确定性 TypeScript backend、生成 metadata、runtime input/output validation、`run` 和 `smoke`。
- Hidden verifier benchmark tasks 和 serial suite runner。
- Strip-assist TypeScript artifact 等价门禁。

## 当前验证状态

最近一次本地运行结果：

- `npm run typecheck` 通过。
- `npm test` 通过：35 个 test files，295 个 tests。
- `npx prettier --check "**/*.{md,json,yml,yaml}"` 通过。

当前 v0.2 regression 覆盖：

- `tests/lang/v0_2_regression.test.ts`：intent conversion、Console boundary、DB.Write storage value boundary、capability deny、declared raise 和 called-error propagation。
- `benchmarks/L3/optional_label_default`：`Optional<Text>` 的 explicit `match Some/None`。
- `benchmarks/L3/state_status_label`：declared state 的 exhaustive `match`。

## 已知限制

- Storage effect 当前只是 metadata/checking boundary；body-level storage operation 尚未实现。
- `DB.Read` 会作为 effect 和 storage reference 被检查，但没有 runtime read API。
- error handle / error exhaustiveness 尚未实现。
- `transition`、`task`、`requires`、`ensures`、`invariants`、`entity.with` 和独立 IR 尚未实现。
- Intent wrapper 是 Sophia checker type；生成的 TypeScript runtime shape 会擦除 intent brand。
- Benchmark 规模仍小，在稳定报告格式下重复运行前只能视为早期可行性信号。

## 当前优先级

1. 从可重复运行中生成稳定 benchmark reports。
2. 把 intent-safety checker fixture 转成 adversarial benchmark tasks。
3. 保持 language design、syntax guide、diagnostics 和 tests 同步。
4. 继续审计 prompt 输入，确保 implementation / repair 只消费确定性 context，不接收 validation-only expected output。
