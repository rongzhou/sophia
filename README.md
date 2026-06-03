# Sophia

> A **deterministic semantic programming language** for LLM-native / Agent-native systems, designed for unsupervised LLM automatic programming.

Sophia’s core question is: if an LLM is not good at traditional syntax and conventions, but has strong natural-language semantic understanding, can a language, checker, and workflow designed specifically for it let it perform autonomous programming reliably without human review as a fallback?

Sophia’s answer is a division of labor:

- **The LLM is responsible for** semantic understanding, task decomposition, structured expression, and repair suggestions.
- **The language, compiler, and toolchain are responsible for** determinism, boundaries, types, side effects, errors, and capability constraints.

The LLM may generate source code, but the source code’s behavior can only be determined by the formal language and compiler. Sophia is not natural-language programming and not a prompt DSL; it is a compilable language.

> ⚠️ The project is in an early stage (`v0.1.0`). Current status: **v0 interpreter execution** (completed) + **v1 WASM codegen Workflow A has landed**. After the full compilation pipeline, source can be executed by the in-process interpreter or emitted by `sophia build` as a WASM artifact (equivalent to the interpreter per case, guarded by differential tests; full coverage of the starter subset). Remaining v1 work is demand-driven language / standard-library expansion and incremental query architecture. See `docs/en/engineering_architecture.md` §14.2 and `docs/en/wasm_codegen.md` for the roadmap. APIs and language surface may still change.

## Two-Layer System

| Layer | Nature | Responsibility |
| --- | --- | --- |
| **Heuristic exploration layer** | Nondeterministic, forkable, fallible | Let the LLM propose candidates on a controlled Development Graph while preserving versions and failure paths |
| **Deterministic compilation layer** | Deterministic, reproducible, testable | Parse, check, audit, materialize, and run formal `.sophia` source |

Two iron laws:

1. The exploration process may be nondeterministic, but **formal source and compilation results must be deterministic**.
2. **The compiler never calls LLMs** — all LLM calls happen only in the workflow layer, and the language core remains purely deterministic.

## Compilation Pipeline (v0)

```
Source (.sophia)
  → AST            (core/syntax, Tree-sitter)
  → HIR            (core/hir, name resolution / module resolution / Task Closure)
  → Semantic IR    (core/semantic, type / effect / contract layers)
  → Execution Graph IR (core/exec-ir)
  → Interpreter    (runtime, the only v0 execution backend)
```

## Workspace Structure

Strict layering: `core/*` is zero-I/O and does not depend on `workflow/*`.

| Path | Responsibility |
| --- | --- |
| `core/syntax` | Tree-sitter grammar, CST, AST, span |
| `core/hir` | Name resolution, ASG index, Task Closure / Semantic Paging |
| `core/semantic` | Three-layer type / effect / contract semantic analysis |
| `core/exec-ir` | Execution Graph IR |
| `runtime` | Interpreter, EffectHost, input/output validation, Execution Trace |
| `tools/check` | Static checker (syntax + semantics + strip-assist equivalence gate) |
| `tools/audit` | Constraint audit / regression gate |
| `tools/materialize` | Materialize Gate type-state chain + atomic writes |
| `workflow/graph-db` | Development Graph persistence (SQLite + event sourcing) |
| `workflow/llm` | LLM backend abstraction (OpenAI-compatible / Ollama) + structured outputs |
| `workflow/prompt` | Prompt template and JSON Schema management |
| `workflow/engine` | Workflow orchestration (scheduler spine + goal-tree traversal layer) |
| `lsp` | Language Server (hover / diagnostics / goto definition) |
| `cli` | `sophia` command-line entry point (the layer that owns I/O and presentation) |

## Quick Start

For prerequisites and detailed steps, see [INSTALL.md](INSTALL.md).

```bash
# Build and run tests
cargo build --workspace
cargo test --workspace

# Create a project skeleton
cargo run -p sophia-cli -- init my-project

# Static check and interpreter execution
cargo run -p sophia-cli -- check --root my-project
cargo run -p sophia-cli -- run <ActionName> --root my-project --arg int:41
```

### Common CLI Commands

Deterministic commands (no LLM calls): `init` / `parse` / `index` / `check` / `build` / `run` (with `--trace`) / `context` / `smoke` / `repair-context` / `graph` (workflow subcommands) / `lsp`.

LLM commands (backend constructed through `--model` / `--mode`): `graph design` / `graph implement-loop`.

For the full command table, see section IX of `docs/en/engineering_architecture.md`.

### Real LLM Tests and Benchmarks (Optional)

Both real-LLM entry points are `example`s (they do **not** enter the `cargo test` gate and skip cleanly without an API key):

- **e2e** (`cargo run -p sophia-cli --example e2e`): verifies that the Sophia v0 loop works end-to-end. See `docs/en/e2e_test.md`.
- **benchmark** (`cargo run -p sophia-cli --example benchmark`): compares “LLM writes Python directly” vs “Sophia workflow” by **success rate + elapsed time** on small tasks. `baseline` mode requires `python3` (if missing, only `sophia` runs; `python3` is a runtime external tool only and does not enter the Cargo dependency tree). See `docs/en/benchmark_test.md`.

## Documentation

New readers should start with the concept guide:

- **`docs/en/concepts.md` — Concept Guide (read this first)**: uses diagrams to explain the two-layer system, the three “graphs,” the `.pseudo`/`.sophia` two-stage flow, and the relationship between actions/transitions/effects/capabilities.
- `docs/en/language_design.md` — Language and workflow concepts, design decisions (the LLM-facing “big language” layer).
- `docs/en/language_implementation.md` — Compiler / runtime implementation (AST, IR, type inference, checker pipeline).
- `docs/en/engineering_architecture.md` — Toolchain, directory structure, CLI.
- `docs/en/workflow_graph_spec.md` — Development Graph schema and invariants (SSOT).
- `docs/en/dev_checklist_v1.md` — Engineering progress (current SSOT, v1), including v1 requirements / language / standard-library expansion plan. `docs/en/dev_checklist_v0.md` — v0 phase archive (read-only).
- `docs/en/engineering_notes.md` — Engineering decision log.
- Testing guides (three categories): `docs/en/unit_test.md` (unit tests: enter `cargo test`, deterministic, the only place mocks are allowed), `docs/en/e2e_test.md` (end-to-end: real LLM + real I/O, no mocks), `docs/en/benchmark_test.md` (benchmark: success-rate / elapsed-time comparison between Sophia workflow and direct Python generation, no mocks).
- v1 feature design docs: `docs/en/type_system.md` (F1 type-syntax unification with `one of` / `list of`), `docs/en/wasm_codegen.md` (Workflow A: WASM codegen design review).
- Library docs: `docs/en/stdlib_design.md` (library design: manifest-driven plugin model / unified standard + third-party libraries / “I/O = library” boundary / prompt scaffolding), `docs/en/stdlib_implementation.md` (library implementation: `sophia-library` registry + `sophia-stdlib` content + route-B host injection), `docs/en/http_lib.md` (`Http` library), `docs/en/file_lib.md` (`File` library).

For the contribution workflow and code conventions, see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE).

Unless explicitly stated otherwise, contributions intentionally submitted for inclusion in this project are licensed under the MIT License, with no additional terms.
