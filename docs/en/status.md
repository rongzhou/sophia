# Sophia Status

Current boundary: v0.2 prototype.

This file is a short factual snapshot of the current implementation. Language semantics are documented in `sophia_language_design.md`, workflow rules in `heuristic_workflow.md`, and future work in `roadmap.md`.

## Current Thesis

Sophia explores whether LLM-native programming can externalize part of programming competence into deterministic artifacts. Large-scale code pretraining is valuable, but Sophia tests whether a semantically capable, code-weaker model can still make progress when the language and workflow carry more of the structure.

The externalized artifacts include formal nodes, types, intent wrappers, capability/effect boundaries, structured diagnostics, action-rooted context, and graph-based repair gates. Scaffold, diagnostics, and gates reduce load and constrain outputs; they do not replace the LLM's responsibility for pseudocode design, checkable source generation, or heuristic graph decisions.

## Implemented Language Surface

Top-level nodes:

- `domain`
- `entity`
- `state`
- `storage`
- `error`
- `capability`
- `action`

Types:

- `Unit`, `Bool`, `Int`, `Text`
- `to_text(Int)`
- `List<Int>`, `List<Text>`
- `Optional<T>` with `Some(expr)` and `None`
- declared entity and state types
- Intent wrappers: `Raw`, `Parsed`, `Validated`, `Sanitized`, `Verified`, `Authorized`, `Persisted`, `Secret`, `Redacted`

Body subset:

- `let`, `let mutable`, `set`
- `return`, `raise`
- `if/else`, `match`
- `repeat N times`
- `print`
- complete entity construction
- direct action-call expressions

## Implemented Deterministic Checks

- Supported file layout and one top-level node per file.
- PascalCase top-level naming and path/name consistency.
- Duplicate declaration detection.
- Supported type checks for action input/output, entity fields, storage values, and error fields.
- Block-scoped locals, no visible-variable shadowing, and mutable reassignment rules.
- Return type checking and all-path return/raise checking for non-`Unit` actions.
- Entity construction completeness and field type compatibility.
- Action-call input, effect, error propagation, and recursion checks.
- Minimal error algebra: declare variants and check `raise`.
- Intent assignability, explicit conversion action contracts, Console boundary, and DB.Write storage value boundary.
- Effect/capability allow/deny checks.
- Unsupported syntax diagnostics for common LLM-generated mistakes.

## Implemented Tooling

- `.pseudo` checks, JSON pseudocode outlines, repair context, and LLM-facing scaffold generation.
- Ollama-based design, implementation, repair, and graph-decision commands with JSON validation.
- Action-rooted semantic context in implementation and repair prompts.
- Append-only graph workflow: design, check, repair, audit, diff, verify, select, materialize.
- Deterministic `context --action` output with files, sources, nodes, edges, summary, and diagnostics.
- Deterministic TypeScript backend, generated metadata, runtime input/output validation, `run`, and `smoke`.
- Hidden verifier benchmark tasks and serial suite runner.
- Strip-assist TypeScript artifact equivalence gate.

## Current Validation State

Recent local validation:

- `npm run typecheck` passes.
- `npm test` passes: 35 test files, 295 tests.
- `npx prettier --check "**/*.{md,json,yml,yaml}"` passes.

Current v0.2 regression coverage:

- `tests/lang/v0_2_regression.test.ts`: intent conversion, Console boundary, DB.Write storage value boundary, capability deny, declared raise, and called-error propagation.
- `benchmarks/L3/optional_label_default`: explicit `match Some/None` for `Optional<Text>`.
- `benchmarks/L3/state_status_label`: exhaustive `match` over a declared state.

## Known Limits

- Storage effects are metadata/checking boundaries only; body-level storage operations are not implemented.
- `DB.Read` is checked as an effect/storage reference but has no runtime read API.
- Error handling and error exhaustiveness are not implemented.
- `transition`, `task`, `requires`, `ensures`, `invariants`, `entity.with`, and independent IR are not implemented.
- Intent wrappers are Sophia checker types; generated TypeScript erases runtime intent brands.
- Benchmark scale is still small and should be treated as feasibility signal, not the main project proof.

## Current Priorities

1. Generate stable benchmark reports from reproducible runs.
2. Convert intent-safety checker fixtures into adversarial benchmark tasks.
3. Keep language design, syntax guide, diagnostics, and tests synchronized.
4. Continue auditing prompt inputs so implementation/repair consume deterministic context only, not validation-only expected output.
