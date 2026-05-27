# Sophia v0 Diagnostic Code Reference

Diagnostic codes follow `<AREA>-<TOPIC>-<NNN>`.

- `<AREA>` identifies the emitting module, not severity.
- `<TOPIC>` identifies a rule family such as `FILE`, `BODY`, or `SYNTAX`.
- `<NNN>` is a three-digit number unique within each `(AREA, TOPIC)`.

All diagnostics use the common record shape from `src/lang/diagnostics.ts`: `code`, `severity`, `problem`, optional `location`, and optional `repair`. Use `location` consistently; do not add ad-hoc `path` fields. Severity lives in the diagnostic record, not in the code. `repair` is the standard hint consumed by deterministic tools and LLM repair loops.

## Areas

| Area          | Source modules                                                                   | Meaning                                                      |
| ------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `PARSE-*`     | `src/lang/parser.ts`                                                             | Brace balance, top-level structure, named block recognition. |
| `CHECK-*`     | `src/lang/checker*.ts`, `src/lang/body_*.ts`                                     | Static checks over `.sophia` files.                          |
| `PSEUDO-*`    | `src/pseudo/check.ts`                                                            | `.pseudo` structure and semantic clarity checks.             |
| `INDEX-*`     | `src/analysis/indexer.ts`                                                        | ASG index construction for materialized domain trees.        |
| `CONTEXT-*`   | `src/analysis/context.ts`                                                        | Action-rooted semantic context closure.                      |
| `AUDIT-*`     | `src/analysis/constraint_audit.ts`                                               | Constraint audit between `.pseudo` and `.sophia`.            |
| `DIFF-*`      | `src/analysis/artifact_diff.ts`                                                  | Repair artifact diff gate.                                   |
| `BUILD-*`     | `src/backend/ts_codegen.ts`, `ts_typecheck.ts`, `strip_assist_equivalence.ts`    | Sophia to TypeScript build and post-build checks.            |
| `RUN-*`       | `src/backend/ts_runner.ts`, `ts_runtime_validation.ts`, `ts_generated_module.ts` | Generated module runtime validation.                         |
| `DIRECT-TS-*` | `src/experiment/direct_ts_runner.ts`                                             | Direct TypeScript baseline runner.                           |

Current parser/checker top-level declaration kinds: `domain`, `entity`, `capability`, `action`, `storage`, `state`, `error`.

## PARSE-\*

| Code               | Source           | Severity | Problem                                |
| ------------------ | ---------------- | -------- | -------------------------------------- |
| `PARSE-SYNTAX-001` | `lang/parser.ts` | error    | Unbalanced braces.                     |
| `PARSE-FILE-001`   | `lang/parser.ts` | error    | File has no top-level declaration.     |
| `PARSE-FILE-002`   | `lang/parser.ts` | error    | File has multiple top-level nodes.     |
| `PARSE-FILE-003`   | `lang/parser.ts` | error    | Unsupported top-level kind.            |
| `PARSE-BLOCK-001`  | `lang/parser.ts` | error    | Unsupported named block inside a node. |
| `PARSE-BLOCK-002`  | `lang/parser.ts` | error    | Duplicate named block inside a node.   |

## CHECK-\*

`CHECK-*` diagnostics are emitted by `src/lang/checker*.ts` and `src/lang/body_*.ts`.

- `CHECK-FILE-{001..006}`: file layout and file path/name/kind consistency.
- `CHECK-NAME-{001..002}`: naming and cross-file uniqueness.
- `CHECK-BLOCK-001`: malformed body block.
- `CHECK-SYNTAX-{004,006..016}`: forbidden syntax such as `for`, `var`, `call`, typed locals, bare `return`, dangling action calls, and forbidden conversion helpers.
- `CHECK-BODY-{001..005}`: body semantics such as natural-language body text, direct `Console.Write`, unsupported `append`, invented statements, and `list == []`.
- `CHECK-VAR-{001..003}`: declaration, shadowing, mutability.
- `CHECK-RETURN-{001..002}`: return type compatibility and all-path return/raise.
- `CHECK-MATCH-{001..006}`: `match` expression type, case types, duplicate cases, explicit exhaustiveness, and `Some` binding shadowing. Sophia has no `_` catch-all.
- `CHECK-OUTPUT-001`: output declaration structure.
- `CHECK-EFFECT-{001..004}`: effect declarations and capability/effect alignment.
- `CHECK-CAPABILITY-{001..004}`: capability declaration, uniqueness, and `deny`.
- `CHECK-ENTITY-{001..005}`: entity declaration, fields, and field types.
- `CHECK-STATE-{001..003}`: state declaration, value block, duplicate values.
- `CHECK-ERROR-{001..009}`: error declaration, variants, action `errors`, and `raise` field structure.
- `CHECK-STORAGE-{001..002}`, `CHECK-STORAGE-READ-001`, `CHECK-STORAGE-WRITE-{001..002}`: storage declarations and access boundaries.
- `CHECK-TYPE-{001..003}`: supported v0 types and compatibility.
- `CHECK-ACTION-{001..005}`: action declaration structure and uniqueness.
- `CHECK-ACTION-CALL-{001..008}`: action-call validity, recursive call graph cycles, and called-error propagation.
- `CHECK-INTENT-BOUNDARY-001`, `CHECK-INTENT-CONVERSION-{001..004}`: Semantic Assist and intent type rules.

Exact problem strings live near their emit sites. High-frequency repair guidance lives in `data/prompts/common/repair_diagnostic_guide.md`.

## PSEUDO-\*

| Code                 | Severity | Problem                                                           |
| -------------------- | -------- | ----------------------------------------------------------------- |
| `PSEUDO-SECTION-001` | error    | Missing required `.pseudo` section.                               |
| `PSEUDO-LOOP-001`    | error    | Repeat lacks a precise count or condition.                        |
| `PSEUDO-OUTPUT-001`  | warning  | Multiple output fields do not align with the v0 scaffold.         |
| `PSEUDO-EFFECT-001`  | warning  | Algorithm uses `print` but effects do not describe output intent. |
| `PSEUDO-LIST-001`    | warning  | Algorithm directly tests list emptiness.                          |
| `PSEUDO-STATE-001`   | warning  | Algorithm uses increment/decrement shorthand.                     |
| `PSEUDO-TEXT-001`    | warning  | Algorithm asks for explicit conversion to `Text`.                 |
| `PSEUDO-TEXT-002`    | warning  | Console output is phrased as `print ... as text`.                 |
| `PSEUDO-BOOL-001`    | warning  | Numeric `0/1` flag is used as a Bool condition.                   |
| `PSEUDO-VAGUE-001`   | warning  | Step is too vague for deterministic implementation.               |
| `PSEUDO-HINT-001`    | warning  | `.pseudo` contains implementation hints.                          |
| `PSEUDO-FLOW-001`    | warning  | Algorithm flow contains unreachable/dead steps.                   |
| `PSEUDO-BRANCH-002`  | error    | Independent inputs are nested into an `else` chain.               |

`PSEUDO-BRANCH-001` is retired and intentionally not reused.

## INDEX-\*

| Code              | Severity | Problem                                                   |
| ----------------- | -------- | --------------------------------------------------------- |
| `INDEX-FILE-001`  | error    | File path is outside supported v0 layout.                 |
| `INDEX-FILE-003`  | error    | File path kind does not match top-level declaration kind. |
| `INDEX-PARSE-001` | error    | Materialized file parse failed.                           |
| `INDEX-NODE-001`  | error    | Duplicate top-level node in materialized tree.            |

## CONTEXT-\*

| Code                 | Severity | Problem                                        |
| -------------------- | -------- | ---------------------------------------------- |
| `CONTEXT-ACTION-001` | error    | Requested action root does not exist.          |
| `CONTEXT-ACTION-002` | error    | Duplicate action declaration in context input. |
| `CONTEXT-ERROR-001`  | error    | Action declares an unknown error variant.      |
| `CONTEXT-NODE-001`   | error    | Referenced node does not exist.                |
| `CONTEXT-NODE-002`   | error    | Referenced node has the wrong kind.            |
| `CONTEXT-NODE-003`   | error    | Duplicate top-level node in context input.     |

## AUDIT-\*

| Code                  | Severity | Problem                                                                    |
| --------------------- | -------- | -------------------------------------------------------------------------- |
| `AUDIT-FORBIDDEN-001` | error    | Generated `.sophia` violates a forbidden constraint.                       |
| `AUDIT-HARDCODE-001`  | error    | Generated `.sophia` hardcodes a full expected list.                        |
| `AUDIT-HARDCODE-002`  | error    | Generated `.sophia` directly returns an expected scalar literal.           |
| `AUDIT-LOOP-001`      | warning  | `.pseudo` uses `repeat N times`, but `.sophia` does not preserve the loop. |

## DIFF-\*

| Code                  | Severity | Problem                                     |
| --------------------- | -------- | ------------------------------------------- |
| `DIFF-FILE-001`       | error    | Repair deleted a `.sophia` file.            |
| `DIFF-ACTION-001`     | error    | Repair deleted an action declaration.       |
| `DIFF-CAPABILITY-001` | error    | Repair deleted a capability declaration.    |
| `DIFF-EFFECT-001`     | error    | Repair deleted a declared effect reference. |
| `DIFF-SIZE-001`       | warning  | Repair produced a large text change.        |

## BUILD-\*

| Code                     | Severity | Problem                                              |
| ------------------------ | -------- | ---------------------------------------------------- |
| `BUILD-TARGET-001`       | error    | Build target is not `typescript`.                    |
| `BUILD-CHECK-001`        | error    | Sophia checker failed before build.                  |
| `BUILD-PARSE-001`        | error    | Parser failed during build.                          |
| `BUILD-CODEGEN-001`      | error    | Code generator threw during emit.                    |
| `BUILD-STRIP-ASSIST-001` | error    | Removing Semantic Assist changed emitted TypeScript. |
| `BUILD-TYPECHECK-001`    | error    | `tsc` failed on generated TypeScript.                |

## RUN-\*

| Code                | Severity | Problem                                             |
| ------------------- | -------- | --------------------------------------------------- |
| `RUN-BUILD-001`     | error    | Build produced no entry file.                       |
| `RUN-TRANSPILE-001` | error    | TypeScript-to-ESM transpilation failed.             |
| `RUN-ACTION-001`    | error    | Generated build does not export action or metadata. |
| `RUN-EXEC-001`      | error    | Generated action threw at runtime.                  |
| `RUN-INPUT-001`     | error    | Action input is not a JSON object.                  |
| `RUN-INPUT-002`     | error    | Action input contains unknown fields.               |
| `RUN-INPUT-003`     | error    | Action input is missing required fields.            |
| `RUN-INPUT-004`     | error    | Action input field has the wrong v0 runtime type.   |
| `RUN-OUTPUT-001`    | error    | Action result has the wrong v0 runtime type.        |

## DIRECT-TS-\*

| Code                      | Severity | Problem                                       |
| ------------------------- | -------- | --------------------------------------------- |
| `DIRECT-TS-EXPORT-001`    | error    | Candidate module does not export `runAction`. |
| `DIRECT-TS-TYPECHECK-001` | error    | `tsc` failed on candidate TypeScript.         |
| `DIRECT-TS-RUN-001`       | error    | Candidate threw at runtime.                   |

## Adding Codes

1. Choose the matching `<AREA>`. If no area fits, add one.
2. Reuse an existing `<TOPIC>` when appropriate.
3. Increment `NNN` within the `(AREA, TOPIC)` family.
4. Add the new code to this document; if common in repair loops, add it to `repair_diagnostic_guide.md`.
5. Keep severity in the diagnostic record, not the code.
