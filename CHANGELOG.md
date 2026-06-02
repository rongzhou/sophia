# Changelog

This file records important user-facing changes. The format follows [Keep a Changelog](https://keepachangelog.com/),
and versions follow [Semantic Versioning](https://semver.org/).

> For detailed engineering progress and item-by-item change records, see `docs/en/dev_checklist_v1.md` (current) / `docs/en/dev_checklist_v0.md` (v0 archive).

## [Unreleased]

### Changed
- **Type-system unification (F1)**: fallible / nullable returns changed from the planned `Result<T,E>` to `one of {...}` union types (members are constructed and matched directly, with no `Ok`/`Err`/`Some`/`None` wrapper nodes); all type syntax is unified: `<>` is dedicated to Intent Types, and structural types use the `of` keyword family (`list of T` / `one of { M, ... }` / `schema of T`). `Optional<T>` / `List<T>` / `Schema<T>` / `Some` / `None` / `<optional>.exists` are deprecated; the built-in `Null` type and match type patterns are added. See `docs/en/type_system.md`.

### Added
- **Built-in `Http` effect family (F2)**: `Http.Get(url) -> Raw<Text>`, isomorphic to `Console`/`DB` (zero new syntax); untrusted network data is statically controlled through intent boundaries. See `docs/en/http_lib.md`.
- **Real HTTP client host (S1)**: the CLI coordination layer provides a real-network implementation of `Http.Get` based on `reqwest::blocking` (runtime remains zero-I/O); it is injected only when the entry effect contains `Http.Get`. See `docs/en/http_lib.md`.
- **Standard-library prompt scaffolding (S2)**: on-demand library prompt assets (`assets/stdlib/<lib>.md`, with the first one for `http`) — the design stage sees the library catalog and chooses libraries, and the implement stage receives full usage instructions. See `docs/en/stdlib_design.md` / `docs/en/stdlib_implementation.md`.
- Human-authorization checkpoint (`DecompositionReviewer`) in the goal-tree traversal layer `run_goal_tree`: after accepting a `Decomposition`, child goals inherit binding through `member_of`.
- End-to-end test groups G5 (storage persistence) and G6 (goal-tree traversal decompose).
- CI guard tests for append-only / I9 invariants.

### Removed
- Removed the agent-orchestration direction: the top-level `node` construct, built-in `Llm`/`Tool`/`Stream` effect families, five built-in nodes (prompt/router/aggregator/tool/stream), single-node interpretation, and the entire `sophia-stdlib` crate. Rationale: this diverged from the language positioning (the LLM is the programmer, not a built-in program capability) and was overdesign introduced through the stdlib side door. The top-level `effect` construct and the generic `Family.Op(args)` reference form are **kept** (the independent result of removing grammar hard-coding for effects and allowing domain effects to be declared).

## [0.1.0]

First public baseline: **v0 interpreter execution** (source → AST → HIR → Semantic IR → Execution Graph IR → interpreter, no codegen).

### Added
- **Syntax layer**: Sophia-Core Tree-sitter grammar (9 top-level node kinds + body sublanguage), CST / AST, spans.
- **HIR**: name resolution, ASG index, Task Closure / Semantic Paging.
- **Semantic IR**: three-layer type / effect / contract analysis; strip-assist equivalence gate.
- **Execution Graph IR** and **interpreter**: starter subset + storage body operations run end-to-end; runtime input/output validation; Execution Trace projection.
- **Top-level `effect` construct**: `effect Family { operation Op {...} }` declares effect families + generic `Family.Op(args)` references (removes grammar hard-coding for effects; built-in Console/DB, and users can declare domain effects).
- **Development Graph**: SQLite + event-sourcing persistence, node / edge schemas and invariants, Active Context derivation.
- **Workflow engine**: LLM abstraction (OpenAI-compatible / Ollama) + structured outputs, prompt templates, scheduler spine + goal-tree traversal, multi-candidate scoring/ranking.
- **Toolchain**: `tools/check` (static checks), `tools/audit` (constraint audit / regression gate + hidden-case executor), `tools/materialize` (gate type-state chain + atomic writes).
- **Language Server**: hover / diagnostics / goto definition.
- **CLI** `sophia`: `init` / `parse` / `index` / `check` / `build` / `run` (with `--trace`) / `context` / `smoke` / `repair-context` / `graph` workflow subcommands / `lsp`.

[Unreleased]: https://example.invalid/sophia/compare/v0.1.0...HEAD
[0.1.0]: https://example.invalid/sophia/releases/tag/v0.1.0
