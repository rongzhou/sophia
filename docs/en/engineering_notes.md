# Engineering Notes & Decision Log

Purpose: record small engineering decisions that are not fully specified in the design docs but must persist across the codebase, to prevent implementation drift and lower onboarding cost.

## Conventions
- Keep entries concise; prefer updating an entry over adding a near-duplicate.
- Each decision should include: date, scope, decision, rationale, impact, status.
- If it affects public APIs or cross-crate patterns, reference this entry in the PR/commit message.

## Global Principle: Single Path Only, No Multi-Path or Backward-Compat Burden
- At no layer are multiple implementation paths allowed (including but not limited to: feature toggles, dual stacks, temporary fallbacks, legacy-API adapters, etc.).
- Once a design change is confirmed, migrate directly and remove the old path; do not carry backward compatibility cost.
- Temporary placeholders are for keeping builds runnable only; they must live on the single code path and return explicit unimplemented errors, not functional fallbacks.
- This applies to all crates (core, workflow, tools, lsp, cli, runtime, stdlib).
- MRs violating this principle will be rejected; when necessary, add an entry here with tradeoffs and a migration plan.

## Decision Log

- 2026-05-28 — Error handling baseline
  - Decision: Library crates use `thiserror` for typed errors; binary crates (bin, e.g. `cli`) may use `anyhow` for app-layer aggregation where needed.
  - Rationale: Keep clear error classes in core crates; friendlier error presentation at the edges.
  - Impact: Introduce `thiserror` to relevant crates; public lib APIs avoid exposing `anyhow::Error` directly.
  - Status: Accepted

- 2026-05-28 — SQLite backend choice
  - Decision: `workflow/graph-db` uses `rusqlite` with `bundled` feature.
  - Rationale: Zero external system dependency, simple API.
  - Impact: Enable the `bundled` feature; CI/local builds don’t depend on system SQLite.
  - Status: Accepted

- 2026-05-28 — Formatting config
  - Decision: Add `rustfmt.toml` (edition=2021, max_width=100, Unix newlines).
  - Rationale: Unified code style across multi-crate workspace.
  - Impact: Enforce `cargo fmt` locally and in future CI.
  - Status: Accepted

- 2026-05-28 — Git and branch naming
  - Decision: Local git only for now, no remote; default branch renamed to `main`.
  - Rationale: Fresh Rust workspace; avoid coupling with old TS repo; follow common naming.
  - Impact: Workflows assume `main` as default branch.
  - Status: Accepted

- 2026-05-28 — Workspace and layering discipline
  - Decision: Crate layout follows `engineering_architecture.md`; `core/*` forbids I/O and must not depend on `workflow/*`.
  - Rationale: Improve testability, determinism, and preserve WASM possibility.
  - Impact: Enforced during dependency additions and reviews.
  - Status: Accepted

- 2026-05-28 — CLI scope
  - Decision: Use `clap`, keep only minimal subcommand placeholders; only consume stable interfaces (e.g., `syntax::parse_file`) to avoid large follow-up changes.
  - Rationale: Reduce placeholder churn and later refactor cost.
  - Impact: Expand subcommands gradually as subsystems mature.
  - Status: Accepted

- 2026-05-28 — Dependency policy
  - Decision: Bring in deps just-in-time; prefer minimal and well-maintained crates; cross-cutting selections are recorded here.
  - Rationale: Control surface area, avoid early binding before semantics stabilize.
  - Impact: New deps must be made explicit in PRs and recorded here as precedent.
  - Status: Accepted

- 2026-05-28 — Parser baseline (single path)
  - Decision: Force Tree-sitter at syntax layer, remove multi-path/feature flags; `parse_file` goes only via Tree-sitter.
  - Rationale: Align with design and avoid drift/split from multi-path.
  - Impact: `core/syntax` depends on `tree-sitter`; once Sophia grammar crate is available, add `set_language` binding and record here.
  - Status: Accepted

- 2026-05-28 — Unified language for comments and docs
  - Decision: Comments/docs in the repo use Chinese; when an English term is needed, annotate as “Chinese (English term)” on first appearance.
  - Rationale: Reduce ambiguity, keep style consistent, ease team collaboration/review.
  - Impact: Reviews must unify language if mixed; clean up existing mixes over time.
  - Status: Accepted

- 2026-05-28 — Pace and commit strategy (big strides, one-and-done)
  - Decision: Use big-stride, one-shot completion; prohibit tiny repeated commits around the same file; forbid “minimal interface/placeholder” in lieu of pushing real functionality.
  - Rationale: Reduce drift/context-switching and avoid churn and rework.
  - Impact: Reject fragmented changes in review; milestone-level changes must land in one merge and be runnable/validated.
  - Status: Accepted

- 2026-05-28 — Code structure boundary (core/syntax responsibilities)
  - Decision: `core/syntax` only provides parsing and stable APIs; do not mix in ad-hoc diagnostics, I/O, CLI logic; split helpers as modules if necessary and keep `lib.rs` cohesive and simple.
  - Rationale: Clear responsibilities improve maintenance and evolution; avoid “kitchen sink”.
  - Impact: Clean `core/syntax` of ad-hoc diagnostics/unrelated logic; CLI layer handles I/O and presentation.
  - Status: Accepted

- 2026-05-28 — Version and dependency alignment (latest stable and in-sync)
  - Decision: Prefer “latest stable” under compatibility, and ensure alignment across toolchain/generated artifacts/crates (e.g., Tree-sitter CLI, generated parser.c ABI, Rust crate version). Do not guess by memory. Current alignment: tree-sitter crate 0.26 + tree-sitter-cli 0.26.x + ABI 15.
  - Rationale: Avoid build/runtime failures from ABI/API mismatches; reduce future migration cost.
  - Impact: Specify compatibility matrix before introducing/upgrading; record alignment and verification in commits.
  - Status: Accepted

- 2026-05-28 — Temporary and diagnostic code
  - Decision: Do not merge temporary diagnostics/debug prints into main; remove after local verification. No temporary code paths in commits.
  - Rationale: Keep main clean; avoid noise and nondeterminism.
  - Impact: Remove or reject PRs with temporary code.
  - Status: Accepted

- 2026-05-28 — Vendor and external repos
  - Decision: Only vendor necessary headers/generated artifacts; forbid embedding external git repos as subdirs. If needed, use submodules and document; default is no submodules.
  - Rationale: Avoid repo pollution and complex pulls; ensure reproducible builds.
  - Impact: Remove embedded external repo indices; build only uses local vendor/generated dirs.
  - Status: Accepted

- 2026-05-28 — Documentation sync discipline
  - Decision: After each big-stride merge, immediately sync the current progress checklist (now `dev_checklist_v1.md`; v0 phase is archived as `dev_checklist_v0.md`) and these notes; forbid repeating already-completed items in chats/commits.
  - Rationale: Single source of truth (SSOT), reduce coordination cost.
  - Impact: Review checks that commits include doc sync.
  - Status: Accepted

- 2026-05-28 — Output determinism and traversal order
  - Decision: Use lexical order for directory/file traversal; JSON uses stable keys (e.g., `BTreeMap`); paths use forward slashes.
  - Rationale: Make outputs comparable, snapshot-able, and reproducible.
  - Impact: All interfaces/implementations follow this; deviations must be recorded here.
  - Status: Accepted

- 2026-05-29 — Orchestration layer `workflow/engine` and the “inject reports” layering pattern
  - Decision: Add `workflow/engine` for workflow orchestration (`run_llm_step` / loop_steps / implement_loop / scheduler / select_materialize). It is the only crate that depends on both `workflow` and `tools`. Deterministic analyzers in `tools/*` do not depend on the Development Graph; they only produce structured reports (`CheckReport`/`AuditReport`/`GateReport`/`VerifierOutcome`). Graph side-effects (emit `DiagnosticNode`, create `checks→` edges) live in the orchestration layer. Orchestration that needs to run a checker obtains deterministic results via injected callbacks (`CodeChecker`) instead of `use`-ing the checker directly.
  - Rationale: Funnel non-determinism and graph side-effects into `workflow`/`cli` and keep `core`/`tools` deterministic, testable, and graph-agnostic; avoid layering cycles.
  - Impact: New orchestrations follow “inject report/callback” rather than direct coupling; `graph-db`/`llm`/`prompt` remain independent.
  - Status: Accepted

- 2026-05-29 — LLM structured schemas must be faithful contracts
  - Decision: For each workflow LLM step, the JSON Schema must strictly match the server-side deserialization target (`additionalProperties:false` + complete `required`), no slack.
  - Rationale: `decision_node.json` once only required `state_assessment.kind`, looser than Rust enums, causing “schema passes but deserialization fails”. The schema is a strict contract (workflow_graph_spec 1.3), never looser than types.
  - Impact: Compare schemas with target types; discriminated unions use `oneOf` with per-tag required full fields.
  - Status: Accepted

- 2026-05-29 — Centralize shared test utilities
  - Decision: Move integration-test commons (mock client, schema access, node seed, temp dirs) to `tests/common/mod.rs`, guarded by `#![allow(dead_code)]` since each bin uses a subset; test schemas must reuse authoritative sources (`sophia_prompt::schema_for`), no hand-written copies.
  - Rationale: Remove duplication across tests; prevent schema copies from drifting from artifacts leading to “testing the wrong thing”.
  - Impact: New tests reuse `common`; do not handwrite schemas.
  - Status: Accepted

- 2026-05-29 — stdlib blocked by language design (node/effect top-level syntax)
  - Decision: Implementing built-in nodes (prompt/router/aggregator/tool/stream) and effect contract files is blocked on adding two new top-level constructs `node` and `effect` (design done, see `language_design.md` §13). Until then, keep stdlib crate as a shell; do not force-fit with existing 9 node kinds.
  - Rationale: Current grammar lacks `node`/`effect` top-level constructs and a closed effect set; force-fitting violates single-path and the intent of “built-in nodes”.
  - Impact: Mark stdlib as “blocked by language design” in the checklist; when landing, migrate hardcoded effects to stdlib predecls (triples for effects, unified `Family.Op(args)` references), remove grammar hardcoding, no dual-stack.
  - Status: Superseded (2026-05-30 fully removed node/agent; see final “Remove agent orchestration / node construct” entry)

- 2026-05-29 — Render prompts at call time in the scheduler (`StepPrompts` provider replaces static `StepRequests`)
  - Decision: The goal-progression scheduler and implement-loop in `workflow/engine` no longer take pre-rendered static `CompletionRequest` (`StepRequests`), but instead take a prompt-provider trait `StepPrompts` implemented at coordination layer: the scheduler calls it just before each LLM step with inputs derived from current graph state (decision/design pass active context + focus; implement passes the pseudocode body generated this round; repair passes prior candidate + diagnostics). The provider renders on the spot. The `ContextSnapshot` and the provider must be based on the exact same active-context computation (same source).
  - Rationale: As per `language_design.md` §10.7/§10.8 — the prompt is all the LLM sees and must be rendered from the active context at call time; that’s exactly what `consumed→ ContextSnapshot` snapshots/audits. Static pre-rendering causes: (1) no state evolution; (2) snapshot mismatches the actual LLM view; (3) implement steps miss the freshly designed pseudocode. This conflicts with “prompt = projection of active context at call time”.
  - Impact: Replace `StepRequests` with `StepPrompts` (single path, no dual stack); `run_goal_loop` / `run_implement_loop` accept `&impl StepPrompts`; schemas still selected by the scheduler (`prompt::schema_for`). Engine has no prompt templates/extraction logic (layering unchanged per §3.3); template rendering and leak-prevention stay in the provider. See `engineering_architecture.md` §8.4.
  - Status: Accepted (implemented: engine `prompts` module + `run_llm_step` rendering closure; CLI/e2e harness adapted; G3-01 proves 2 LLM rounds advancing to a candidate)

- 2026-05-30 — Goal-tree traversal layer independent of the spine (decompose/backtrack)
  - Decision: Move `decompose` (action 6) and `backtrack` (action 7) to a separate traversal layer above the spine `run_goal_loop` (`engine::run_goal_tree` in `traversal`), not inside the spine. `decompose`’s graph construction is done by deterministic helper `graph-db::build_decomposition` (LLM only provides decomposition structure). After `decompose_goal`, the traversal drives the spine to each child (DFS). `backtrack` only records abandonment.
  - Rationale: The spine is “linear advancement of a single goal” — independently testable/reusable (the CLI implement-loop uses only the spine). Shoving tree ops into the spine would make it a bloated mix of branch semantics. Splitting yields single-responsibility, separable testing, and honors §10.9’s “do not invent tree semantics inside the spine”.
  - Impact: Add `graph-db::{build_decomposition, ChildGoal, DecompositionNodes}`, engine `decompose_goal` and `traversal` layer (`run_goal_tree` / `GoalResolution` / `TreeBudget`), prompt `decompose` template + `decompose_result` schema; extend `StepPrompts` with `decompose` (update all impls). Honesty hard-constraint: `Decomposition` is the LLM execution-product node and thus has its own `consumed→ ContextSnapshot` (I6), anchored on the call that produced this structure, not the DecisionNode that triggered it (action choice vs execution, §10.8). `build_decomposition` takes and validates the snapshot. Child `Objective`s are structurally derived (like Assessment’s FirstSlice/Constraint), indirectly anchored via `member_of` instead of separate `consumed→`. Backtrack does not fake `WithdrawalEvent`; binding is not faked (accept/reject are human N4; LLM-derived children are unbound; binding is inherited after an AcceptanceEvent for the Decomposition). See `language_design.md` §10.9, `engineering_architecture.md` §8.5, `workflow_graph_spec.md` §I6 / 4.1.4 / 6.1.
  - Status: Accepted (implemented: 6 graph-db decomposition + 5 engine traversal + 1 prompt render tests; all workspace tests green)

- 2026-05-30 — Trace projection and the verifier executor’s ownership
  - Decision: (1) Execution trace (impl §9.4) belongs to `runtime`: `core/exec-ir` introduces stable `ExecEdgeId`; `runtime/trace` collects `ExecutionSpan` (node_id/edge_id projected to graph) during interpretation; `run_action` takes an explicit HostRegistry and returns `(Outcome, Trace)`. (2) The verifier executor for constraint audits belongs to `runtime` (`verify` module `run_hidden_case`), not `tools/audit`; `audit` remains a pure judgment layer (consumes injected `VerifierOutcome`), with coordination mapping `runtime::VerificationResult` to audit zero-loss.
  - Rationale: The interpreter is the only execution backend in v0 (architecture §3.2); anything that “actually runs code” belongs to runtime. Tools must remain deterministic/judgment-only, not depend on an execution backend (§3.3 inject-report). Same for trace (observability of the interpreter, §9.2), not core/tools.
  - Impact: `run_action` changes signature (`Execution` struct replaces tuple), and all call sites migrate (single path, no dual stack). Trace determinism: no wall-clock durations, only graph projection and entry order. Verifier executor connects to audit via coordination; the graph gate’s auto-drive is landed by “hidden verifier cases storage” below. See `language_implementation.md` §9.4 and `dev_checklist_v0.md` runtime/tools items.
  - Status: Accepted (implemented: exec-ir ExecEdgeId + runtime trace/verify; 4 trace + 6 verify + 1 CLI end-to-end tests; all 292 tests passed)

- 2026-05-30 — Storage for hidden verification cases: reuse runtime value model, no mirror types
  - Decision: Hidden-case storage (`sophia-runs/verifiers/hidden.json`) uses `runtime::Value` / `HiddenCase` / `ExpectedOutcome` directly (derive serde), instead of a separate `VerifierValue` mirror type.
  - Rationale: Single value model — hidden-case args/expectations feed the interpreter (`runtime::Value`); a mirror type would require two-way conversion and dual maintenance (violates single-path). `runtime::Value` is the authoritative representation — just make it serializable to serve both execution and storage.
  - Impact: Add serde to `runtime`; serialize `Value` as externally tagged union (`{"Int":42}`); CLI `verifier_store` deserializes `Vec<HiddenCase>`. Gates read `verifier.ref` from the raw `ConstraintNode` payload (not `ConstraintView`) due to anti-cheat: `ConstraintView` does not project the verifier, yet the gate needs the ref, so read from the raw node. Layering: loading hidden.json + chaining execution/judgment happens in the CLI coordination; tools/audit and runtime don’t know storage/graph.
  - Status: Accepted (implemented: runtime serde + CLI verifier_store + run_constraint_audit wiring; 3 CLI gate integration tests; all 296 tests passed)

- 2026-05-30 — Human authorization checkpoint in goal-tree traversal (decomposition reviewer + binding inheritance)
  - Decision: Add `DecompositionReviewer` trait (returns `ReviewDecision::{Accept, Reject}`) to `run_goal_tree`. After landing decompose nodes (create `Decomposition` + child `Objective`s) and before recursing into kids, call reviewer: Accept → create a real human `AcceptanceEvent accepts→ Decomposition`, children then inherit binding via `member_of` and enter their active contexts, then recurse; Reject → do not recurse, and do not fake a `WithdrawalEvent` (record `GoalResolution::DecompositionRejected`). Provide `AutoAcceptReviewer` (caller represents human authorization, still goes via real AcceptanceEvent).
  - Rationale: Fulfills design 5.3 / N4. Previously we recursed into LLM-derived children immediately; but children are LLM provenance, thus unbound by default (binding requires human implicit accept at chain head or AcceptanceEvent on chain), so their active context was empty and real LLM couldn’t proceed. Mock tests masked this (they didn’t read prompts). Correct modeling is an authorization checkpoint: engine does not fake human authorization; it exposes the seam for caller (human via CLI, e2e harness, or policy).
  - Impact: Export `DecompositionReviewer` / `ReviewDecision` / `AutoAcceptReviewer`; add reviewer parameter to `run_goal_tree` / `drive_goal` / `drive_decompose` (single path, all call sites updated); extend `GoalResolution` with `DecompositionRejected`. The e2e prompt provider becomes focus-aware (extract prompt surface per focus id from active context; identical for root focus), add `CaseKind::Tree` + `tree_drive` (via `run_goal_tree` + `AutoAcceptReviewer`) with G6 suite. Add 2 traversal tests (accept path inherits binding / reject path doesn’t recurse/fake). Existing 5 tests pass after adapting to reviewer.
  - Status: Accepted (implemented: engine reviewer path + 2 unit tests; e2e G6-01 wiring; real LLM run pending API key; all 307 tests passed)

- 2026-05-30 — Built-in node interpretation: single-node dispatch via EffectHost, multi-in/out deferred to assembly syntax
  - Decision: Make declared built-in `node` runnable (v0 subset): (i) exec-ir models `ExecNodeKind::Node`; (ii) runtime `EffectHost` adds `invoke_node_effect` (family/op dispatch), interpreter `run_node` delegates a node with exactly one non-Pure effect and single in/out to host; (iii) multi-in/out and Pure structural nodes (router/aggregator) error out with `RuntimeError` — scheduler not implemented.
  - Rationale: Node execution was a large semantic hole. But language had no surface syntax to assemble nodes into a graph, so implementing multi-in/out scheduling would be speculative dead code. Honest stance: implement the meaningful subset with a real entry (`sophia run <NodeName>`); leave multi-in/out to future syntax.
  - Impact: `core/exec-ir` (ExecNodeKind::Node + from_model + is_node), `core/semantic` (NodeDecl flags for multi_input/multi_output), `runtime` (EffectHost::invoke_node_effect + InMemoryHost stub + Interpreter::run_node). CLI unchanged. The stub marks it is not faking real LLM/tools/streaming backends; real backends belong to v1 host import. See `language_design.md` §13.5, `language_implementation.md` §20.1.
  - Status: Superseded (2026-05-30 node/agent orchestration fully removed; see next entry)

- 2026-05-30 — Fully remove agent orchestration / `node` construct; return to language positioning (repeals 2026-05-29 and 2026-05-30 node decisions)
  - Decision: Delete top-level `node` construct, built-in effect families `Llm`/`Tool`/`Stream`, five built-in node contracts, single-node interpretation, and the entire `sophia-stdlib` crate. Keep top-level `effect` and generic `Family.Op(args)` references (builtins only `Console`/`DB`, housed in `hir::builtins`).
  - Rationale: Confirmed with author — `node` + agent effects were smuggled in via the un-argued assumption that “stdlib must include prompt/tool/stream nodes”, introducing agent orchestration not aligned with Sophia’s positioning (§1: LLM-native deterministic language; LLM is the programmer, not an in-language primitive; compilers must not call LLM). Further: (i) library constructs should not be execution entrypoints; `node` had no body and was not invocable by actions; only imagined assembly existed, but no syntax; single-node interpretation was a fake entry; (ii) pushing external nondeterministic services into the standard library conflicts with the deterministic core stance; (iii) `sophia-stdlib` had no consumers (compiler reads effects from Rust tables, not from stdlib). Delete entirely (not disable). Keep `effect` construct — it fixes hardcoded effects and is independent of agents.
  - Impact: Grammar (remove node_def, regenerate parser.c), AST (remove Item::Node/NodeDef), HIR (remove NodeKind::Node, keep BUILTIN_EFFECT_OPS for Console/DB only), semantic (remove NodeDecl/model.nodes/check_node_contracts), exec-ir (remove ExecNodeKind::Node), runtime (remove run_node/invoke_node_effect), delete `sophia-stdlib` crate (remove from workspace members/deps), delete related tests. Docs: rewrite design §13 to effect-only and record “no node” decision at §13.5; architecture §4 rewrite; impl §20; concepts.md and README synced. All 298 tests passed. If agent orchestration is ever needed, it must be a separate explicit language direction with assembly syntax, not a stdlib byproduct.
  - Status: Accepted (replaces the two earlier node decisions; those are marked Superseded)

- 2026-05-30 — Benchmark: external interpreter dependency and task representation
  - Background: `docs/benchmark_design.md` establishes Sophia workflow vs “LLM generates Python directly” baseline. Sophia mode reuses existing `runtime::verify` (no new execution capability), but baseline must actually execute the LLM-generated Python. The workspace has no such capability.
  - Task representation: Use Rust `Problem` + reuse `runtime::Value` / `runtime::verify::HiddenCase` (isomorphic to e2e’s Rust `Case`), no external config format.
  - Decision: `baseline` runs a `python3` subprocess to execute candidates + `value_to_json` compare; by the “JIT dependencies” rule, `python3` is a runtime external tool dep and not in Cargo tree; if missing, baseline mode is skipped cleanly. Language is a parameter; only Python is implemented (widely available), not TypeScript.
  - Safety: sandboxed temp dir, 5s hard timeout, `DirGuard` cleanup.
  - Honesty: Propagate hard errors/timeouts honestly; do not fake passes. Sophia mode (reuse runtime::verify) and baseline (fresh subprocess + cross-language compare) are asymmetric by nature; acknowledged in docs. Anti-leak: `Problem::public_brief()` type-isolates hidden cases so prompts don’t receive answers; stronger than runtime guards.
  - Impact: Implemented in `cli/examples/benchmark/` (6 files; symmetric with e2e; not in `cargo test`; skips cleanly without key/python3); outputs under `sophia-runs/benchmark/<label>/{runs.jsonl, summary.md}` (two core metrics). L1–L4 with 6 tasks, non-overlapping with e2e. Sophia mode is a closed loop but not reusing e2e harness. Python runner validated by smoke tests.
  - Status: Accepted (example builds; clippy -D warnings clean; `--list` shows 6 tasks; all 298 tests pass; real LLM run pending key)

- 2026-05-30 — Add unary arithmetic negation `-x` to starter subset (fixes abs_difference failure’s root cause)
  - Background: Benchmark `abs_difference` in Sophia mode repeatedly used `-diff` (unary negation) for abs and failed to converge. Root cause: grammar’s `unary_expr` had only `not` (Bool), no arithmetic negation. Though `0 - diff` works, models don’t always find it. §16.5 lists integer arithmetic in the starter subset but omitted negation — deemed accidental, not intentional.
  - Decision: Add `-x` (Int→Int) across the full stack: grammar `unary_expr` gets `-`; regenerate parser.c with aligned tree-sitter CLI 0.26.9 (ABI 15); AST adds `Expr::Neg`; lower dispatches not/neg by op; semantic typing requires Int operand and yields Int; interpreter implements `Value::Int(-i)`. The shared syntax baseline makes negation explicit and notes still no `/` or `%` (intentional exclusions).
  - Rationale: General fix — removes an expression gap for all programs rather than only this task; follows the rule to fix language issues in design, not by patches.
  - Impact: 6 files + shared baseline + new `runtime` unary_negation test; all 299 tests pass. Docs: `language_implementation.md` §16.5 updated. `abs_difference` now passes with `-diff`.
  - Status: Accepted

- 2026-05-30 — Shared syntax baseline adds “name fidelity” rule (scaffolding fix for traffic_next failure)
  - Background: In `traffic_next`, the prompt used generic `Light` instead of `TrafficLight`; implementation matched `Light`, passed checks, but runtime validation failed on a call to `NextLight(TrafficLight.Green)`.
  - Decision: Add name-fidelity rule to the shared syntax baseline (`sophia_syntax_baseline`): names explicitly given in the task/acceptance (node/field/state value/error variant) must be used verbatim — no renaming/translating/abbreviating/case changes. Clarify difference from “don’t copy neutral example names.” Use a neutral `WidgetKind` example unrelated to any task; anti-leak assertions list benchmark tokens (AbsDifference/TrafficLight/…).
  - Rationale: Anti-leak safety — rule requires fidelity only to public task names and does not reveal hidden cases; generalizable across e2e and benchmark.
  - Impact: Update `sophia_syntax_baseline.md` + snapshot; add anti-leak tokens; fix traffic_next with exact `TrafficLight` naming; e2e benefits as well.
  - Status: Accepted

- 2026-05-30 — Entering v1: recalibrate phase positioning (two project goals, WASM is first-class)
  - Background: v0 (interpreter) core chain + workflow loops + e2e groups + benchmark ladder have run with real LLMs. Old report clarified two goals with priority: (1) primary — build a usable language/toolchain for autonomous LLM programming (serious engineering); (2) secondary — publish a paper.
  - Decision: Enter v1. v1 = “turn prototype into serious language”, with two parallel workstreams: A WASM codegen (must-have; interpreter becomes oracle/diff baseline) + B language/stdlib expansion (`Result<T,E>`/error handling/`task` execution/`entity.with`/cross-domain intent flow/contract proofs — admitted by machine-checkable value, to support more complex programs/L6+). Both are necessary: v1 is done when WASM equals interpreter, language expresses beyond v0, and strip-assist holds for artifacts.
  - Calibration: WASM is not optional/vague; it’s mandatory for goal (1). Benchmarks are evidence, not central value, but that doesn’t demote WASM’s priority.
  - Impact: Docs updated: `language_design.md` §1.1 adds “two goals”; `engineering_architecture.md` §14.2 reframes as WASM + language expansion with §14.3 evolvability; `language_implementation.md` §19.1 adds v1 build order; `benchmark_design.md` §3.1 marks ladder extending to L6+; `dev_checklist.md` overview becomes “v0 wrap-up / v1 start”. Documentation only.
  - Status: Accepted

- 2026-05-30 — Split progress checklists by version (v0 archived/frozen, v1 active); keep engineering notes unified
  - Decision: Rename `dev_checklist.md` → `dev_checklist_v0.md` and freeze as read-only (v0 interpreter phase done). Create `dev_checklist_v1.md` as the active SSOT, organized per `language_implementation.md` §19.1 (workstream A WASM codegen + workstream B language/stdlib), carrying over open items. `engineering_notes.md` remains unified — decisions cross versions.
  - Rationale: v0 checklist is a completed record; mixing v1 items obscures current progress. Versioned split keeps each focused; decisions remain unified context.
  - Impact: `git mv` keeps history; update references across repo — active pointers (README/CONTRIBUTING/PR template/CHANGELOG/concepts cheat sheet) point to v1; historical references (design score items, impl §19 v0 steps, trace/verify notes, graph_cmd comments) point to v0. Doc-sync discipline now says “sync current-progress checklist.” Docs only.
  - Status: Accepted

- 2026-05-30 — Workstream B becomes demand-driven with design reviews (audit correction; create `v1_demands.md`)
  - Background: v1 checklist draft turned §16.6 “outside of starter subset” labels (Result/entity.with/cross-domain/proofs/task) into a fixed sequence B7–B12. Review found these were labels, not designed features; sequencing them is overdesign/speculation. Some are v2+ scale; including in v1 breaks boundaries.
  - Decision: Workstream B becomes demand-driven with per-feature design reviews. Two admission paths: (i) demonstration need (add minimal feature strictly to meet a persuasive demo/benchmark need); (ii) strongly reasoned LLM-native feature (higher bar). Create `docs/v1_demands.md`: pick three v1 demos (D1/D2/D3) and backsolve minimal set F1 (`Result<T,E>`) + F2 (Http effect family) + S1 (HTTP host stdlib); each completes its design review before implementation.
  - Explicitly defer to v2+: `entity.with`, cross-domain/library intent flow, contract proofs, `task` execution — until a demo need appears.
  - Tighten v1 done-criterion 2 to the bounded “D1/D2/D3 pass end-to-end + one real accept/reject row for D2”.
  - Impact: Add `v1_demands.md`; update `dev_checklist_v1.md` workstream B and completion criteria; update `language_implementation.md` §19.1 workstream B; `engineering_architecture.md` §14.2; README/concepts index. Rationale for F2/Http: reuse intent + capability/effect + host, minimal surface, big value.
  - Status: Accepted

- 2026-05-30 — Language design guideline: do not emulate advanced human-language mechanisms; prefer semantic clarity / no elision / verbosity is fine
  - Decision: Sophia does not design user-extensible generics, templates, macros, traits/typeclasses, operator overloading, implicit conversions, or elision syntactic sugar (e.g., `?` propagation / `unwrap`). Syntax prioritizes explicit, semantically direct forms; verbosity is not a flaw. Existing closed built-in wrappers (List/Optional/Result/Intent) are fixed first-class constructs, not user-parameterizable; they do not constitute a generic system.
  - Rationale: Align with LLM profile — strong semantics, weak memory. LLMs are good at expressing intent directly as structure; they are bad at remembering and applying rules for expansion (type inference, macro hygiene, implicit traits, desugaring). Verbosity is not costly to LLMs; implicitness/elision is. Matches the philosophy: turn memory into expression.
  - Impact: `language_design.md` §3 adds the guideline + §3.2 tradeoffs + §12 Non-goal tightened; `v1_demands.md` F1 aligned (explicit `match` for Result-like forms, no `?`/`unwrap`, `E` only error variants). Future features must meet this yardstick: “shorter/more like another language/reuse abstraction” is rejected by default; “direct expression, local readability, machine-checkable” are positive signals. Long-term discipline across v0/v1/v2.
  - Status: Accepted

- 2026-05-30 — Delete `v1_demands.md` (fold into checklist) + two stdlib clarifications (functional library; prompt scaffolding)
  - Decision: (i) `v1_demands.md` was a temporary analysis doc; fold its essence (demand-driven methodology + D1/D2/D3 + boundaries) into `dev_checklist_v1.md` §2 and delete the file; update upstream references. (ii) Stdlib scope = functional library, not protocol stack: add only functional capabilities per need (e.g., `Http.Get`), do not build TCP/IP/TLS/socket stacks (use host runtime). (iii) Add S2: stdlib prompt scaffolding — LLM has no prior on libraries; provide standardized, on-demand prompt assets (purpose/usage/intent boundaries/capability) per library, using §8.3 preamble + `prompt/assets/` mechanics; inject only the libraries used. Minimal set becomes F1 + F2 + S1 + S2.
  - Rationale: Avoid keeping temporary docs; keep progress/boundaries in a single checklist source. Prevent scope creep into protocol stacks. Without prompt assets, LLMs cannot produce code using stdlib; S2 is a prerequisite for D2 and matches “context as assets, on-demand”.
  - Impact: Delete `docs/v1_demands.md`; inline demos in `dev_checklist_v1.md` §2; add S2 breakdown (S2.0–S2.3) and S1 functional scope note; update `engineering_architecture.md` §14.2 and `language_implementation.md` §19.1; README/concepts updated. Docs only.
  - Status: Accepted

- 2026-05-30 — F1 correction: drop Rust-like `Result<T,E>`, use `one of {...}` for fallible returns, and unify all type syntax
  - Decision: Reject prior F1 `Result<T,E>` (`Ok/Err`) approach. All fallible/nullable returns use `one of { members... }` unions — members are constructed/matched directly, no `Ok`/`Err`/`Some`/`None` wrappers. Unify type syntax: `<>` reserved for Intent Type only; structural types use `of` family (`list of T` / `one of { ... }` / `schema of T`); deprecate `Optional<T>` / `List<T>` / `Schema<T>` generic-like forms, `Some`/`None`, and `<optional>.exists`; add built-in `Null` (literal `Null`); add type patterns to `match` (`Int x =>` / `Todo t =>` / `Null =>` / `V { f } =>`, no `_`, same exhaustiveness). `storage.get` returns `one of { ValueTy, Null }`; `save` still returns `ValueTy`. Predicates for “has a value” use `!= Null` or `match` on `Null`. Spec in `docs/type_system.md` (replaces deleted `result_type.md`).
  - Rationale: (i) Under prior “Result as single-parameter wrapper” plan C, the wrapper added no information (members already named/tagged); and Result/Some/None reflected Rust mental model — implementation language ≠ language design. (ii) `<>` overloading for both intent and containers was a v0 exception; unify to “`<>` = intent; `of` = structure” (one rule, zero exceptions). (iii) “member is itself” is cheaper at runtime (no extra discriminant), matching LLM-native stance. Single path, thorough refactor, no compat layer, no sugar.
  - Impact: Full-stack refactor — grammar.js + parser.c (`intent_type`/`list_of`/`one_of`/`schema_of`, type/variant patterns, `Null` literal); `syntax` (AST `TypeRef`/`Pattern`/`Expr`, lower); `hir` (builtins `INTENT_WRAPPERS` + `Null`/`Unknown` scalars, resolve `one of` members + type-pattern bindings); `semantic` (`Ty::OneOf`/`Null`/`ErrorVariant`, distinguishability + exhaustiveness, storage-get typing, assignability upcast); `exec-ir`; `runtime` (`Value::Null`/`ErrorValue`, `one of` values are members, `match` by tag); benchmark value_json. Docs: `type_system.md`, `language_design.md` §3/§6/§7 + examples, `language_implementation.md` §7.1/§8.2/§16.1/§16.5/§16.6 + §14, `benchmark_design.md` / `e2e_test_design.md` / `architecture` §14.2 + `dev_checklist_v1` F1 block; shared syntax baseline rewritten + snapshot. 299 tests passed; clippy -D warnings clean; fmt clean; flagship probe passes end-to-end.
  - Status: Accepted (replaces earlier F1 `Result<T,E>` plan)

- 2026-05-30 — F1 refactor completeness audit: add `one of` distinguishability check + resolve type names in match patterns
  - Decision: After auditing, implement two checks specified by design but missing: (i) `one of` member distinguishability (`type_system.md` §2.2/§7/§9.5) — add `core/semantic/src/union_check.rs`, using the runtime match tag as criterion (scalars by type name; `Null` unique; entity/state/variant by name; intents erase at runtime so inspect inner; expand nested unions). Traverse all type positions (entity fields / storage / callable sigs / error variant fields / effect params), report `IndistinguishableUnion` (CHECK-TYPE-006) for repeated tags. (ii) Resolve type names in match type patterns — add `resolve_pattern_type_name` to HIR; `match x { Bogus v => }` now reports `UnresolvedReference`.
  - Rationale: Distinguishability is prerequisite for deterministic `one of` dispatch; otherwise first-match-wins destroys semantics. Resolving pattern types honors the “all references must resolve” rule.
  - Impact: `core/semantic` new `union_check` + diagnostic code; `core/hir/resolve.rs` type-name resolution; 6 new tests; remove stale comments. All 305 tests passed; clippy clean.
  - Status: Accepted

- 2026-05-30 — F2 landed: built-in `Http` effect family (D2 flagship), zero new syntax, isomorphic to storage
  - Decision: Introduce `Http` as built-in effect family (`hir::builtins::BUILTIN_EFFECT_OPS`), with body-level `Http.Get(url) -> Raw<Text>` via the same “special-root method_call + host delegation” path as `storage.X.get(k)` — zero new syntax. Three decisions (see `http_effect.md` §8): (i) effect identity `Http.Get` carries no URL arg (capability granularity is “can GET”; URLs are runtime values and would be wildcarded by `covered_by`); (ii) return bare `Raw<Text>` (network failures are host hard errors; D2 focuses on intent safety); (iii) `Http` is in the same “special root” class.
  - Rationale: Minimal surface, big LLM-native value — adds just one effect triple + one host method + one semantic branch, yet enables the flagship accept/reject matrix.
  - Impact: `core/hir` (builtins + special-root resolution), `core/semantic` (`type_layer::infer_effect_op`), `runtime` (`EffectHost::http_get` + `InMemoryHost` mock + `interp::try_effect_op`); 9 tests. `sophia_syntax_baseline` unchanged — Http knowledge lives in S2 prompt assets. Docs synced. Next: S1 (real reqwest host) / S2 (prompt assets).
  - Status: Accepted

- 2026-05-30 — S2 landed: stdlib prompt scaffolding (on-demand), before S1
  - Decision: Implement on-demand stdlib prompt assets — layout under `workflow/prompt/assets/stdlib/<lib>.md` (first: `http.md`); add `stdlib_asset` / `stdlib_libs` / `stdlib_preamble(&[libs])` APIs (dedup lexically; ignore unknown; empty set → empty string). Selection signal is the task’s explicitly declared libraries (e2e `Case.libs` / benchmark `Problem.libs`), not text sniffing. Inject `stdlib_preamble(libs)` in three implement systems (e2e harness / benchmark sophia_mode / CLI graph_cmd); default empty set yields zero injection/no regression. Choose S2 before S1: D2 needs S2, and F2’s mock host suffices for deterministic end-to-end; S1 is for live demos only.
  - Rationale: Establish the baseline vs library-assets boundary — persistent `sophia_syntax_baseline` carries core language grammar; library knowledge is in on-demand assets, keeping unrelated tasks’ context clean. Matches “semantic recovery / context pruning” stance. Anti-leak/snapshot discipline applies to assets too.
  - Impact: `workflow/prompt` (assets table + 3 APIs + `assets/stdlib/http.md`), `render.rs` (http snapshot + anti-leak assertions + selection test), `cli/examples/e2e`/`cli/examples/benchmark` (libs field + inject seam + update cases), `cli/src/graph_cmd.rs` (system(libs) seam). All 319 tests passed. Docs updated. Next: S1 or D2 demo.
  - Status: Accepted

- 2026-05-30 — S1 landed: real HTTP client host (coordination-layer injection; runtime zero I/O)
  - Decision: Real host for `Http.Get` lives in CLI coordination layer, not `runtime` (interpreter remains pure logic + zero I/O, delegates effects via `EffectHost`). `cli/src/http_host.rs` `CliHost` composes: reuse `InMemoryHost` for console/storage; override `http_get` with sync `reqwest::blocking` (fixed timeout; non-2xx/network/read failures are honest `Err` → `RuntimeError`). Runtime exposes `run_action` (injection entry; `run_action` remains default convenience). CLI `run` only injects `CliHost` if entry action declares `Http.Get`; otherwise use default in-memory host — zero cost/behavior change for non-network programs.
  - Rationale: Host import semantics are “host implements side-effects” (arch §4). Real network belongs to host (like real LLM backend in `workflow/llm`); core remains zero I/O. Sync reqwest matches sync `http_get`, avoiding bringing tokio into interpreter. Network failures are hard errors, not `one of` returns (consistent with F2); if a future demo needs recoverable network errors, expand types then via design review.
  - Impact: Add `blocking` feature to `reqwest`; `runtime` (`run_action`), `cli` (`http_host.rs` + host selection by effect + dep). Tests: runtime injection seam + CLI seam (delegation equivalence / invalid URL honest Err), no real network in `cargo test`. All 322 tests passed. Docs updated. F1+F2+S1+S2 complete.
  - Status: Accepted

- 2026-05-30 — S2 correction: library selection changes from “task-declared” to “design-time LLM self-selection from catalog”
  - Decision: Reject S2’s initial approach of declaring libraries in task metadata. New two-phase approach: (i) at design/revise, inject a library catalog (`prompt::stdlib_catalog`, “name — purpose” lines, no signatures), and the LLM selects libraries in `design_result.libraries`; (ii) at implement/repair, inject full assets for the selected set (`stdlib_preamble(selected)`). `libraries` flows via `PseudocodeArtifact` → scheduler `current_pseudocode` → `run_implement_loop` → `StepPrompts::implement/repair`. Remove `Case.libs` / `Problem.libs` and the corresponding prompt fields.
  - Rationale: Task-declared libraries leak solution direction and blow up with third-party growth. Correct model: like a real programmer, the LLM sees the library catalog (task-agnostic fact) and chooses; full usage injected only for the chosen set. Aligns with “separate choose/execute” and on-demand context.
  - Impact: `prompt` (assets as triples name/purpose/asset + add `stdlib_catalog`; schema adds `libraries`; templates updated), `engine` (carry `libraries` in `DesignResult`/`PseudocodeArtifact`; thread `libraries` through prompts and loop), `cli` (all StepPrompts impls updated; design injects catalog; remove task `libs`; graph persists `.libs` sidecar between commands). Tests updated; all 326 tests passed. Docs synced. Supersedes prior S2 approach.
  - Status: Accepted

- 2026-05-31 — Comprehensive tech-debt sweep and fixes (7 dimensions)
  - Decision: Following a system audit across directory structure / file naming / file bloat / duplication / dead code / style inconsistency / code–doc drift, land three batches: (i) deduplicate `code_check` bridging (was verbatim in CLI/e2e/benchmark) into `sophia_engine::code_check` + `domain_of_path` (workflow layer; observability printing left to callers); dedupe system prompt text likewise into `sophia_prompt::{design_system_prompt, implement_system_prompt}`; (ii) doc drift — update CHANGELOG [Unreleased] with F1/F2/S1/S2; README doc links; architecture prompt APIs/params; (iii) split giant `cli/src/graph_cmd.rs` (1351 lines) into `graph_cmd/mod.rs` (deterministic cmds + design/implement LLM) and `graph_cmd/gate.rs` (select/materialize gate reruns: code_check + constraint_audit + hidden-case + artifact_diff/runtime), with shared helpers re-exported.
  - Rationale: Duplication forces triple-fix and drifts; giant files harm readability/maintainability. Single sources + modularization align with the single-path principle.
  - Impact: Add `workflow/engine/src/code_check.rs`; prompt system functions; split CLI module; update e2e/benchmark to thin wrappers. All 326 tests passed.
  - Status: Accepted

- 2026-05-31 — D1/D2/D3 integration demos landed + fix F2 `Http.Get` arity latent bug
  - Decision: Land three v1 demos as end-to-end integrations at benchmark level L6: D1 `clamp_or_reject` (return `one of { Int, OutOfRange }` not raise), D2 `fetch_length` (`Http.Get → Raw<Text>` then intent_conversion to `Sanitized<Text>` and take length via mock host), D3 `record_pipeline` (fetch→validate→store pipeline). D2 accept/reject split: accept half (LLM + mock host) in benchmark; reject half (static rejection of unsafe candidates) in deterministic unit matrix `cli/tests/intent_matrix.rs`. Design review `docs/integration_demos.md` fully adopted.
  - Rationale: Fulfills v1 criterion 2; demos are compositions of existing features only. Reject half belongs in deterministic tests, not benchmark.
  - Impact: `runtime/src/verify.rs` adds host variants for hidden cases; benchmark problems extended; intent matrix added; fix F2 arity: `Http.Get` is arity=0 in builtins (caps granularity), not 1; strengthen tests; fix docs. This was exposed only by end-to-end Http programs.
  - Verification: Manual solutions validate expressibility/execution; language facts confirmed (multi-arg input separated by `;`; variant field binding obeys no-shadowing). All 333 tests passed. Status: Accepted

- 2026-05-31 — Stdlib relocation: remove `storage`/`DB`/`Persisted`; “I/O = library”; plan File library
  - Decision: Choose model (B): file/database/network are standard libraries, not language primitives. Therefore: remove `storage` top-level node + `DB.Read/Write` effect family + `storage.X.get/save` syntax; remove `Persisted<T>` intent; keep `print`/`Console.Write` as language built-in. Add File library in v1 (at least `File.Read -> Raw<Text>` and `File.Write`); DB as future candidate (after semantics clarified). Docs first, then code.
  - Rationale: Separate mechanism (effect/capability/intent + syntax) from specific I/O family (Console/File/Http/DB). Builtins table is an implementation fact, not a design claim. `storage` was a confused construct with dedicated syntax; removing aligns the boundary.
  - Impact: Docs gate, then code refactor across grammar/AST/HIR/semantic/runtime/builtins, downstream example updates, tests regenerated. Remove Persisted; delay D3 until File rewrite. Landed as R0 (docs), R1 (removal), R2 (File library). 336 tests passed. Status: Accepted

- 2026-05-31 — Stdlib relocation R3: redo D3 with File + add e2e File case (relocation wrap-up)
  - Decision: Redo D3 as benchmark L6 `archive_or_reject` using File library and add e2e G5-01 for File; both are integration demos, no new language features.
  - Rationale: After storage removal, replace “store” step with File; fill e2e coverage gap for file I/O.
  - Impact: Add `file_seed` to problems for mock; update benchmark/e2e harness accordingly; add anti-leak tokens. Verified by hand; all 336 tests passed. Status: Accepted

- 2026-05-31 — Workstream A (WASM codegen) design review completed: seven decisions adopted
  - Decision: After completing B (F1/F2/S1/S2 + stdlib relocation + D1/D2/D3), proceed with A per `wasm_codegen.md`: (i) no new lowered IR (codegen walks AST; interpreter is oracle); (ii) value ABI = tagged heap values + i32 handles + bump-only linear memory (no GC/refcount); (iii) strings/names in data-section constant pool; dynamic values allocated; fields in lexical order; (iv) raise via `Outcome` (kind + handle) over return channel, mirroring interpreter; no WASM exceptions; (v) pure value helpers generated inside the module; only true I/O via host imports; (vi) use pure-Rust WASM encoder + pure-Rust interpreter for diff tests (cargo-test friendly); (vii) new `tools/codegen` crate (deterministic toolchain layer, depends on core, zero I/O). Diff tests reuse known-good reference solutions.
  - Rationale: Interpreter remains sole oracle; codegen must not force IR changes; pure-Rust tooling keeps CI deterministic.
  - Impact: Add `tools/codegen` (W1 skeleton); extend `tools/check` for artifact strip-assist; wire `sophia build` to emit wasm; diff harness reuses bench/e2e references. Status: Accepted (docs only here)

- 2026-05-31 — Workstream A · W1 landed: freeze codegen input contract + new `tools/codegen` crate
  - Decision: Implement `CodegenInput` encapsulating `SemanticModel`, `ExecGraph` (built inside constructor via `ExecGraph::from_model`, same source as interpreter), and all ASTs; `emit_module` returns `NotYetImplemented` honestly at W1.
  - Rationale: Encode “codegen consumes IR and cannot mutate it” in types; ensure same-source graph as interpreter for diff equivalence.
  - Impact: Add `tools/codegen/{lib.rs, contract.rs, error.rs, tests/contract.rs}` and workspace registration; no changes to existing crates.
  - Verification: Contract tests (graph↔model callable match incl. cross-call edges; honest NotYetImplemented). All 338 tests passed. Status: Accepted

- 2026-05-31 — Workstream A · W2a landed: minimal WASM emit (scalar core) + diff harness (A2 + A3 start)
  - Decision: Choose pure-Rust `wasm-encoder` 0.243 and `wasmi` 0.40 (compatible with rust 1.95 MSRV); value ABI and emit cover scalars, boolean ops, let/set, if-else, cross-call; unimplemented constructs return `NotYetImplemented`.
  - Rationale: Interpreter is oracle; incremental coverage enables per-step diff validation; pure-value helpers remain in-module.
  - Impact: Add `abi.rs`/`emit.rs`, deps, tests; workspace adds versions. All 344 tests passed. Status: Accepted

- 2026-05-31 — Workstream A · W2b landed: error algebra + `one of` returns + `match`
  - Decision: Model `ErrorValue` layout (named record with lex-ordered fields), constant string pool, and emit for `raise`, variant constructs, `match` with scalar/type/variant patterns; guard `Eq`/`Ne` to scalars for now.
  - Rationale: Interpreter parity; deterministic ordering supports strip-assist; honest placeholders for others.
  - Impact: Update ABI/emit; add helpers; extend diff tests. All 347 tests passed. Status: Accepted

- 2026-05-31 — Workstream A · W2c landed: aggregate values (Entity + State)
  - Decision: Add State layout, intern entity/state names, helpers, entity construct, field access, entity/state patterns. Unimplemented: `repeat`, Text/List, `Text.length`, stdlib I/O, nested record fields.
  - Rationale: Interpreter parity; reuse record layout for Entity/ErrorValue; incremental steps with diff tests.
  - Impact: Update ABI/emit; tests extended. All 351 tests passed. Status: Accepted

- 2026-05-31 — Workstream A · W2d landed: Text + `repeat`
  - Decision: Text layout, intern literals, helpers (`make_text`/`text_length`/`text_concat`/`get_text_*`), Unicode scalar-count for length, static dispatch of `Add` (Int add vs Text concat), `repeat` loop; honest placeholders for `print`/`to_text`/`List`/I/O.
  - Rationale: Interpreter parity; Unicode length is a red line for equivalence; YAGNI for `to_text`/List.
  - Impact: Update ABI/emit; tests extended. All 355 tests passed. Status: Accepted

- 2026-05-31 — Workstream A · W4 landed: WASM effect host imports (Console / File / Http)
  - Decision: Map side effects to 5 `sophia_host` imports with byte-level ABI; all modules declare the same imports; real vs mock host provided by instantiator; failures trap; deterministic structure supports strip-assist.
  - Rationale: Keep module self-contained for pure values; only I/O via host; diff tests use pure-Rust mock host mirroring `InMemoryHost`.
  - Impact: Add imports/types; wire emit for effects; extend diff tests. All 358 tests passed. Status: Accepted

- 2026-05-31 — Workstream A · W5 landed: strip-assist gate at artifact layer + `sophia build` emit
  - Decision: Artifact-level strip-assist gate in `tools/codegen` (`emit_from_sources`, `check_artifact_strip_equivalence`); `sophia build` now checks → artifact gate → emits `.wasm`; unimplemented constructs report honestly. Codegen depends on `sophia-hir` for resolving.
  - Rationale: Interpreter remains oracle; artifact gate validates “assist does not leak into bytes”; deterministic emit makes byte-compare meaningful.
  - Impact: Add `build.rs` and exports; CLI build updated; tests added. All 362 tests passed. Status: Accepted

- 2026-05-31 — CI pipeline + fix distorted MSRV declaration
  - Decision: CI with two jobs: main gate (fmt/clippy/test/release, all `--locked`) and MSRV guard (use `rust-version` to install toolchain and run build+test). Correct MSRV from 1.80 to 1.95 per reality, with comments.
  - Rationale: Encode disciplines into CI; `--locked` for reproducibility; honest MSRV reflects transitive dep reality.
  - Impact: Rewrite CI workflow; adjust `Cargo.toml` MSRV. Verified: 1.95 passes; stable passes. Status: Accepted

- 2026-05-31 — Three-class testing: unit / e2e / benchmark; remove mocks from e2e/benchmark; consolidate docs as three guides
  - Decision: Only three kinds of tests are allowed. Unit tests (deterministic, may mock). e2e (real LLM + real I/O, no mocks). Benchmark (vs Python success/time, no mocks, pure-logic tasks). Integration demos are not a fourth class; they merge into e2e (D1/D2/D3 mapped). Use `example.com` for Http; real temp files for File.
  - Rationale: Mocks hide errors; e2e/benchmark must validate real behavior. Keep gates simple and non-overlapping.
  - Impact: Split CLI bin/lib for reuse; inject real `CliHost` based on entry effects; remove mocked demos from benchmark; remove hidden-case host variants; replace design docs with three test guides; update references; update anti-leak tokens. All 359 tests passed. Status: Accepted

- 2026-05-31 — Library plugin model: manifest-driven + registry + path B host + stdlib crate (P1)
  - Decision: Refactor libraries into a single-source manifest (`library.toml`) and a `LibraryRegistry` consumed read-only by each layer (invert indexing from layer→slices to lib→manifest→registry→layers). Adopt decisions from `library_plugin.md`: split `sophia-library` (contracts) and `sophia-stdlib` (contents); embed wasmi for third-party WASM host (oracle preserved; P2 wires); third-party roots `./sophia_libs/` + `$SOPHIA_LIB_PATH`; reuse codegen `sophia_host` byte ABI; place `HostFn`/`HostRegistry` in `sophia-runtime` (needs `Value`); `sophia-library` holds Value-less contracts; libraries map to domains; TypeDesc limited to `Scalar|Unit|Intent<Scalar>` initially; no `abi_version` support; choose host path B (`(family,op)→Box<dyn HostFn>`). Stdlib becomes a crate. Surface (Sophia source/effect-op) × host (none/native/WASM) are orthogonal so programs run in both interpreter/VM modes.
  - Rationale: Make libraries first-class and remove hardcoded slices; path B yields symmetry for native vs WASM hosts; oracle parity requires both modes to support the same libraries.
  - Impact: Add `sophia-library` and `sophia-stdlib`; refactor runtime host; rebuild HIR symbols from registry; drive semantic/type from registry; update prompt/check/codegen/cli/lsp/e2e/benchmark; docs rewritten; add `toml` dep. Verified zero behavior change (File/Http identical). All 366 tests passed. Status: Accepted

- 2026-05-31 — Library plugin P2: third-party discovery + two demo libs (hash_sophia / hash_wasm)
  - Decision: Implement discovery and wasmi-embedded WASM host; add two demos computing the same deterministic digest: `hash_sophia` (pure Sophia) and `hash_wasm` (WASM effect-op). Reject sqlite as first demo (sandbox/ABI/semantics reasons). Fold `library_plugin_p2.md` into stdlib docs. Demos are deterministic and go into `cargo test`.
  - Rationale: Validate plugin mechanism with clean, comparable behavior; keep tests deterministic.
  - Impact: Extend HIR with library domains and discovery; add `WasmHostFn` to runtime; fixtures/tests; docs updated. All 369 tests passed. Status: Accepted

- 2026-05-31 — Library plugin P2 wrap-up: CLI wiring (discovery + source inclusion + WASM host registration)
  - Decision: Wire discovery into CLI: use `full_registry_for(root)`; include library sources into program inputs/ASTs; register WASM hosts for `host.wasm`. Strip-assist must be registry-aware to avoid false inequivalence.
  - Rationale: Host type (native vs WASM) is determined by shipped artifacts, not manifest; include sources so library nodes parse/execute; use same registry for original/stripped builds.
  - Impact: Update stdlib discovery/host registration; CLI commands include sources and register hosts; tools/check/codegen made registry-aware; tests adjusted; docs synced. Verified by smoke: both Sophia/WASM paths yield same digest. All 372 tests passed. Status: Accepted

## Recording template (for future entries)

- YYYY-MM-DD — <short title>
  - Decision: <what was chosen>
  - Rationale: <why>
  - Impact: <affected code/process>
  - Status: Accepted | Superseded | Proposed
