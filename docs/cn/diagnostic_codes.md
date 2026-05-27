# Sophia v0 诊断码参考

诊断码格式为 `<AREA>-<TOPIC>-<NNN>`。

- `<AREA>` 标识发出诊断的模块，不表示严重程度。
- `<TOPIC>` 是简短规则族标签，例如 `FILE`、`BODY`、`SYNTAX`。
- `<NNN>` 是三位十进制序号，在每个 `(AREA, TOPIC)` 内唯一。

所有诊断统一使用 `src/lang/diagnostics.ts` 的 record shape：`code`、`severity`、`problem`，以及可选 `location` 和 `repair`。诊断位置字段统一命名为 `location`；不要在诊断 record 中使用 ad-hoc `path` 字段。严重程度（`error` / `warning` / `info`）存在 diagnostic record 中，不写进诊断码。`repair` 字段是 LLM repair loop 和确定性工具消费的标准修复提示。

## 区域

| 区域          | 来源模块                                                                         | 含义                                                                |
| ------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `PARSE-*`     | `src/lang/parser.ts`                                                             | 花括号平衡、顶层结构、named block 识别。                            |
| `CHECK-*`     | `src/lang/checker*.ts`、`src/lang/body_*.ts`                                     | 对 `.sophia` 文件执行静态检查。                                     |
| `PSEUDO-*`    | `src/pseudo/check.ts`                                                            | `.pseudo` 结构和语义检查。                                          |
| `INDEX-*`     | `src/analysis/indexer.ts`                                                        | 对 materialized domain tree 构建 ASG index。                        |
| `CONTEXT-*`   | `src/analysis/context.ts`                                                        | Action-rooted semantic context closure。                            |
| `AUDIT-*`     | `src/analysis/constraint_audit.ts`                                               | `.pseudo` 与 `.sophia` 约束审计，例如 forbidden / hardcode / loop。 |
| `DIFF-*`      | `src/analysis/artifact_diff.ts`                                                  | Repair 前后 artifact diff gate。                                    |
| `BUILD-*`     | `src/backend/ts_codegen.ts`、`ts_typecheck.ts`、`strip_assist_equivalence.ts`    | Sophia 到 TypeScript 的 build 和 build 后检查。                     |
| `RUN-*`       | `src/backend/ts_runner.ts`、`ts_runtime_validation.ts`、`ts_generated_module.ts` | 生成模块的 runtime input/output validation。                        |
| `DIRECT-TS-*` | `src/experiment/direct_ts_runner.ts`                                             | Direct TypeScript baseline 实验 runner。                            |

Parser 和 checker 当前覆盖的 v0 顶层声明种类：`domain`、`entity`、`capability`、`action`、`storage`、`state`、`error`。

## PARSE-\*

| 诊断码             | 来源             | 严重程度 | 问题                              |
| ------------------ | ---------------- | -------- | --------------------------------- |
| `PARSE-SYNTAX-001` | `lang/parser.ts` | error    | 源文件花括号不平衡。              |
| `PARSE-FILE-001`   | `lang/parser.ts` | error    | 文件中没有顶层声明。              |
| `PARSE-FILE-002`   | `lang/parser.ts` | error    | 文件中有多个顶层 node。           |
| `PARSE-FILE-003`   | `lang/parser.ts` | error    | 不支持的顶层 kind。               |
| `PARSE-BLOCK-001`  | `lang/parser.ts` | error    | node 内存在不支持的 named block。 |
| `PARSE-BLOCK-002`  | `lang/parser.ts` | error    | node 内存在重复 named block。     |

## CHECK-\*

`CHECK-*` 由 `src/lang/checker*.ts` 和 `src/lang/body_*.ts` 发出，按主题分组。

- `CHECK-FILE-{001..006}`：文件布局、文件路径与声明名/kind 匹配。
- `CHECK-NAME-{001..002}`：命名和跨文件唯一性。
- `CHECK-BLOCK-001`：body block 结构错误。
- `CHECK-SYNTAX-{004,006..016}`：禁止语法，例如 `for`、`var`、`call`、typed local、bare `return`、dangling action call、禁止的 conversion helper。
- `CHECK-BODY-{001..005}`：body 级语义，例如自然语言文本、直接 `Console.Write`、不支持的 `append`、发明 statement、`list == []`。
- `CHECK-VAR-{001..003}`：变量声明、shadowing、mutability。
- `CHECK-RETURN-{001..002}`：return 类型匹配；非 Unit action 的所有路径必须通过 return 或 raise 终止。
- `CHECK-MATCH-{001..006}`：body `match` 表达式类型、case 类型、重复 case、显式穷尽性和 `Some` binding shadowing；Sophia 不提供 `_` catch-all。
- `CHECK-OUTPUT-001`：output 声明结构。
- `CHECK-EFFECT-{001..004}`：effect 声明和 capability/effect 对齐。
- `CHECK-CAPABILITY-{001..004}`：capability 声明、唯一性和 `deny` block 生效。
- `CHECK-ENTITY-{001..005}`：entity 声明、字段、类型。
- `CHECK-STATE-{001..003}`：state 声明、value block、重复 value 名。
- `CHECK-ERROR-{001..009}`：error 声明、variant、action `errors` 列表和 `raise` 字段结构。
- `CHECK-STORAGE-{001..002}`、`CHECK-STORAGE-READ-001`、`CHECK-STORAGE-WRITE-{001..002}`：storage 声明和访问边界。
- `CHECK-TYPE-{001..003}`：支持的 v0 类型和类型兼容。
- `CHECK-ACTION-{001..005}`：action 声明结构和唯一性。
- `CHECK-ACTION-CALL-{001..008}`：action-call expression 有效性、递归 call graph cycle 和被调用 error propagation。
- `CHECK-INTENT-BOUNDARY-001`、`CHECK-INTENT-CONVERSION-{001..004}`：Semantic Assist / intent type 规则。

精确 problem 字符串在对应 emit site 附近维护，不在本文档重复。LLM repair 高频诊断的修复说明在 `data/prompts/common/repair_diagnostic_guide.md`。

## PSEUDO-\*

| 诊断码               | 严重程度 | 问题                                                                |
| -------------------- | -------- | ------------------------------------------------------------------- |
| `PSEUDO-SECTION-001` | error    | `.pseudo` 缺少必需 section。                                        |
| `PSEUDO-LOOP-001`    | error    | repeat 缺少明确次数或条件。                                         |
| `PSEUDO-OUTPUT-001`  | warning  | 多 output field 与 v0 scaffold 不对齐。                             |
| `PSEUDO-EFFECT-001`  | warning  | algorithm 使用 `print`，但 effects 未描述语义输出意图。             |
| `PSEUDO-LIST-001`    | warning  | 直接测试 list emptiness。                                           |
| `PSEUDO-STATE-001`   | warning  | 使用 increment/decrement shorthand。                                |
| `PSEUDO-TEXT-001`    | warning  | 要求显式转换为 `Text`。                                             |
| `PSEUDO-TEXT-002`    | warning  | 把 console 输出措辞为 `print ... as text` 而不是 `print value`。    |
| `PSEUDO-BOOL-001`    | warning  | 把数字 `0/1` flag 当作 `Bool` condition。                           |
| `PSEUDO-VAGUE-001`   | warning  | 步骤过于模糊，无法确定性 implementation。                           |
| `PSEUDO-HINT-001`    | warning  | `.pseudo` 包含 implementation hints；这些属于 implementation 阶段。 |
| `PSEUDO-FLOW-001`    | warning  | algorithm flow 中存在 unreachable/dead step。                       |
| `PSEUDO-BRANCH-002`  | error    | 独立 input 被嵌套进 `else` chain。                                  |

`PSEUDO-BRANCH-001` 已退役，是早期规则版本，故意不复用。

## INDEX-\*

| 诊断码            | 严重程度 | 问题                                               |
| ----------------- | -------- | -------------------------------------------------- |
| `INDEX-FILE-001`  | error    | 文件路径不在 v0 支持布局内。                       |
| `INDEX-FILE-003`  | error    | 文件路径 kind 与顶层声明 kind 不一致。             |
| `INDEX-PARSE-001` | error    | materialized 文件 parse 失败，问题包含 `PARSE-*`。 |
| `INDEX-NODE-001`  | error    | materialized tree 中存在重复顶层 node 名。         |

## CONTEXT-\*

| 诊断码               | 严重程度 | 问题                                   |
| -------------------- | -------- | -------------------------------------- |
| `CONTEXT-ACTION-001` | error    | 请求的 action root 不存在。            |
| `CONTEXT-ACTION-002` | error    | context input 中存在重复 action 声明。 |
| `CONTEXT-ERROR-001`  | error    | action 声明了未知 error variant。      |
| `CONTEXT-NODE-001`   | error    | 被引用 node 不存在。                   |
| `CONTEXT-NODE-002`   | error    | 被引用 node 存在，但 kind 不匹配。     |
| `CONTEXT-NODE-003`   | error    | context input 中存在重复顶层 node 名。 |

## AUDIT-\*

| 诊断码                | 严重程度 | 问题                                                      |
| --------------------- | -------- | --------------------------------------------------------- |
| `AUDIT-FORBIDDEN-001` | error    | 生成的 `.sophia` 违反 `forbidden` constraint。            |
| `AUDIT-HARDCODE-001`  | error    | 生成的 `.sophia` hardcode 完整 expected list。            |
| `AUDIT-HARDCODE-002`  | error    | 生成的 `.sophia` 返回与 expected 相同的 literal scalar。  |
| `AUDIT-LOOP-001`      | warning  | `.pseudo` 说 `repeat N times`，但 `.sophia` 未保留 loop。 |

## DIFF-\*

| 诊断码                | 严重程度 | 问题                                   |
| --------------------- | -------- | -------------------------------------- |
| `DIFF-FILE-001`       | error    | Repair 删除了 `.sophia` 文件。         |
| `DIFF-ACTION-001`     | error    | Repair 删除了 action 声明。            |
| `DIFF-CAPABILITY-001` | error    | Repair 删除了 capability 声明。        |
| `DIFF-EFFECT-001`     | error    | Repair 删除了已声明 effect reference。 |
| `DIFF-SIZE-001`       | warning  | Repair 产生了较大的文本改动。          |

## BUILD-\*

| 诊断码                   | 严重程度 | 问题                                                       |
| ------------------------ | -------- | ---------------------------------------------------------- |
| `BUILD-TARGET-001`       | error    | `sophia.toml` build target 不是 `typescript`。             |
| `BUILD-CHECK-001`        | error    | build 前 Sophia checker 报错。                             |
| `BUILD-PARSE-001`        | error    | build 阶段 parser 失败。                                   |
| `BUILD-CODEGEN-001`      | error    | code generator emit 时抛错。                               |
| `BUILD-STRIP-ASSIST-001` | error    | 移除 Semantic Assist attribute 改变了 emitted TypeScript。 |
| `BUILD-TYPECHECK-001`    | error    | `tsc` 对生成 TypeScript 报错。                             |

## RUN-\*

| 诊断码              | 严重程度 | 问题                                          |
| ------------------- | -------- | --------------------------------------------- |
| `RUN-BUILD-001`     | error    | build 没有产出 entry file。                   |
| `RUN-TRANSPILE-001` | error    | TypeScript 转译 ESM 失败。                    |
| `RUN-ACTION-001`    | error    | 生成 build 未导出 action 或 action metadata。 |
| `RUN-EXEC-001`      | error    | 生成 action runtime 抛错。                    |
| `RUN-INPUT-001`     | error    | Action input 不是 JSON object。               |
| `RUN-INPUT-002`     | error    | Action input 包含未知字段。                   |
| `RUN-INPUT-003`     | error    | Action input 缺少必需字段。                   |
| `RUN-INPUT-004`     | error    | Action input field 的 v0 runtime type 错误。  |
| `RUN-OUTPUT-001`    | error    | Action result 的 v0 runtime type 错误。       |

## DIRECT-TS-\*

| 诊断码                    | 严重程度 | 问题                                 |
| ------------------------- | -------- | ------------------------------------ |
| `DIRECT-TS-EXPORT-001`    | error    | 候选 module 未导出 `runAction`。     |
| `DIRECT-TS-TYPECHECK-001` | error    | `tsc` 对 candidate TypeScript 报错。 |
| `DIRECT-TS-RUN-001`       | error    | 候选 runtime 抛错。                  |

## 新增诊断码规则

1. 从上表选择匹配的 `<AREA>`。如果没有合适区域，先新增区域行。
2. 对同一规则族复用已有 `<TOPIC>` segment，优先复用，不要随意发明。
3. 在 `(AREA, TOPIC)` 内递增 `NNN` 后缀。退役编号不得复用，应保持保留。
4. 把新诊断码加入本文档；如果它是 LLM repair 高频目标，也加入 `data/prompts/common/repair_diagnostic_guide.md`。
5. 严重程度属于 diagnostic record，不属于诊断码。
