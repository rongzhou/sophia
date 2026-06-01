# Sophia Unit Test Guide (unit test)

> The first of Sophia’s three test categories. Unit tests are a deterministic, offline-capable regression net that can be run in the `cargo test` gate, verifying the correctness of internal components (parser/checker/interpreter/graph/prompt/codegen, etc.) within each crate.
> This is a test guide: it clarifies what unit tests cover, how to run them, how to organize them with discipline, and the existing cases.

---

## I. Positioning

### 1.1 What to test

Unit tests verify that components of a single crate/module behave correctly in isolation: the shape of CSTs from syntax parsing, the scope of name resolution, diagnostics for the three semantic layers (type/effect/contract), interpreter evaluation, invariants of Development Graph event sourcing, prompt template rendering, equivalence between WASM codegen and the interpreter, etc. They are deterministic—identical input always yields identical output—so they can gate `cargo test`, block CI, and run offline.

### 1.2 What not to test

- Do not test the real LLM end-to-end closed loop—that belongs to e2e (`docs/e2e_test.md`).
- Do not test success-rate/time comparisons with Python—that belongs to benchmark (`docs/benchmark_test.md`).
- Do not initiate real network or real file I/O (except temporary fixtures of the test itself)—non-deterministic external I/O breaks the gate.

### 1.3 Mocking policy (the only category among the three that allows mocking)

Unit tests allow mocking, and mocking is a legitimate means for them. This is the key dividing line among the three categories:

- Unit tests may mock: use mocks to isolate incomplete or uncertain dependencies and validate the component under test by itself. Example: the differential tests of `tools/codegen` use a pure Rust mock host (`Store<HostState>` bridging WASM imports) to isolate real I/O and focus on comparing the “interpreter oracle vs WASM execution”; the `runtime` interpreter tests use `InMemoryHost` (`seed_http`/`seed_file` preseeded buckets) to deterministically execute effects.
- e2e/benchmark must not mock: their purpose is to validate real behavior, and mocks would mask errors (see their respective guides).

> Discipline reminder: mocking is a last resort to “test incomplete code,” not a shortcut to “make tests green.” Real code paths that are mocked must still be covered with real I/O in e2e/benchmark.

---

## II. Running

```bash
cargo test --workspace                 # Whole-workspace unit tests (gate scope)
cargo test -p sophia-semantic          # A single crate
cargo test -p sophia-runtime --test verify   # A single test binary
cargo test --workspace --locked        # CI scope (locked deps)
```

Companion gates (same scope as CI; see `.github/workflows/ci.yml`):

```bash
cargo fmt --all -- --check                          # Formatting
cargo clippy --workspace --all-targets -- -D warnings   # Zero warnings
cargo test --workspace                              # All green
```

Current baseline: 359 passed / 0 failed. Snapshot tests use `insta` (`cargo insta review` to review diffs).

---

## III. Discipline

- Determinism first: Unit tests must not depend on clock/random/network/out-of-process state; when external behavior is required, inject fixed data with mocks (§1.3).
- Snapshot guardianship: Use `insta` snapshots for CSTs, semantic model fingerprints, prompt rendering, etc., to guard against “silent behavior drift.” When changing templates/data structures, run `cargo insta review` to confirm diffs match expectations.
- Honest verdicts: Assertions reflect real behavior; never weaken assertions just to pass; hard execution errors should fail tests rather than being swallowed.
- Answer-leak prevention (shared with e2e/benchmark): `sophia_prompt`’s anti-leak assertions ensure shared prompt assets (syntax baseline + standard library assets) contain no task-domain tokens (see `workflow/prompt/tests/render.rs`).

---

## IV. Case inventory

Organized by crate. Each crate lists test binaries, case counts, and inspection points.

### core (language core: syntax → HIR → semantics)

| crate | test binary | count | focus |
| --- | --- | --- | --- |
| `sophia-syntax` | `src/lib.rs` (unit) | 7 | tree-sitter parsing, CST→AST lowering basics |
| `sophia-syntax` | `tests/lowering.rs` | 17 | AST lowering: items/expr/control-flow/unary-neg/Text—full shapes |
| `sophia-hir` | `tests/resolve.rs` | 19 | name resolution, scopes, special roots (`Http`/`File`), effect reference resolution, no shadowing |
| `sophia-hir` | `tests/closure.rs` | 11 | action-rooted semantic closures, cross-node refs, reads/writes collection |
| `sophia-semantic` | `tests/analyze.rs` | 41 | diagnostics for the three semantic layers (type/effect/contract) + intent boundary (`Raw<Text>` direct use rejected) + model fingerprint snapshots |
| `sophia-exec-ir` | `tests/graph.rs` | 4 | Execution Graph structure (nodes/call edges) + snapshot |

### runtime (interpreter = the only execution oracle)

| test binary | count | focus |
| --- | --- | --- |
| `tests/interpret.rs` | 21 | evaluation: scalars/arithmetic/comparison/`if`/`match`/`let`-`set`/`return`-`raise`/cross-call/effect (`InMemoryHost` mock) |
| `tests/trace.rs` | 4 | Execution Trace projection (nodes/call edges/outcomes) |
| `tests/verify.rs` | 6 | hidden-case executor: return value/raise variant match, mismatch → fail, hard execution error → fail (never fabricate) |

### workflow (prompts/LLM/engine/graph)

| crate | test binary | count | focus |
| --- | --- | --- | --- |
| `sophia-prompt` | `tests/render.rs` | 18 | template rendering snapshots, schema strictness, answer-leak prevention (baseline/library assets contain no task tokens), standard library directory/on-demand injection |
| `sophia-llm` | `src/lib.rs` + `tests/structured.rs` | 7 + 6 | client abstraction, structured outputs (schema validation + retry, mock client) |
| `sophia-engine` | `tests/{implement_loop,loop_steps,scheduler,select_materialize,step,traversal}.rs` | 5+6+9+8+4+7 | design/implement-loop/scheduler decisions/goal-tree traversal + human authorization checkpoints/selection materialization (mock LLM client) |
| `sophia-graph-db` | `tests/{active_context,append_only,assessment,decomposition,factory,store}.rs` | 12+2+7+6+6+17 | event-sourcing append-only invariants, active context derivation, binding predicates, decomposition/evaluation nodes |
| `sophia-materialize` | `tests/{gate,score}.rs` | 9 + 7 | candidate gating (rerun checks) + scoring |
| `sophia-lsp` | `src/lib.rs` + `tests/analysis.rs` | 3 + 9 | LSP diagnostics collection/hover/goto (precise spans) |

### tools (deterministic adjudication/checks/codegen)

| crate | test binary | count | focus |
| --- | --- | --- | --- |
| `sophia-audit` | `tests/audit.rs` | 7 | pure adjudication layer: consumes injected verifier outcomes (does not execute code) |
| `sophia-check` | `src/lib.rs` + `tests/checker.rs` | 2 + 5 | strip-assist equivalence gate, integrated check bridge |
| `sophia-codegen` | `tests/contract.rs` + `tests/diff.rs` | 3 + 21 | WASM value ABI contracts + differential tests (interpreter oracle vs `wasmi` hidden-case equivalence, mock host bridging 5 imports) + artifact strip byte gate |

### cli (coordination layer + deterministic integration)

| test binary | count | focus |
| --- | --- | --- |
| `src/lib.rs` (unit) | 16 | coordination components (project layout/rendering/verifier store/parameter spec) |
| `tests/pipeline.rs` | 22 | deterministic CLI pipeline: init → check → build → run/smoke (no LLM, no real external I/O) |
| `tests/intent_matrix.rs` | 3 | intent accept/reject matrix (deterministic): Sophia statically rejects candidates that directly use `Raw<Text>` from `Http.Get` as `Sanitized<Text>` without conversion (`CHECK-INTENT-001`) + accepts safe candidates via `intent_conversion`; the TS acceptance half is presented as a docs matrix (without introducing a tsc gate) |

> `intent_matrix.rs` is a deterministic code_check matrix (checker rulings over a fixed program, no LLM/network), therefore a unit test. It complements the e2e case of network fetch (G2-03): the reject half (static rejection) is nailed deterministically here; the accept half (real fetch runs through) is verified in e2e with real I/O.

---

## V. Engineering structure

- Place unit tests close by in each crate’s `tests/` (integration tests) or inline in `src/` with `#[cfg(test)]` (private unit tests).
- Snapshots go in each crate’s `tests/snapshots/*.snap` (`insta`).
- Define mock fixtures within the test crate that uses them (e.g., the WASM mock host in `tools/codegen/tests/diff.rs`, `InMemoryHost` in `runtime`); do not extract a shared mock library (YAGNI; avoid over-coupling).
- Everything must enter the `cargo test --workspace` gate; the CI `check` job runs with `--locked`.
