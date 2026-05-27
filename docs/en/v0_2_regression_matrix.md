# Sophia v0.2 Regression Matrix

This document maps committed v0.2 language boundaries to current tests and benchmarks. The goal is to fill capability-proof gaps before expanding syntax.

## Conclusion

The v0.2 implementation covers the main committed boundaries: top-level nodes, action body subset, type checking, TypeScript lowering, runtime input/output validation, LLM graph decision workflow, and benchmark runner. `match` / `Optional` / `state` have entered L3 benchmarks; intent, storage, capability, and error boundaries are covered by focused checker regression fixtures.

## Covered Boundaries

| Capability                                                                                                              | Current coverage                                                      | Hardening recommendation                                                                             |
| ----------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `domain` / `entity` / `state` / `storage` / `error` / `capability` / `action` nodes                                     | parser, checker, context/indexer, workspace path tests                | Keep this as the v0.2 node set. Do not mix future nodes into the current boundary.                   |
| action body: `let` / `let mutable` / `set` / `if` / `repeat` / `return` / `raise` / `print`                             | body AST, checker, return analysis, TS codegen tests                  | Cover more combinations with benchmarks rather than expanding syntax.                                |
| `match Bool` / `match State` / `match Optional<T>`                                                                      | AST, checker, return analysis, empty-list inference, TS codegen tests | `optional_label_default` and `state_status_label` benchmarks added.                                  |
| Types: `Unit` / `Bool` / `Int` / `Text` / `List<Int>` / `List<Text>` / `Optional<T>` / entity / state / intent wrappers | type parser, expression inference, runtime validation, checker tests  | Intent wrappers are erased at TS runtime; use checker fixtures to lock policy semantics.             |
| action calls, effects, capability allow/deny, error propagation, recursion rejection                                    | checker tests, v0.2 regression fixture                                | Benchmark verifiers mostly check runtime results/effects; do not pretend they fully validate policy. |
| unified diagnostic shape                                                                                                | diagnostics.ts, parser, analysis, backend, tests                      | New diagnostics must continue using `location`, not ad-hoc `path`.                                   |
| LLM graph node decision                                                                                                 | graph decision/apply/report tests, workflow docs                      | Keep LLM responsible for node selection; scaffold only reduces load.                                 |

## Added Hardening

- `tests/lang/v0_2_regression.test.ts` positive cases: Raw/Secret can become Sanitized/Redacted only through explicit `intent_conversion: true` actions; Sanitized/Redacted can cross the Console boundary; `DB.Write("Todos")` action output must match storage value type; callers must declare propagated errors.
- Negative cases: Raw/Secret cannot be printed directly; Raw output cannot be declared as writing Sanitized storage; capability `deny` overrides `allow`; undeclared `raise` and unpropagated called errors are rejected.
- `benchmarks/L3/optional_label_default` covers explicit `match Some/None` for `Optional<Text>`.
- `benchmarks/L3/state_status_label` covers exhaustive `match` over a declared state.

## Still Needs External Runs

- Generate stable benchmark reports after adding L3 match benchmarks, using the same model for `full` and `direct-ts`.

## Out of v0.2 Scope

These remain future design and must not be treated as current requirements: storage body operations, DB.Read runtime, `handle` / error exhaustiveness, `transition`, `task`, `requires` / `ensures` / `invariant`, `entity.with`, formal IR, and runtime nominal representation for intent wrappers.
