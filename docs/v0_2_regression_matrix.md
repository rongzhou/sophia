# Sophia v0.2 回归矩阵

本文档把 v0.2 已承诺语言边界映射到当前测试和 benchmark。目标是让后续巩固工作优先补“能力证明”缺口，而不是扩张语法。

## 结论

v0.2 语言实现已经覆盖当前承诺的主体边界：top-level 节点、action body 子集、类型检查、TypeScript lowering、运行时输入输出校验、LLM graph 决策工作流和 benchmark runner 都有自动化测试。`match` / `Optional` / `state` 已进入 L3 benchmark；intent、storage、capability 和 error 边界已补集中 checker regression fixture。

## 已覆盖边界

| 能力                                                                                                                       | 当前覆盖                                                              | 巩固建议                                                                       |
| -------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `domain` / `entity` / `state` / `storage` / `error` / `capability` / `action` 节点                                         | parser、checker、context/indexer、workspace path tests                | 保持为 v0.2 节点集；不要把 future 节点混入当前边界                             |
| action body: `let` / `let mutable` / `set` / `if` / `repeat` / `return` / `raise` / `print`                                | body AST、checker、return analysis、TS codegen tests                  | 用 benchmark 覆盖更多组合，而不是继续扩语法                                    |
| `match Bool` / `match State` / `match Optional<T>`                                                                         | AST、checker、return analysis、empty-list inference、TS codegen tests | 已新增 `optional_label_default` 与 `state_status_label` benchmark              |
| 类型系统：`Unit` / `Bool` / `Int` / `Text` / `List<Int>` / `List<Text>` / `Optional<T>` / entity / state / intent wrappers | type parser、expression inference、runtime validation、checker tests  | intent wrapper 在 TS runtime 仍是擦除表示；用 checker fixture 固定 policy 语义 |
| action call、effect、capability allow/deny、error propagation、recursion rejection                                         | checker tests、v0.2 regression fixture                                | benchmark verifier 目前主要看运行结果和 effects，不适合假装完整验证 policy     |
| 诊断统一 shape                                                                                                             | diagnostics.ts、parser、analysis、backend、tests                      | 后续新增诊断必须继续用 `location`，不能回退到 ad-hoc `path`                    |
| LLM graph node decision                                                                                                    | graph decision/apply/report tests、workflow docs                      | 保持 LLM 负责节点选择；脚手架只降低负荷                                        |

## 已补巩固

- `tests/lang/v0_2_regression.test.ts` 正向证明：Raw/Secret 只能通过显式 `intent_conversion: true` action 转为 Sanitized/Redacted；Sanitized/Redacted 可以进入 Console boundary；`DB.Write("Todos")` 的 action output 必须匹配 storage value type；调用会 raise 的 action 时 caller 必须声明 propagated error。
- `tests/lang/v0_2_regression.test.ts` 反向证明：Raw/Secret 不能直接输出到 Console；Raw output 不能声明写入 Sanitized storage；capability `deny` 会覆盖 `allow`；未声明 `errors` 的 `raise` 和未传播 called error 都被拒绝。
- `benchmarks/L3/optional_label_default` 覆盖 `Optional<Text>` 的 `match Some/None`。
- `benchmarks/L3/state_status_label` 覆盖 declared `state` 的 exhaustive `match`。

## 仍需外部运行

- 生成稳定 benchmark report：新增 L3 match benchmark 后，用同一模型跑一次 `full` 与 `direct-ts`，作为 v0.2 能力基线。

## 非 v0.2 范围

以下仍属于未来设计，不应被当前测试当作必需能力：storage body ops、DB.Read runtime、`handle`/error exhaustiveness、`transition`、`task`、`requires`/`ensures`/`invariant`、`entity.with`、正式 IR，以及 intent wrapper 的 runtime nominal representation。
