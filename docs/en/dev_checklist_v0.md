# Sophia Engineering Progress · v0 (dev_checklist_v0, archived)

> Status: v0 (interpreter) phase archived and frozen. The core v0 chain and workflow loop are complete; this document is a read-only record of v0 progress and change history. v1 progress tracking lives in dev_checklist_v1.md (active SSOT). Engineering decision log remains unified in engineering_notes.md (cross-version, not split into v0/v1).
>
> This document is organized by the v0 build order (steps 1–15) from language_implementation.md §19, and also lists engineering infrastructure items. Status categories: Completed / Partially Completed / Not Completed / Planned (Roadmap).

---

## I. Overview

Current phase: v0 interpreter (essentially complete) → preparing to enter v1. The core of build steps 1–15 has landed end-to-end: the compiler mainline (parse → HIR → semantic → exec-ir → interpreter run) runs the starter subset; Development Graph persistence (SQLite + event sourcing + invariants + provenance factories), Active Context derivation, LLM structured-output fallback and prompt templates, Materialize Gate type-state chain, and Language Server initial features are all implemented.

A Rust workspace and strict layering are established (`core` has zero I/O and does not depend on `workflow`). LLM is connectable: `HttpLlmClient` supports OpenAI-compatible and Ollama; `run_llm_step` codifies “build snapshot → structured call → emit nodes / RawLlmNode fallback”; assessment decomposition protocol has landed; the workflow execution loop (design/implement/repair building Pseudocode/Code nodes + `addresses→`/`implements→`/`repairs→` edges), implement-loop (implement→check→repair budget loop), Selection/Materialize orchestration, and the spine scheduler driven by DecisionNode (decision → design → implement-loop → candidate ready, with budgets / I6) have all landed. High-level scheduler actions (revise/clarify into spine; decompose/backtrack in the separate goal-tree traversal above spine); multi-candidate scoring; top-level `effect` declaration (remove hardcoded effects, `Family.Op(args)` and built-in families Console/DB + user-declared effects); execution Trace projection (exec-ir stable edge IDs + runtime trace + CLI `--trace`); constraint audit verifier executor + hidden-case storage (run regression gate truly on candidates) have also landed. All are single-path with no functional fallback (see engineering_notes.md single-path principle). The CLI can drive the Development Graph end-to-end from the command line: `graph init`/`start`/`context`/`nodes`/`design`/`implement-loop`/`select`/`materialize` (full chain from start to materialize, gate rerun before writes and atomic write to `domains/`), and offers deterministic convenience commands `smoke` / `repair-context` / `run --trace`.

v0 wrap-up / v1 start: v0 core chain and workflow loop are complete; six e2e groups and the benchmark ladder L1–L5 have run with real LLMs. The next phase v1 is “turn the prototype language into a serious language” (language_design.md §1.1 Goal 1) with two parallel workstreams: A WASM codegen (extend the execution backend beyond the interpreter to a deployable artifact; the interpreter becomes the oracle) + B language/stdlib expansion (`Result<T,E>` / error handling / `task` execution / `entity.with` / cross-domain boundary / contract proofing, to support more complex “serious programs” and L6+ benchmarks). See language_implementation.md §19.1 for v1 build order; engineering_architecture.md §14.2 for the route.

> Note: Early work once introduced a top-level `node` construct and agent orchestration (prompt/router/aggregator/tool/stream built-in nodes, `Llm`/`Tool`/`Stream` effects, `sophia-stdlib` crate, single-node interpretation). As it diverged from the language’s positioning (agent orchestration is not a goal of this language), this path was fully removed on 2026-05-30 (see change log); the top-level `effect` construct, as the correct de-hardcoding result, remains.

Milestones from language_implementation.md §19:

| Step | Subsystem | Status |
| ---- | --------- | ------ |
| 1 | syntax: grammar / CST / AST / span | Completed |
| 2 | hir: name resolution / module resolution / scope | Completed |
| 3 | semantic: type / effect / contract | Completed |
| 4 | exec-ir + interpreter: starter subset runs | Completed |
| 5 | interpreter runtime input/output validation | Completed |
| 6 | GraphStore + node/edge schema | Completed |
| 7 | ContextSnapshot + active context derivation | Completed |
| 8 | Small nodes (Decomposition/Constraint/AcceptanceCriterion) | Completed |
| 9 | Core goal nodes (Objective/Milestone) | Completed |
| 10 | Event nodes (Acceptance/Withdrawal/Activation) | Completed |
| 11 | Assessment family (Assessment/FirstSlice/Decision decomposition protocol) | Completed |
| 12 | DiagnosticNode (5 kinds) | Completed |
| 13 | LLM integration (design/implement/repair/decision) | Completed |
| 14 | Selection/Materialize + Materialize Gate type-state chain | Completed |
| 15 | LSP basics (hover/diagnostics/goto) | Completed |

---

## II. Worklist

### 2.1 Completed

#### Engineering infrastructure
- [x] Cargo workspace with 14 member crates (`core` ×4 / `workflow` ×4 / `tools` ×3 / `lsp` / `cli` / `runtime`)
- [x] Strict layering: `core/*` zero I/O, no deps on `workflow` (only pure libs like `thiserror`/`slotmap`/`serde`)
- [x] `workspace.dependencies` to unify dep versions
- [x] Version alignment: tree-sitter crate 0.26 + CLI 0.26.9 + ABI 15 (satisfy triad alignment)
- [x] `rustfmt.toml` (edition 2021 / max_width 100 / Unix newlines)
- [x] `.gitignore` (target, generated artifacts, SQLite, node_modules)
- [x] git init (local `main` branch, no remote; initial commit recorded, see 2026-05-30 in change log)
- [x] Error handling baseline: libs use `thiserror`, binaries (cli) use `anyhow`
- [x] append-only / I9 CI invariant test (graph-db/tests/append_only.rs): via read-only auditor `GraphStore::raw_event_log` ensure event log append-only — after each write, the old log is a byte-for-byte prefix of the new; rejected writes have no side-effects; reopening and replay does not rewrite history (deterministic, in `cargo test`/CI)

#### Syntax layer (core/syntax, build step 1)
- [x] Sophia-Core Tree-sitter grammar: covers all 9 top-level nodes (domain/entity/state/transition/error/capability/storage/action/task)
- [x] Body sublanguage: let/set/return/raise/if-else/match/repeat/print + restricted expressions
- [x] Type syntax: scalars, `List<T>`, `Optional<T>`, Intent wrapper, entity/state refs
- [x] Semantic Assist fields parsed as separate nodes (for strip-assist equivalence gate)
- [x] Key disambiguation: dot field access as `field_access`; state value pattern as `qualified_name`; match head uses no-struct expression variant; `_` catch-all disallowed at syntax level (permanent ban)
- [x] `tree-sitter.json` (ABI 15) + `build.rs` (compile local `parser.c` only, do not embed external repos)
- [x] CST wrapper `SyntaxTree`: root, source slice, `to_sexp`, resilient diagnostic collection (deterministic preorder traversal)
- [x] `Span` / `Point` (0-based line/col + byte offset), carried through later IRs
- [x] Stable entry `parse_str`; typed `SyntaxError` + `SyntaxDiagnostic`
- [x] AST data model (`ast`): arena + `ExprId` refs (expression arena), full coverage of top-level Item/Callable/Block/Stmt/Pattern/Expr; Semantic Assist modeled separately
- [x] CST → AST lowering (`lower`): drop trivia keep span; resilient and panic-free; strip string quotes + escapes; stable entries `parse_ast` / `SyntaxTree::to_ast`
- [x] Unit tests + CST insta snapshots + lowering integration tests (13 cases, cover all node types and body sublanguage)

#### CLI (cli, deterministic commands wired end-to-end)
- [x] `clap` framework + `tracing` init; modular layers (`project` scan / `render` diagnostics / `commands`)
- [x] `sophia init`: create standard dirs (domains + sophia-runs/{generated,task_closures,build,graph}) and `sophia.toml` (5.2 minimal config)
- [x] `sophia parse <file>`: single-file parse + syntax diagnostics (1-based lines)
- [x] `sophia index`: scan `domains/` (lex order determinism) → emit `sophia-runs/asg_index.json` (17.2 spec)
- [x] `sophia graph`: print ASG node summary (name/kind/domain)
- [x] `sophia check`: syntax + HIR name resolution + three-layer semantics; diagnostics attributed precisely to files (`resolve_item` / `analyze_one_callable`, stable codes)
- [x] `sophia context --action/--task`: compute semantic closure from action/task root (§8, deterministic, no LLM), stable outputs of nodes/edges/files; `--sources` includes source text
- [x] `sophia build`: v0 no-op (declared after successful check, no codegen)
- [x] `sophia run <action>`: scan → check gate → interpret; `--arg Type:Value` args; replay console, show return or raise (domain error = nonzero exit)
- [x] 6 end-to-end integration tests (init/index/check/run/syntax fail rejection/raise propagation, drive compiled bin)

#### CLI Development Graph workflow subcommands (cli `graph_cmd`, architecture §9.2)
- [x] `sophia graph` refactored to optional subcommands (no subcommand = ASG summary, backward compatible; `--root` as a flag)
- [x] Deterministic subcommands (no LLM): `graph init` (create SQLite event-sourced store) / `graph start <title>` (human ObjectiveNode) / `graph context` (derive and show active context, no writes) / `graph nodes` (list nodes; replay across processes persists)
- [x] LLM subcommands: `graph design <NodeId>` (design_solution → PseudocodeNode; `.pseudo` saved to `sophia-runs/graph/artifacts/`) / `graph implement-loop <NodeId> --pseudo <PseudoId> --max-repairs N` (implement→code_check→repair budget loop → candidate files saved to artifacts, not materialized)
- [x] select / materialize subcommands (last mile): `graph select <CodeId>` (rerun gates → SelectionNode `selects→ Code`) / `graph materialize <SelectionId>` (follow `selects→` to candidate → rerun gates → atomic write to `domains/` + MaterializeNode `materializes→ Selection`)
- [x] Gate reruns (type-state proofs cannot be persisted across processes; for irreversible writes, rerun is safer, design §10.10): code_check (bridge `tools/check`) / constraint_audit (run `tools/audit` for bound invariants; declared executable verifier without executor → hard error, honestly reflects “to be wired”) / artifact_diff (strip-assist equivalence) / runtime validation (no hidden cases at starter stage → pass, not faked). Each gate emits `DiagnosticNode` `checks→ Code`; any failure blocks (no faking success)
- [x] Engine refactor: split `run_selection_materialize` into `run_selection` + `run_materialization` primitives (CLI are two processes). `CodeCandidate<Selected>` remains the proof across both
- [x] LLM backend flags (`BackendArgs`): `--model` / `--mode openai|ollama` / `--base-url` / `--api-key` (or env `SOPHIA_LLM_API_KEY`) → build `HttpLlmClient`; CLI uses one-shot current-thread tokio to cross async
- [x] code_check bridge (`code_check_files`): bridge candidates to `tools/check` (syntax → HIR → semantic → strip-assist), produce `DiagnosticPayload(CodeCheck)` for implement-loop — CLI injects results, engine does not run checkers (layering)
- [x] Failures are not faked: backend unreachable → keep RawLlmNode (`attempted→ target`) and exit with failure; candidate file writes reject absolute paths/`..` escape
- [x] 6 CLI integration tests (graph without subcommands still shows ASG summary / dev workflow init→start→nodes→context / start append across processes / design unreachable backend emits RawLlmNode and fails / design rejects illegal nodes / implement-loop rejects non-Pseudocode source) + 7 unit tests (4 for `code_check_files` + 3 select/materialize: clean candidate select→materialize writes domains / code_check failure blocks select / materialize rejects non-Selection)

#### HIR layer (core/hir, build step 2)
- [x] ASG index (`AsgIndex`): name → `NodeInfo{kind,domain,path}`; `BTreeMap` stable order; `to_json` matches §17.2 (top-level nodes only); one file per node; forbid cross-file duplicates
- [x] error variant member symbol table: variants are not top-level nodes; build separate table (`#[serde(skip)]` excluded from JSON); validate `errors { ... }` and `raise Variant`; forbid duplicate variant names across errors
- [x] name resolution (5.2): type refs (scalar/wrapper/entity/state), capability binding, error variants, entity construct/transition call, callee (builtins/transition/action), task include; distinguish `WrongReferenceKind` vs `UnresolvedReference`
- [x] builtins table: scalar types, 13 wrappers (containers + gradual + Intent), builtin `to_text`, special roots `self`/`output`/`storage`
- [x] scope analysis (language design §7): input as root scope; child scopes for `let`/`if`/`repeat`/match arms; forbid shadowing visible vars (incl. input); `Some(name)` binding lives only in that arm; `set` target must be declared and mutable
- [x] cross-domain checks: implicit cross-domain refs report `ImplicitCrossDomain`; task include is the explicit exemption
- [x] Resilient diagnostics (`HirDiagnostic` + 6 kinds with spans); hard errors via `HirError`
- [x] Top-level entry `resolve_program` (build index + per-node resolution with stable ordering); 15 integration tests

#### Task Closure / Semantic Paging (core/hir, language design §8)
- [x] `action_context` (§8.1): from action root, traverse ASG neighborhood — bind capabilities; input/output types; storages for effects; errors referenced by errors; called action/transition (recursive); constructed entities; files by domain
- [x] `task_context` (§8.2): include from task entries and merge respective formal deps; apply `task.exclude` — if a formal dep (storage) is excluded, report `ExcludedDependency` (do not silently drop)
- [x] Explanatory edges `ContextEdge` (§8.1 step 8): `binds_capability`/`calls`/`raises`/`reads`/`writes`/`uses_type`/`in_domain`/`includes`, explain why each node enters the context
- [x] Pure HIR computation (consumes AST + AsgIndex, zero I/O); deterministic output (nodes/edges/files deduped and sorted); root kind validation (action/task) and missing-root error
- [x] 7 tests (action closure coverage / explanatory edges / determinism / root-type validation / missing root / task closure deps / exclude error)

#### Semantic IR (core/semantic, build step 3)
- [x] Three-layer structure: `type_layer` / `effect_layer` / `contract_layer`, unified entry `analyze_program`
- [x] Table pattern (§6.2): declaration info (`SemanticModel`, immutable) separated from derived results (`TypeTable`, indexed by `ExprId`, recomputable); AST nodes are not mutated
- [x] Normalized type `Ty`: scalar / `List` / `Optional` / `Schema` / `Unknown` / entity / state / `Intent`; gradual recovery for `Unknown`/`Error`; `assignable_to` enforces strict intent equality (§7.2)
- [x] Type-layer checks (§7.6): field assignments / return / call arg types; entity construction requires full field coverage / unknown field / field type; expression intent inference (`+` preserves left intent); non-Unit must return/raise on all paths (flow termination); match exhaustiveness (Bool/state/Optional; `_` forbidden)
- [x] Effect-layer checks (§7.3): `used ⊆ declared` (includes called actions’ effects merged via type layer), `Pure` is exclusive with other effects
- [x] Contract-layer checks (§7.4/§7.5): capability deny overrules allow; effects require capability binding; `raise` variants must be declared; callee’s errors must propagate
- [x] Intent boundaries: `Console.Write` accepts only literals / `Sanitized<T>` / `Redacted<T>`
- [x] Compiler diagnostics (`SemanticDiagnostic` with span and stable code; 17 kinds); 18 integration tests (incl. canonical TodoDomain subset)

#### Execution Graph IR (core/exec-ir, build step 4)
- [x] `ExecGraph` / `ExecNode` / `ExecEdge`: one execution node per callable (`Action`/`Transition`) in lexical-name order (deterministic outputs); `EdgeKind` has 5 kinds (Data/Stream/Control/Conditional/Fallback)
- [x] `from_model` builds nodes from the Semantic model; calls in body to action/transition become `Control` call edges (starter subset has no concurrency/await/retry)
- [x] Interpreter executes via Execution Graph IR (design §9.2 pipeline `Semantic IR → Execution Graph IR → Interpreter`): interpreter owns the `ExecGraph`; on each callable entry it resolves in the graph (node existence + call-edge checks), making exec-ir a real bridge instead of a dead artifact

#### Interpreter (runtime, build steps 4–5)
- [x] Runtime value model `Value`: scalar / List / Optional / entity record / state tagged union; `RaisedError` (variant tag + fields)
- [x] Interpreter `Interpreter`: full body sublanguage (let/set/return/raise/if-else/match/repeat/print + expressions); `Signal` for return/raise control flow; lexical-scope environment; `match` pattern matching and bindings
- [x] Routed via Execution Graph IR: interpreter consults `ExecGraph`; callable invocation first resolves on the graph (`ExecNode` presence + `Control` call edge), fulfilling design §9.2 (previously exec-ir was not consumed; now fixed)
- [x] Cross-file call resolution: owns whole-program AST; `cur_ast` saved/restored on recursion (`ExprId` valid only within owning AST); transition constructed-call reorders args by input field order
- [x] Effect host abstraction `EffectHost`: in v0 the only observable runtime effect is `Console.Write` (via `print`), handled by host `console_write`; default `InMemoryHost` captures console output for testability. `DB.Read/Write` can be declared and statically checked, but runtime execution depends on body-level storage ops (§16.6 extension subset) and is not in the v0 interpreter; do not pre-reserve dead host APIs
- [x] runtime input/output validation (step 5): at action boundary, validate args and return structure with entity/state/error metadata (`validate::check_value`, consume Semantic metadata directly, no intermediate IR); intent is static; runtime only validates structure
- [x] Integration tests (parse→HIR→semantic→exec-ir→run across arithmetic/control flow/repeat/print capture/state & optional match/raise/entity construct/cross-file calls/transition calls/input validation)

#### Development Graph persistence (workflow/graph-db, build step 6)
- [x] Vocabulary: `NodeId` (`N0001` format, serde as string), `Provenance`, `NodeRole` (20 kinds), `NodeCreationStatus`; `Provenance::allowed_for` implements the provenance×role matrix
- [x] `NodeMeta`: `#[serde(deny_unknown_fields)]`, contains id/role/provenance/creation_status/created_at/summary/tags/model/prompt_artifact/response_artifact (1.2)
- [x] 20 payload schemas (section 4): all `deny_unknown_fields`; `StateAssessment` as a tagged union; `NodePayload` unified union by role tag; `role()` validates meta.role matches payload
- [x] Edge directory (section 6): `EdgeKind` 27 kinds; `allows(from_role,to_role)` encodes all hard constraints over `(from,to,type)` (including `T*` multi-role sides)
- [x] `GraphStore` (SQLite + event sourcing): `graph_events` append-only; `replay` rebuilds in-memory view; no update/delete APIs (N1/N2/I9); ID allocation is non-reusing
- [x] Append-time invariants: I2 (role×provenance), I8 (Failed only for RawLlm), I3 (edge role constraints), I5 (dangling refs), I4 (supersedes: same role/no cycles/single outgoing); payload field constraints (non-empty, Pseudocode artifact_path, ContextSnapshot digest 64-hex, Decision confidence in [0,1], Clarification kind↔provenance)
- [x] I6 overall check `validate_i6`: LLM-provenance Decision/Pseudocode/Code/Assessment must have `consumed→ ContextSnapshot` (as a tail invariant; allows writing node then adding edge)
- [x] 16 tests (incl. replay round-trip; negative paths for invariants)

#### Active Context derivation (workflow/graph-db, build step 7)
- [x] `ActiveContext` and `*View` types: expose only a subset (id + key fields), do not leak full `NodeMeta`
- [x] Binding predicate (§5.2): chain head + (human implicit accept ∨ chain has AcceptanceEvent), and no later WithdrawalEvent; version chains with `chain_of` / `head_of_chain` along supersedes
- [x] Binding inheritance (§5.3): propagate along `member_of` / `groups` / `requires` (Decomposition → children; Milestone → groups, and requires invariants)
- [x] Active milestone (§5.4 step 5): latest `ActivationEvent` among bound milestones
- [x] Aggregations: bound_constraints (active milestone requires/excludes + bound objective constrained_by), bound_acceptance_criteria (validated_by), open_change_requests (no accept/withdraw), outstanding_questions (no answers)
- [x] Stable serialization + digest: sets sorted by NodeId; fields in fixed order; SHA-256 lower-case hex (I10 determinism)
- [x] `snapshot_payload` helper: derive → pack into `ContextSnapshotPayload`; digest and content are in lockstep
- [x] 11 tests (binding/accept/withdraw/supersedes head/inheritance/active milestone/constraints/change request/questions/digest determinism/snapshot validates via store)

#### Node factory layer (workflow/graph-db, build steps 8–12)
- [x] Provenance-grouped factories (N6): `as_human` / `as_llm` / `as_deterministic` three creation entry points; `GraphStore::append_node` becomes crate-private primitive, callers cannot set/forge provenance
- [x] HumanFactory: objective / constraint / acceptance_criterion / milestone / change_request / acceptance_event / withdrawal_event / activation_event / answer (Clarification kind=Answer)
- [x] LlmFactory: objective / constraint / acceptance_criterion / decomposition (step 8 small nodes) / milestone / assessment / first_slice / question (kind=Question) / decision / pseudocode / code / raw_llm (forced Failed)
- [x] DeterministicFactory: context_snapshot / baseline_decision / diagnostic (step 12, 5 diagnostic kinds) / selection / materialize
- [x] Each entrance fixes provenance and, if needed, creation_status / Clarification kind; compile-time seals forging paths
- [x] 6 factory tests (provenance fixed per path; question/answer kind; raw_llm Failed; baseline decision) + provenance×role matrix unit tests

#### LLM abstraction and structured output (workflow/llm, build step 13)
- [x] `LlmClient` trait: backend-agnostic, only needs free-text `complete` (single path); `CompletionRequest` (model/system/prompt, `with_repair_hint`) / `CompletionResponse`
- [x] `complete_structured`: JSON extraction (tolerate pre/post prose) + `jsonschema` strict validation (`additionalProperties:false`) + retry fallback (carry error messages and retry; exceed attempts → structured error; do not fake success). Backend unreachable → report immediately w/o retries
- [x] `LlmError`: variants aligned with `RawLlmFailureKind` (4.4.8); failures bubble up as RawLlmNode emitted by upper layers
- [x] Concrete backend `HttpLlmClient` (reqwest): two modes `BackendMode::{OpenAiCompatible, Ollama}`, shared system+user messages shape; OpenAI uses `/chat/completions`, Ollama `/api/chat` (`stream:false`); non-2xx/network errors → `BackendUnavailable` (never fake success)
- [x] 13 tests (6 for structured fallback + 7 backend: endpoint/message construction/response parsing)

#### Prompt template management (workflow/prompt, build step 13)
- [x] `PromptRegistry` (minijinja): embed 6 templates (design_solution / implement_design / repair_code / revise_design / decision / decompose), strict undefined (missing vars error)
- [x] 6 JSON Schemas (design_result / implement_result / decision_node / pseudo_check / repair_result / decompose_result), `schema_for` to fetch; all `additionalProperties:false` (one schema per workflow step; `pseudo_check` schema ready; checker command pending)
- [x] 11 tests + 4 insta render snapshots (guard templates / baseline changes cannot silently affect LLM behavior)

#### Decomposition protocol (workflow/graph-db, build step 11)
- [x] `AssessmentLlmOutput` / `AssessmentSelfCheck` (4.2.2): strict LLM-output contracts with `#[serde(flatten)]` head; `deny_unknown_fields`
- [x] `decompose_assessment`: deterministic helper to split LLM output into nodes+edges — Assessment (`assesses→` target), optional FirstSlice (`proposes→`), 0..N Constraint(Invariant) (`proposes→`, forced kind=Invariant), Decision (`proposes→`, change-kind state assessment); each LLM node has `consumed→ ContextSnapshot` (I6)
- [x] Require self-check all-true to decompose (else reject, treated as invalid assessment)
- [x] 7 tests (minimal/full decomposition / self-check fail rejection / non-Invariant rejection / Decision shape / strict schema rejects extra fields / flatten parsing)

#### LLM invocation orchestration (workflow/engine, build step 13)
- [x] `run_llm_step`: codifies §7 entry points — (1) before calling LLM, deterministically build `ContextSnapshot` from active context; (2) call `complete_structured`; (3) on success return value + snapshot (caller emits downstream nodes with `consumed→`), on failure emit `RawLlmNode` (`attempted→ target`, failure_kind from `LlmError`)
- [x] On failure still build snapshot first (auditability/reproducibility); never fake success
- [x] New `workflow/engine` crate (depends on graph-db + llm + prompt); layering: persistence layer does not depend back on LLM
- [x] 4 tests (success builds snapshot / backend-unavailable fallback + attempted edge / schema failure fallback / snapshot built even on failure)

#### Workflow execution loop, implement-loop, and Selection/Materialize orchestration (workflow/engine, build step 13+ / step 14 companion)
- [x] design/implement/repair steps (`loop_steps`): on top of `run_llm_step`, chain LLM calls into graph artifacts — `design_solution` builds `PseudocodeNode` (`addresses→` the target domain); `implement_design` builds `CodeNode` (`addresses→` target + `implements→ Pseudocode`); `repair_code` builds new `CodeNode` (`addresses→` target + `repairs→` old Code)
- [x] Artifact bodies return with outcomes (`PseudocodeArtifact.text` / `CodeArtifact.files`): graph nodes do not store bodies (4.4.3/4.4.4), but downstream gates and materialization need them, so types carry text/files to the caller (who persists/feeds gates)
- [x] Execution side of “separate choose/execute”: `consumed→ ContextSnapshot` is built by `run_llm_step` for each LLM node (I6); any step failure returns `LoopStepOutcome::Failed` (RawLlmNode already emitted + `attempted→` target), no faking — caller decides next actions
- [x] Pre-validate graph structure: `addresses→` targets limited to Objective/Milestone/FirstSlice; implement source must be Pseudocode; repair predecessor must be Code; CodeNode stores only file paths; bodies are handled by upper layers (4.4.3/4.4.4)
- [x] implement-loop (`implement_loop`, corresponds to CLI `sophia graph implement-loop`, architecture §9.2): budgeted implement → deterministic `code_check` injection (`CodeChecker`, kind must be CodeCheck) → emit `DiagnosticNode` (`checks→ Code`) → ok returns passing candidate; else within `max_repair_attempts` (design §10.9) re-render `repair_code` with diagnostics and try again; on budget exhaust, return `BudgetExhausted` (keep last candidate + diagnostic node)
- [x] Layering: engine does not run checker itself (belongs to tools); check results are injected by caller — same shape as materialize consuming `GateReport`. LLM failures still go via RawLlmNode fallback
- [x] Selection/Materialize orchestration (`select_materialize`): consume `tools/materialize`’s `CodeCandidate<Selected>` (types guarantee all gates passed), create `SelectionNode` (`selects→ Code`) → atomic write → `MaterializeNode` (`materializes→ Selection`, payload records logical root + relative file list; no machine-dependent absolute paths to preserve determinism)
- [x] Layering: engine depends on graph-db + llm + prompt + materialize; atomic writes still implemented in materialize crate; graph nodes are created by orchestration after gates pass
- [x] 17 tests (design→implement→repair loop + I6 guard / design failure fallback missing Pseudocode / implement rejects non-Pseudocode source / design rejects non-target domain; implement-loop one-shot pass / repair pass / budget exhaust / propagate implement failure / reject wrong diagnostic kind; Selection/Materialize full flow + edges + writes / reject non-Code target / orchestration keeps I6 / multi-file materialize)

#### Workflow spine scheduler (workflow/engine, build step 13+)
- [x] `run_goal_loop` (scheduler): per-round LLM decision (emit DecisionNode + `considers→ focus` + `consumed→ snapshot`), then dispatch by `selected_action` (design §10.8 “action choice must be LLM-produced”)
- [x] Execution delegation: `design_solution` builds current Pseudocode version; `implement_design` runs `run_implement_loop` (implement→check→repair); on pass, return `CandidateReady` to caller for select/materialize
- [x] Budget enforcement (design §10.9 starter subset): `max_decisions` (~=max_depth), `max_pseudocode_versions`, `max_total_llm_nodes` (LLM subset of max_total_nodes_per_goal). Budget-exhausted → `BudgetExhausted`
- [x] Materialization is explicit: do not auto-run irreversible writes in scheduler (design §10.10 “domains/ is the only write path”); passing candidates are returned via `CandidateReady` to caller
- [x] High-level actions yield: `decompose`/`backtrack`/`revise_design`/`needs_clarification` exceed the spine’s scope; spine yields and does not invent semantics (single path). `revise_design`/`needs_clarification` are wired; `decompose`/`backtrack` are handled by the separate goal-tree traversal layer (`engine::run_goal_tree`, build step 13+, architecture §8.5)

#### Goal-tree traversal layer (workflow/engine `traversal`, build step 13+)
- [x] `run_goal_tree`: above the spine, drive non-linear goal trees — `Decompose` in spine → `decompose_goal` (LLM structure → deterministic `graph-db::build_decomposition`) then recurse DFS into each child; `Backtrack` → record abandonment (`GoalResolution::Backtracked`)
- [x] Decomposition reviewer `DecompositionReviewer` (human authorization checkpoint, design §5.3 / N4): after decompose landed and before recursion, call reviewer — Accept → create real human `AcceptanceEvent accepts→ Decomposition`, children inherit binding via `member_of` and enter their active contexts, then recurse; Reject → do not recurse; do not fake withdrawal (`GoalResolution::DecompositionRejected`). Provide `AutoAcceptReviewer` (caller represents human authorization; still lands real AcceptanceEvent; not bypassing binding predicate). Engine does not fake human authorization; caller holds the authority
- [x] `graph-db::build_decomposition` (deterministic helper): builds `Decomposition`; connect `parent decomposes→ Decomposition`; for each child, build `Objective` and `member_of→ Decomposition`; reject non-Objective parent; reject <2 children
- [x] Honesty hard constraints: `Decomposition` is the LLM execution-product node (holds the LLM-produced structure) and thus has its own `consumed→ ContextSnapshot` (I6, same as Pseudocode/Code/Assessment), anchored on the call that produced it, not the DecisionNode that triggered it (“should decompose” vs “how to decompose” are separate calls, §10.8). `build_decomposition` accepts and validates the snapshot. Child `Objective`s are structural derivations, indirectly anchored via `member_of` (no separate `consumed→`). `backtrack` does not fake `WithdrawalEvent`; binding is not faked (accept/reject are human authority N4; LLM-derived children are unbound by default; binding is inherited after human accepts the Decomposition). See language_design.md §10.9, engineering_architecture.md §8.5, workflow_graph_spec.md §I6/4.1.4/6.1
- [x] `TreeBudget`: `max_depth` (decompose nesting depth) + `max_goals` (total spine calls) to prevent explosion; each goal’s spine still constrained by `SchedulerBudget`
- [x] 6 graph-db decomposition tests (nodes + consumed→ snapshot / edges / reject invalid parent / reject invalid snapshot anchor / reject too few children / binding inheritance after accept) + 5 engine traversal tests (each child resolves to candidate with I6 / leaf resolves directly / backtrack abandons without fake withdrawal / depth cap / total goals cap)
- [x] Audit fix: `decision_node.json` now `oneOf` three state_assessment kinds each with its own required full fields (previously only `kind` was required; schema-passes-but-deser-fails; strict mode 1.3 = schema is a faithful contract)
- [x] Layering: deterministic `code_check` injected by caller (`CodeChecker`); scheduler does not run checker; prompt context extraction belongs in CLI coordination; requests and schemas are injected (`StepRequests`)
- [x] 7 tests (design→implement produces candidate + considers-edge + I6 / no-pseudocode implement yields / high-level actions yield / decision rounds budget / pseudocode versions budget / decision backend failure fallback / reject illegal focus)

#### Materialize Gate (tools/materialize, build step 14)
- [x] Type-state chain (impl §15): `CodeCandidate<S>` with states `Unchecked → CheckPassed → AuditPassed → RuntimeValidated → Selected`; `materialize` exists only on `Selected`
- [x] Gate conditions (design §10.10): code_check → constraint_audit → artifact_diff (strip-assist equivalence) + runtime input/output validation → select; each gate consumes deterministic `GateReport` (do not reimplement checks)
- [x] Compile-time gate guarantees: `compile_fail` doc tests prove skipping gates cannot compile
- [x] Atomic writes: write to hidden staging first, rename to replace target on success; cleanup on failure; reject absolute paths/`..` escapes
- [x] 9 integration + 2 doc tests (full flow; each gate failure halts; multi-file; path-escape rejection)
- [x] Layering: does not depend on workflow graph (MaterializeNode created by orchestration after gate pass)

#### Language Server (lsp, build step 15)
- [x] Protocol-agnostic analysis core `Workspace` (based on semantic data, §10.3): multi-document parse → ASG index + symbol tables (module/symbol caches); query-style APIs leave room for incremental analysis
- [x] Diagnostics: merge syntax + hir + semantic; attribute precisely to documents — HIR uses `resolve_item`; semantic adds `analyze_one_callable` per item/callable to avoid cross-document 0-based span collisions
- [x] hover: show kind and definition of symbol under cursor (from symbol tables)
- [x] goto definition: resolve symbol under cursor to its top-level symbol definition (cross-document supported)
- [x] span ↔ LSP position conversion: byte offsets ↔ 0-based lines + UTF-16 columns (handle multi-byte correctly)
- [x] tower-lsp shell: didOpen/didChange(FULL)/didClose + publishDiagnostics + hover + definition; `initialize` declares capabilities; `run_stdio` entry
- [x] 9 analysis integration tests (incl. cross-document attribution, cross-doc goto) + 3 position-conversion unit tests

#### Deterministic checker (tools/check)
- [x] `check_program`: assemble HIR name resolution + three-layer semantics + strip-assist equivalence gate, return structured `CheckReport`
- [x] Strip-assist equivalence gate (design 5.1): `Ast::strip_assists` removes all Semantic Assists (meaning/not/... and entity’s semantic_identity/evolution), then compares pre/post-removal Semantic IR fingerprints (declaration model `formal_fingerprint` + semantic diagnostics). Mismatch reports the first differing line
- [x] `SemanticModel::formal_fingerprint` (deterministic `Debug`, no spans, no assists) as the formal core fingerprint
- [x] Wired into CLI `sophia check` (sophia.toml `require_strip_assist_equivalence`)
- [x] 7 tests (clean pass / rich-assist equivalence / state value assist / semantic diagnostics / HIR diagnostics / diff pinpoint)

#### Constraint audit (tools/audit)
- [x] `audit_constraints`: audit a set of constraints and produce structured `AuditReport` (aligned to Diagnostic kind=ConstraintAudit/RegressionGate, workflow_graph_spec 4.4.5)
- [x] Regression gate rules (4.1.2 / §7.4): only `Invariant` + executable verifier (HiddenCase/AuditRule) drive the gate (Pass/Fail decided by injected `VerifierOutcome`); Manual/no verifier/non-Invariant → Skipped (context only); declaring an executable verifier but missing its outcome → hard error
- [x] Layering: tools layer does not depend on the workflow graph or the runner; verifier execution results are injected by a deterministic pipeline (isomorphic to how materialize consumes `GateReport`)
- [x] 7 tests (invariant pass/fail, non-invariant skipped, manual skipped, missing verifier skipped, missing outcome hard error, mixed constraints only report invariant failures)

#### Top-level `effect` declaration (core, language design §13 / impl §20)
- [x] `effect` top-level construct: `effect Family { operation Op { param... } }`; grammar `effect_def` + `effect_operation` + `effect_param`; AST `EffectDef`; lowering
- [x] General effect reference: `effect_ref` (`Family.Op` / `Family.Op(args)` / `Pure`) replaces hardcoded variants; AST `Effect::{Pure, Op{family,op,args}}` + `EffectArg` (single-path, remove old 4-variant form)
- [x] HIR effect symbol table: `AsgIndex::effect_ops` (`Family.Op → arity`), built-in families `Console/DB` predeclared by `builtins::BUILTIN_EFFECT_OPS` + user-declared `effect`s merged; name resolution validates declared + arity (`UnresolvedEffect`); `NodeKind::Effect` enters index
- [x] Semantic triple representation: `Effect=(family,op,args)`, `EffectArg::{Lit,Binding}`; capability matching via `covered_by` — literals must equal; binding names are wildcard (preserve `DB.Read("A")≠DB.Read("B")`)
- [x] Tests: syntax/lowering (effect decl / strip-assist) + HIR effect resolution (incl. user domain effects) + semantic effect/capability checks; CST snapshots use `effect_ref`

#### Scheduler high-level actions and goal-tree traversal (workflow/engine, build step 13+)
- [x] Prompt rendering at call time for the scheduler (`StepPrompts` provider replaces static `StepRequests`): `run_llm_step` accepts a prompt-rendering closure (rendered with the same active context as the ContextSnapshot, §10.7 same-source). `design_solution`/`implement_design`/`repair_code` accept `FnOnce(&ActiveContext) -> CompletionRequest`; `run_implement_loop`/`run_goal_loop` accept `&impl StepPrompts` (`prompts` defines the trait + `GoalProgress`). Static `StepRequests` removed entirely (single path). Design in engineering_architecture §8.4
- [x] `revise_design` / `needs_clarification` wired into the spine: when implement-loop fails within budget we return to decision (`Dispatch::ImplementExhausted` + `GoalProgress.last_implement_failed`), LLM may choose `revise_design` to rewrite pseudocode (new Pseudocode + `revises→` old, making revise reachable, design §10.8 principle 3); `needs_clarification` truly emits `Clarification(Question)` + `asks_about→ focus`, then yields
- [x] Goal-tree traversal `run_goal_tree` (`traversal`, handles decompose/backtrack per design §10.9, not inside the spine, architecture §8.5): spine yields `Decompose` → run `decompose_goal` (LLM structure → deterministic `build_decomposition`) then recurse DFS for each child; yields `Backtrack` → record abandonment (`GoalResolution::Backtracked`). `TreeBudget` (max_depth / max_goals) prevents explosion
- [x] `graph-db::build_decomposition` (deterministic helper): build `Decomposition`; connect `parent decomposes→ Decomposition`; for each child build `Objective` + `member_of→ Decomposition`; reject non-Objective parent; reject invalid snapshot anchor; reject <2 children. Honesty: `Decomposition` is an LLM execution-product node with its own `consumed→ ContextSnapshot` (I6), anchored on the call that produced the structure (decision vs decompose are separate calls per §10.8). Children are structurally derived, indirectly anchored via `member_of`; `backtrack` does not fake `WithdrawalEvent`; binding is not faked (human authority N4; children are unbound until human accepts the Decomposition in §5.3). Tightened `decision_node.json` schema to `oneOf` three kinds each with full required fields (strict mode faithful contract 1.3)
- [x] 9 scheduler tests + 6 graph-db decomposition + 5 engine traversal tests

#### Ranked multi-candidate selection (tools/materialize + workflow/engine, design §10.9 score)
- [x] `score` module: `score_candidate` / `rank_candidates` / `Score` / `ScoreInputs` / `ScoreWeights` — seven weighted dimensions (compile/tests/constraints from gate reports; simplicity/locality/capability_minimality measured from source by formulas; pseudocode_clarity only if caller provides, else neutral 0.5; never faked). Hard constraint `compile=0 → overall≤0.49`; deterministic tie-break by ascending index
- [x] Engine `run_ranked_selection`: rank candidates, pick winner, create `SelectionNode` (rationale records score summary). Scores live only in memory and do not enter the graph (spec has no Score role). 7 score unit tests + 2 engine ranked tests

#### Execution Trace projection (core/exec-ir + runtime, impl §9.4)
- [x] Stable `ExecEdgeId(u32)`: introduce stable IDs for exec-graph edges in `core/exec-ir` (`add_edge` returns ID; `call_edge_id` / `edge` queries) — prerequisite for trace projection
- [x] `runtime/trace`: `Trace` / `ExecutionSpan` / `SpanOutcome`; interpreter opens a span upon entering each callable (pre-order) and writes back the outcome; `run_action` returns `Execution { outcome, host, trace }`. Span carries `node_id` / triggering `edge_id` (top-level None) / `depth` / `outcome`. Determinism first: no wall-clock durations; only graph projection and entry sequence `seq`. LLM metering (tokens/cost) awaits LLM execution nodes
- [x] CLI `sophia run --trace`: render projection (indent by depth + node/edge IDs + outcome). 4 trace tests + 1 CLI `--trace` integration test

#### Constraint audit verifier execution + hidden-case storage (runtime + tools/audit + cli, spec V.A)
- [x] Hidden-case executor (`runtime/verify`): `HiddenCase` / `ExpectedOutcome` (Returns / Raises) / `run_hidden_case` / `run_hidden_cases` — truly execute hidden cases on the v0 interpreter and compare against expectations, produce `VerificationResult` (passed + detail). Execution hard errors count as fail; never faked
- [x] Hidden-case storage (CLI `verifier_store`): hidden case bodies live outside the graph at `sophia-runs/verifiers/hidden.json` (`ref → HiddenCase`), physically isolated from the dev_graph and absolutely excluded from the active context. Triple anti-cheat isolation: (1) graph nodes only store opaque `verifier.ref`; (2) `ConstraintView` strips the verifier entirely (spec 5.6); (3) bodies outside the graph. `runtime::{Value, HiddenCase, ExpectedOutcome}` get serde (single value model; no mirror)
- [x] Gate auto-drive: `run_constraint_audit` reads `verifier.ref` from the raw `ConstraintNode` payload (not `ConstraintView`), builds models on the candidate and calls `runtime::run_hidden_cases`, then injects `VerifierOutcome` into `audit_constraints` with zero-loss mapping; missing cases → do not inject → audit yields `MissingVerifierOutcome` hard error. Layering conserved: execution in runtime, adjudication in tools/audit, loading + wiring + graph writes in CLI. 6 runtime verify tests + 4 CLI integration tests

#### CLI convenience commands (cli, architecture §9.1, deterministic, no LLM)
- [x] `sophia smoke`: one-shot chain init (idempotent) → check → build → run (`--action <Name>` optional; omitted runs only check/build). Any failure aborts with nonzero exit (honest; never faked)
- [x] `sophia repair-context --error <code>`: build structured context for LLM repair loop (impl §14.3). Filter `check` diagnostics by code substring and, for each match, report owning file + 1-based position + code + message + action-rooted semantic closure (related nodes/files). Does not invent repair suggestions (edits are LLM’s job). `commands::collect_diagnostics` factored and shared by `check`/`repair-context`. 5 CLI integration tests

#### Snapshot testing infrastructure
- [x] Already used for CST / prompt rendering / prompt assets; extended to HIR (ASG index JSON), Semantic IR (`formal_fingerprint`), and Execution Graph IR (node + call-edge structure). Each adds 1 insta snapshot to guard core IR artifacts against silent drift

#### Body-level storage operations (core/semantic + runtime, §16.6 storage extension subset)
- [x] Type layer (`type_layer::infer_storage_op`): recognize `storage.<Name>.get(key)` / `.save(key, value)` — `get → Optional<ValueTy>`, `save → ValueTy` (v0 excludes `Result<T,E>`, so `save` returns the value directly); merge effects `DB.Read("<Name>")` / `DB.Write("<Name>")` (same `used ⊆ declared` + capability checks as declarative effects); validate key/value arg types against the storage declaration; unknown storage/op produce diagnostics
- [x] Runtime (`interp::try_storage_op` + `effect_host`): interpreter recognizes the same shapes and delegates via `EffectHost` (`storage_get` / `storage_save`); default `InMemoryHost` uses a per-storage bucketed in-memory key→value map; keys are explicitly passed ("everything explicit" principle; do not infer by entity field name convention)
- [x] 7 tests (runtime: save→get roundtrip / missing key returns None / overwrite same key; semantic: valid storage op passes / missing DB effect decl errors / key type mismatch errors / unknown storage errors) + CLI e2e smoke (check passes + `run --trace` returns 42)

### 2.2 Partially Completed

- [ ] `context_files` in `graph design`: currently `graph design <ObjectiveId>` operates on the Development Graph, while `context` closure is computed from the project’s source action/task root — the two roots differ. The link from a graph Objective to a project action is not yet modeled, therefore `context_files` for design remains honestly empty (no invention)

### 2.3 Not Completed

#### core
- [ ] Execution Graph IR scheduling extensions (step 4+): concurrency / await / retry / cancellation / checkpoint, and richer edge semantics (starter subset only builds callable execution nodes + Control call edges; Data/Stream/Conditional/Fallback are vocabulary placeholders without surface sources, thus not materialized)

#### runtime
- [ ] Integrate Tokio substrate (when real async effects such as network/files are introduced)

#### workflow / CLI
- [ ] Fill in remaining `graph` workflow subcommands: `decision` / `assess` etc. (`init` / `start` / `context` / `nodes` / `design` / `implement-loop` / `select` / `materialize` are done — end-to-end `start → design → implement-loop → select → materialize` works)

#### lsp
- [ ] LSP extensions (step 15+): rename / autocomplete / semantic navigation; incremental analysis (Salsa-ification)

#### engineering
- [ ] CI pipeline integration (auto run fmt / clippy / test; local commands already ready, see §III)

### 2.4 Planned (Roadmap, after starter subset)

- [ ] v1: WASM codegen (entity/state/error → type section + metadata; action → wasm function; effect → host import)
- [ ] v1: strip-assist WASM artifact byte-for-byte equivalence
- [ ] Incremental analysis: query-caching à la Salsa (APIs already query-style)
- [ ] MessagePack serialization: graph snapshots / runtime state / semantic cache
- [ ] Formatter: AST/HIR → pretty printer, deterministic output
- [ ] Entropy/evolution checks for Semantic Identity / Evolution Boundary
- [ ] Extension subsets such as transition contract proofing / `Result<T,E>` / cross-domain boundary / `entity.with`
- [ ] v2+: optional backends (native cranelift/LLVM; targeted named-language emits on demand)

---

## III. How to Verify

- Build: `cargo build --workspace`
- Test: `cargo test --workspace`
- Lint: `cargo clippy --workspace --all-targets`
- Format: `cargo fmt --all -- --check`
- Syntax manual check: `cargo run -p sophia-cli -- parse <file.sophia>`

---

## IV. Change Log

- 2026-05-29 — Initialize this progress document; record workspace/syntax completions and placeholders
- 2026-05-29 — Complete build step 1: AST data model (Arena + `ExprId`) and CST → AST lowering; add `parse_ast` / `SyntaxTree::to_ast` entries and 13 lowering integration tests. Mark syntax as Completed
- 2026-05-29 — Review fix: `first_named_child` previously returned comment (trivia) nodes, violating the “drop trivia in CST → AST” rule (impl §4). Fixed and added regression covering comments inside expressions
- 2026-05-29 — Complete build step 2: HIR name/module/scope resolution. Add `AsgIndex` (incl. variant member table), builtins table, resilient diagnostics, `resolve_program` entry, and 15 integrations. Mark HIR as Completed
- 2026-05-29 — Review fix: HIR `Resolver.diags` doc comment wrongly said “task include whitelist”; corrected to “resiliently collected diagnostics”
- 2026-05-29 — Complete build step 3: Semantic IR (type/effect/contract). Add normalized `Ty`, effect algebra, `SemanticModel` declaration view, `TypeTable`, `analyze_program` entry, and 18 integrations (incl. canonical TodoDomain). Mark Semantic as Completed
- 2026-05-29 — Review fixes (Semantic IR): (1) In `ensures`, `output` should be a record with output parameters as fields (`output.<param>.<field>` per design §5), previous single-output type caused `NoSuchField`; added `Ty::Record`. (2) Output `where` predicate scope missed the output param itself; fixed. (3) `set` previously skipped assignment type compatibility; added `check_assignable`. Added regressions for all
- 2026-05-29 — Complete build steps 4–5: Execution Graph IR (`ExecGraph`) and Interpreter. Add runtime value model, `EffectHost` (default `InMemoryHost`), `Interpreter` (full body subset, cross-file calls, transition constructor calls), runtime input/output validation. 14 tests (13 interpreter + 1 exec-ir). Mark exec-ir/runtime Completed (starter scope)
- 2026-05-29 — Review fix (runtime): domain errors raised by callees were surfaced as hard errors `RuntimeError::Raised` at the `run` boundary; per §7.5/§16.3 they should propagate as domain results. Now mapped to `Outcome::Raised` at `run` boundary; added regression
- 2026-05-29 — Complete build step 6: Development Graph persistence (workflow/graph-db). SQLite + event sourcing `GraphStore` (append-only; no update/delete), 20 payload schemas (strict), 27 edge kinds with `(from,to,type)` hard constraints, append-time invariants I2/I3/I4/I5/I8, and overall I6 validation. 16 tests (incl. replay roundtrip). Mark graph-db Completed
- 2026-05-29 — Review fix (graph-db): previously missing payload-level edge constraints (§6.1) — `answers` must be Answer→Question; `asks_about` must originate from Question; `requires`/`excludes` must target specific Constraint kinds. Added `validate_edge_payload` and 3 regressions
- 2026-05-29 — Complete build step 7: Active Context derivation. Add `ActiveContext` / `*View`, binding predicate and inheritance, active milestone, aggregates for constraints/acceptance/change-requests/questions, stable serialization + SHA-256 digest, and `snapshot_payload`. 11 tests. Mark Active Context Completed
- 2026-05-29 — Review fix (active context): binding inheritance was a single snapshot and missed transitive chains (bound Decomposition → member Milestone → groups Objective). Switched to fixed-point iteration (order-independent) and added transitive-inheritance regression
- 2026-05-29 — Complete build steps 8–12 (creation side): provenance-grouped factories enforce N6 — `as_human` / `as_llm` / `as_deterministic`; `append_node` made crate-private. Cover small nodes / goals / milestones / events / assessment family / DiagnosticNode. 6 factory tests. Steps 8/9/10/12 Completed; step 11 protocol waiting for LLM wiring (schema ready)
- 2026-05-29 — Step 13 (core): LLM abstraction and structured-output fallback (`LlmClient` / `complete_structured`, retries + jsonschema + never fake success), Prompt templates (minijinja 5 templates + 3 schemas + insta snapshots). 13 tests. Step 13 Partially Completed; concrete backend and orchestration (build snapshot / emit nodes) pending network/CLI
- 2026-05-29 — Complete build step 14: Materialize Gate type-state chain (`CodeCandidate<S>`: Unchecked → CheckPassed → AuditPassed → RuntimeValidated → Selected) and atomic writes (staging → rename). `compile_fail` doc-tests enforce “skipping gates doesn’t compile.” 9 integration + 2 doc tests. tools/materialize doesn’t depend on workflow graph. Step 14 Completed
- 2026-05-29 — Complete build step 15: Language Server. Protocol-agnostic `Workspace` (multi-doc → index + symbol tables), diagnostics attribution (semantic adds `analyze_one_callable`), hover, goto definition, span↔UTF-16 conversions, tower-lsp (`run_stdio`). 12 tests. Step 15 Completed; rename/autocomplete and incremental analysis as extensions. Steps 1–15 ready (step 11 protocol and step 13 orchestration pending wiring)
- 2026-05-29 — CLI deterministic commands wired end-to-end: `init` / `parse` / `index` / `graph` / `check` / `build` / `run`. Add `project`/`render`/`commands` modules; precise diagnostic attribution; run executes after check and prints return/raise. 6 CLI e2e tests. `asg_index.json` matches §17.2
- 2026-05-29 — Complete `tools/check`: `check_program` assembles HIR + semantics + strip-assist; add `Ast::strip_assists` and `SemanticModel::formal_fingerprint`; wire gate into `sophia check` via sophia.toml `require_strip_assist_equivalence`. 7 tests. tools/check Completed; audit pending
- 2026-05-29 — Complete `tools/audit`: `audit_constraints` + regression gate. Only Invariant + executable verifier drive the gate (via injected `VerifierOutcome`); others skipped; missing outcome is a hard error. tools layer does not depend on workflow graph. 7 tests. tools/audit Completed; verifier executor pending
- 2026-05-29 — Complete steps 11 and 13: (1) concrete LLM backend `HttpLlmClient` (OpenAI-compatible + Ollama, reqwest); (2) assessment decomposition `decompose_assessment` (AssessmentLlmOutput → nodes+edges, gated by self-check); (3) LLM orchestration `run_llm_step` (new workflow/engine crate: build snapshot → complete_structured → success value / RawLlmNode fallback + attempted edge). 18 tests. Steps 11/13 Completed
- 2026-05-29 — After audit (no deviations), complete the workflow execution loop and Selection/Materialize orchestration (workflow/engine): (1) `loop_steps` — `design_solution`/`implement_design`/`repair_code` build Pseudocode/Code with `addresses→`/`implements→`/`repairs→`, failures go via RawLlmNode, I6 guaranteed by snapshot edges; (2) `select_materialize` — consume type-state `CodeCandidate<Selected>`, create SelectionNode → atomic write → MaterializeNode; (3) add strict schemas `design_result`/`implement_result` (architecture §8.2 “one schema per step”). Engine now depends on sophia-materialize (tools layer; no cycle). +12 tests (4 loop + 4 selection/materialization covering pos/neg and I6 guard; previous 4 retained) + prompt schema tests. Workspace 189 passed / 0 failed; clippy 0 warnings; fmt clean. Remaining workflow item: spine scheduler (LLM-driven loop + budgets/scoring)
- 2026-05-29 — Review fix + add implement-loop. Previously, `loop_steps` parsed LLM artifact bodies then dropped them with `#[allow(dead_code)]`, making downstream gates/materialization lack bodies. Now artifacts return in outcomes (`PseudocodeArtifact.text` / `CodeArtifact.files`), nodes still don’t store bodies (per 4.4.3/4.4.4). Add `implement_loop`: budgeted implement→code_check→repair; check injected by caller (`CodeChecker`, kind=CodeCheck; isomorphic to materialize’s `GateReport`), each attempt emits `DiagnosticNode` `checks→ Code`; budget exhausted returns `BudgetExhausted`. +5 implement-loop tests and update loop_steps assertions. Workspace 194/0, clippy 0, fmt clean
- 2026-05-29 — Add workflow spine scheduler (`scheduler`): `run_goal_loop` drives goals by DecisionNode — per round do decision (emit DecisionNode + `considers→ focus`) → dispatch design/implement-loop; on pass, return `CandidateReady` for select/materialize. Budgets: max_decisions / max_pseudocode_versions / max_total_llm_nodes; high-level actions (decompose/backtrack/revise/clarification) yield; materialization is explicit (design §10.10). Tighten `decision_node.json` to `oneOf` with full required fields. Layering: code_check injected by caller; prompts/schemas via `StepRequests`; scheduler doesn’t run checker/extract context. +7 scheduler tests. Workspace 201/0, clippy 0, fmt clean. Remaining: high-level actions + ranking
- 2026-05-29 — CLI `graph_cmd` workflow subcommands (architecture §9.2): `sophia graph` split into subcommands (no subcommand = ASG summary, backward-compatible). Deterministic subcommands append to `sophia-runs/graph/dev_graph.sqlite` (event-sourced; persists across processes): `graph init`/`start`/`context`/`nodes`. LLM subcommands `graph design` (→ PseudocodeNode + `.pseudo` artifact) / `graph implement-loop` (implement→code_check→repair → candidates in artifacts, not materialized). Backend via `--model/--mode/--base-url/--api-key` builds `HttpLlmClient`; CLI uses one-shot tokio runtime to cross async. Add `code_check_files` to bridge to `tools/check`. Failures are not faked (backend-unavailable keeps RawLlmNode + nonzero exit). +6 CLI integration + 4 `code_check_files` unit tests. Workspace 211/0, clippy 0, fmt clean. Remaining CLI: `graph select`/`materialize`, `context`/`smoke`/`repair-context`
- 2026-05-29 — CLI adds `graph select` / `materialize`, closing the last mile of the workflow loop (`start → design → implement-loop → select → materialize`). Engine refactors `run_selection_materialize` into `run_selection` + `run_materialization` (two processes). Type-state proofs (`CodeCandidate<Selected>`) cannot be persisted across processes; thus both commands reload artifacts and rerun materialize gates (design §10.10): code_check (bridge tools/check) / constraint_audit (tools/audit; declared executable verifier without runner → hard error, honestly reflecting “to-be-wired”) / artifact_diff (strip-assist) / runtime validation (no hidden cases at starter stage → pass, not faked). Each gate emits `DiagnosticNode` `checks→ Code`; any failure blocks. Materialize writes atomically to `domains/`. +3 selection/materialize unit + 2 engine primitive tests. Workspace 216/0, clippy 0, fmt clean
- 2026-05-29 — Feasibility review shows stdlib “built-in node contracts” were blocked by language design (no `node`/`effect` top-level grammar, closed effect set), and out-of-scope for v0. Marked as blocked; do not invent grammar. Implement `sophia context --action/--task` instead (language design §8 Task Closure / Semantic Paging). Add `core/hir/closure`: `action_context`/`task_context`, deterministic outputs (nodes + explanatory edges + files), pure HIR. CLI `sophia context` added (`--sources` includes sources). 7 HIR closure tests + 2 CLI tests. Workspace 225/0, clippy 0, fmt clean
- 2026-05-29 — Deep technical-debt cleanup (behavior-preserving; 225/0 green). After systematic sweep by a context-gatherer subagent: (1) Deduplicate engine tests: consolidate `MockClient`, schema getters using `sophia_prompt::schema_for`, seed helpers, temp-dir helpers into `workflow/engine/tests/common/mod.rs` (remove ~200 LOC duplication; schemas no longer drift). (2) `graph_cmd` refactors: parse/expect helpers, prepare_selected_candidate, merge `report` helpers, split `code_check_files` into `syntax_diagnostics` + `semantic_diagnostics`. (3) Scheduler deconstructed: split budget gate + dispatchers. (4) Error handling: keep typed `ImplementLoopError` via `#[from]`. (5) Remove dead code/aliases. Keep designed assets: `revise_design` template / `pseudo_check` schema / `run_selection_materialize`. clippy 0; fmt clean; LOC 19527 → 19443
- 2026-05-29 — Complete stdlib prerequisite language design: `node` / `effect` top-level constructs (language design §13, upgraded from draft). `effect <Family> { operation <Op> { param... } }` generalizes the closed built-ins; normalized representation becomes `(family, op, args)`; reference syntax unified as `Family.Op(args)`. `node <Name> { input|inputs / output|outputs / effects / capability }` declares built-in node interface contracts (no body; provided by runtime; not in v0 interpreter). Document processing across layers and single-path migration (remove hardcoded effects in one go; current `DB.Read("Todos")` unchanged to users). Update engineering_architecture §4.1 and engineering_notes (Accepted). stdlib moves from “blocked by design” to “design ready, to implement.” Pure design; no code
- 2026-05-29 — Implement `node` / `effect` top-level constructs (language design §13), unblocking stdlib end-to-end. Syntax adds `effect_def`/`node_def`/`effect_operation`/`inputs_block`/`outputs_block`; effect references via `effect_ref` (`Family.Op(args)`/`Pure`); re-generate parser.c ABI 15; AST adds `Item::{Effect,Node}` + `EffectDef`/`NodeDef`/`EffectArg`; HIR adds `NodeKind::{Effect,Node}` + `AsgIndex::effect_ops` with built-ins (Console/DB/Llm/Tool/Stream) via `builtins::BUILTIN_EFFECT_OPS` (core is zero I/O so built-ins are Rust data); resolve validates declared + arity (`UnresolvedEffect`); semantic adds `Effect` triple and `covered_by`; adds node contract checks; stdlib provides 5 effects + 3 capabilities + 5 node `.sophia`, embedded via `include_str!`, with `load_contracts`/`check_contracts`. Single path: remove old effect enum variants; existing `DB.Read("Todos")` remains. 15 tests (4 syntax + 4 HIR + 4 semantic + 3 stdlib). Workspace 240/0, clippy 0, fmt clean. Docs: language_design §13 “Implemented” and corrected processing/migration; architecture §4.1 “Landed”
- 2026-05-29 — Fix v0 drift: Execution Graph IR was a dead artifact — only referenced in workspace manifest; `from_model` built nodes but not call edges; interpreter bypassed exec-ir by executing directly on AST+Semantic, violating §9.2 pipeline. Fix: (1) `ExecGraph::from_model` now scans callable bodies and materializes call edges (`Call` to action/transition, and constructor-like transition calls). (2) `runtime` depends on `sophia-exec-ir`; `Interpreter` owns the graph and resolves calls via the graph. Cross-file action calls / transition constructor calls now route via call edges. +2 exec-ir tests (edge build / non-callable constructors). Single path: interpreter executes only via graph. Workspace 242/0, clippy 0, fmt clean
- 2026-05-29 — v0 consistency sweep: remove placeholders / fake paths / redundant APIs / doc drift (behavior-preserving; 247/0, clippy 0, fmt clean). Cleanup: (1) Remove dead `EffectHost::db_read/db_write` and `InMemoryHost.storage`; keep only real `console_write`. Remove `LlmError::NotImplemented` and its mapping. (2) Remove dead public APIs unused across workspace. (3) Unify edge creation via `add_edge`. (4) Doc-code drift: §16 starter subset now includes transition calls (constructor/direct) across the full pipeline; remove outdated `requires_runtime_check` promise; clarify subset boundaries (ensures/requires name resolution + Bool typing only; no proof obligations)
- 2026-05-29 — First real LLM end-to-end run passes (design → implement → check → interpreter run). Add `cli/examples/todo_llm_e2e.rs` harness with two tasks; backend NVIDIA OpenAI-compatible; model `deepseek-ai/deepseek-v4-flash` default; API key via env only; examples skip cleanly without keys. Observations: inject the Sophia syntax baseline explicitly in prompts; clarify pseudocode/file shapes; provide implement/repair JSON shapes. Run via `cargo run -p sophia-cli --example todo_llm_e2e` with optional `SOPHIA_LLM_TASK=todo`
- 2026-05-29 — e2e security review + stricter positioning (prevent answer leakage; require first-try success). Remove leaked answers from `SOPHIA_SYNTAX_PRIMER`; rewrite to generalized rules with neutral examples; scrub path/name leaks; set `SOPHIA_LLM_MAX_REPAIRS=0` and treat any repair as fail for baseline runs. Both tasks now pass first-try with genuine generalization. clippy -Dwarnings clean; grep ensures no keys/answers in scaffolding
- 2026-05-29 — Scaffolding stratification + real repair-loop test. Move syntax baseline out of design phase into implement/repair system prompts only; design sees only goals + acceptance (pure semantics). Add `cli/examples/repair_llm_e2e.rs` to prove repair convergence with real `tools/check` diagnostics (1-round convergence). Pseudocode template headers are not repair points; keep shared template snapshots stable
- 2026-05-29 — Decide and land “shared syntax baseline prompt asset” (LLM-facing natural language with neutral examples). Not stdlib (stdlib is formal `.sophia`, consumed by zero-I/O core; prompts belong to workflow/LLM). Add `prompt/assets/sophia_syntax_baseline.md` and `preamble(name)` in prompt crate; wire CLI/examples to use it; add snapshot + anti-leak assertions
- 2026-05-29 — Shared syntax baseline asset completed. Unify consumers; delete per-example primers; remove misuse of graph Constraints for syntax baseline. Add prompt tests: snapshot of baseline; anti-leak tokens; unknown asset name returns None. Re-verify e2e: arithmetic/todo one-shot pass; repair converges in one
- 2026-05-29 — Systematize e2e design (`docs/e2e_test_design.md`): purpose/scope; anti-leak as first principle; capability groups G1–G4 and R; realism requirements; harness structure and registry; run/judge/report/CI relation. Plan: implement G1 + existing R-01 under unified harness
- 2026-05-29 — Implement e2e G1 (4 cases) + R-01 under unified harness `cli/examples/e2e/`. Delete old examples. Harness supports filtering, grouped summary, bounded retries (`RetryClient` only retries `BackendUnavailable`). Deepseek-flash: G1 4/4 first-try; R-01 converges in 1 repair. Feed two decisive rules back into the shared baseline (no semicolons at end of body statements; to_text/length directions). Snapshot/anti-leak updated. Workspace 250/0, clippy 0, fmt clean
- 2026-05-29 — Implement e2e G2 (effects+capability; 2 cases) + serial batch script. Harness `Case` adds `expected_console`; success checks return and console outputs. Both cases one-shot pass. Add `--list` and `scripts/run_e2e.sh`. Feed two more decisive baseline rules (Console.Write intent boundary; effect/capability directions). Workspace 250/0, clippy 0, fmt clean
- 2026-05-29 — Design (docs only): scheduler prompt rendering at call time (`StepPrompts` provider replaces static `StepRequests`) to fix §10.7/§10.8 issues. Next: implement engine changes and G3
- 2026-05-29 — Land call-time prompt rendering + e2e G3 (heuristic nodes). Engine accepts closures; steps take renderers; add `prompts` trait + `GoalProgress`; `run_implement_loop`/`run_goal_loop` take `&impl StepPrompts`; remove `StepRequests`; fix test schema bug (repair used implement schema). Harness gains `CaseKind::Scheduler`; G3-01 runs 2 autonomous decision rounds to a candidate; interpreter outputs 42. Workspace 250/0, clippy 0, fmt clean
- 2026-05-29 — Implement e2e G4 (complex programs; 2 cases). Harness `Expect` can assert Returns(Value) or Raises(Variant). G4-01: cross-action call via exec-graph; G4-02: error algebra and raise; both pass. Add two decisive baseline rules (cross-action calls; error algebra). Increase retries to 6. Workspace 250/0, clippy 0, fmt clean
- 2026-05-29 — v0 doc-code alignment + two advances (docs first): (1) Correct design §9.4 reference to nonexistent `ExecEdgeId` — mark trace “not implemented yet” pending stable edge IDs. (2) Partially land high-level scheduler actions: `revise_design` in-loop; `needs_clarification` emits Question. Add 2 tests; add insta snapshots for HIR/Semantic/Exec-IR core artifacts. Workspace 255/0, clippy 0, fmt clean
- 2026-05-29 — Implement ranked multi-candidate selection (design §10.9 score). Add `score` module with seven-dimension scoring and hard caps; engine `run_ranked_selection` builds SelectionNode with rationale. 7 score tests + 2 engine tests. Workspace 264/0, clippy 0, fmt clean
- 2026-05-30 — Advance Group B (CLI deterministic): add `sophia smoke` and `sophia repair-context --error <code>`. 5 CLI tests; workspace 269/0; clippy 0; fmt clean. Remaining blocked items called out (trace projection needed stable edge IDs; verifier execution needed runtime/harness integration)
- 2026-05-30 — Advance Group A: goal-tree traversal layer `engine::run_goal_tree` (spine yields decompose/backtrack; traversal executes decompose then recurses; records backtrack). Add `decompose` template + schema; extend `StepPrompts`. Export traversal APIs; enforce honesty (snapshot on Decomposition; no fake withdrawals/bindings). Add tests and docs. Workspace 280/0, clippy 0, fmt clean
- 2026-05-30 — Decompose refactor and I6 consistency: ensure `Decomposition` itself carries `consumed→ ContextSnapshot`; `build_decomposition` accepts and validates snapshot; include in `validate_i6`. Update specs/docs. Add tests. Workspace 281/0, clippy 0, fmt clean
- 2026-05-30 — Unblock two Group B items after prerequisites: (1) Trace projection onto exec-graph with stable `ExecEdgeId` and `runtime/trace`; (2) Constraint audit verifier executor with `runtime/verify` and CLI wiring into `audit_constraints`. Add tests; docs updated. Workspace 292/0, clippy 0, fmt clean
- 2026-05-30 — CLI `sophia run --trace` prints exec-graph trace projection; add flag and rendering; add CLI test; docs updated. Workspace 293/0, clippy 0, fmt clean. Data + executor + human-readable output complete; persistence/metering deferred pending LLM exec nodes
- 2026-05-30 — Draft design for hidden-case storage (docs only): triple isolation (opaque ref in graph; verifier stripped from `ConstraintView`; bodies in `sophia-runs/verifiers/hidden.json`). Gate wiring: execution in runtime, adjudication in tools/audit, wiring in CLI; missing data is a hard error; hidden.json provided by problem author. Specs updated. Implementation pending loader + auto-drive in gate
- 2026-05-30 — Implement hidden-case storage + gate auto-drive. `runtime::{Value,HiddenCase,ExpectedOutcome}` get serde; CLI `verifier_store` loads hidden.json; `run_constraint_audit` wires runtime execution into `audit_constraints`. Add tests; docs updated. Workspace 296/0, clippy 0, fmt clean. Constraint-audit verifier is now truly E2E-drivable
- 2026-05-30 — Implement body-level storage ops (§16.6). Type+runtime bridged; keys explicit; `save` returns value (no `Result`). Add tests; CLI e2e proves `save 42→get→42`. Docs updated. Workspace 303/0, clippy 0, fmt clean. This fills the only tangible feature gap in v0
- 2026-05-30 — Deep workspace cleanup (behavior-preserving; 303/0, clippy 0, fmt clean): remove dead code/re-exports; wire LSP into CLI (`sophia lsp`); reduce Result+expect churn; dedupe graph-db test helpers; fix stale CLI/core docs; confirm remaining exceptions are intentional. Initialize local git repo (main; no remote)
- 2026-05-30 — Phase A: E2E + verification gaps. Add e2e Group G5 (persistence/storage) after storage landed; introduce syntax-baseline decisive rules for storage; add CI invariant append-only tests in graph-db; move checklist engineering items “git init”/“CI invariants” to Completed. Workspace 305/0, clippy 0, fmt clean; e2e `--list` includes G5-01
- 2026-05-30 — Phase A2: wire real LLM e2e for goal-tree traversal + fix child binding chain. Add `DecompositionReviewer` (human authorization checkpoint); `AutoAcceptReviewer` for callers; make prompts focus-aware; add `CaseKind::Tree`; only allow decompose for tree cases without pseudocode at root. Add Group G6 (temperature panel decomposed to two child goals). Tests pass; real LLM run awaits API keys. Workspace 307/0, clippy 0, fmt clean
- 2026-05-30 — Built-in node interpretation (v0 node-execution subset). Clarify language has no surface “node graph wiring” syntax; only single-input/single-output node with exactly one non-Pure effect (Prompt/Tool/Stream node shapes) can run via EffectHost; multi-in/out or Pure structural nodes are honest runtime errors. Add `ExecNodeKind::Node`; runtime `run_node` with EffectHost dispatch; CLI unchanged. +1 exec-ir + 6 runtime tests; docs updated. Workspace 314/0, clippy 0, fmt clean
- 2026-05-30 — Remove agent orchestration / node construct entirely (course correction). Delete grammar/AST/HIR/semantic/exec-ir/runtime/stdlib pieces; keep top-level `effect` and generic `Family.Op(args)`. Update all docs. Workspace 298/0, clippy 0, fmt clean
- 2026-05-30 — Consistency cleanup (docs/code + dependencies): remove 6 unused workspace deps; keep exec-ir `EdgeKind` vocabulary; fix stale counts (14 member crates; 6 prompt assets); move unimplemented CLI subcommands to Roadmap; mark `pseudo_check` as “ready; checker command pending.” Workspace 298/0, clippy 0, fmt clean
- 2026-05-30 — e2e docs add §5.5 “artifacts in-memory; not on disk” clarifying harness behavior; no code changes
- 2026-05-30 — Add benchmark design draft `docs/benchmark_design.md` (compare Sophia workflow vs baseline Python/TS on success-rate + time; sophia mode reuses `runtime::verify`; baseline spawns external interpreter; design lists open decisions). Docs only
- 2026-05-30 — Implement benchmark example harness with L1–L4 problems; neutral JSON value contract; sophia mode with minimal loop and shared baseline; baseline_py mode with sandboxed `python3` runner and timeouts; reporting to `runs.jsonl` + `summary.md`. Workspace 298/0, clippy 0, fmt clean; real LLM run awaits keys
- 2026-05-30 — Benchmark first run + real LLM run + docs. Replace out-of-subset problems; add bounded-retry client; improve observability; add script; real run: baseline 6/6; sophia 4/6 (two honest fails due to language limits and naming drift). Manage secrets in `.secrets/`. Update README/INSTALL. Workspace 298/0, clippy 0, fmt clean
- 2026-05-30 — Root-cause and generalizable fixes for benchmark sophia failures: (1) unary negation missing — add grammar/AST/semantic/interpreter support; update syntax baseline; add regression; (2) naming fidelity — enforce in baseline prompt asset; update snapshots and anti-leak assertions. Re-run: sophia 6/6. Workspace 299/0, clippy 0, fmt clean
- 2026-05-30 — Clarify benchmark selection philosophy; add L5 `checkout_limit`; fix adjudication symmetry: for two-level errors, use most-specific variant name for Python exception and use the same identity in both modes. Re-run: both pass. Docs updated. Workspace 299/0, clippy 0, fmt clean
- 2026-05-30 — Enter v1: calibrate project goals and v1 tracks (docs only). Two parallel workstreams: A WASM codegen; B language/stdlib expansion (`Result<T,E>` / error handling / `task` / `entity.with` / cross-domain intent flow / proofs). Docs updated across language_design/engineering_architecture/language_implementation/benchmark_design/engineering_notes; overview updated to “v0 wrap-up / v1 start.” No code changes; workspace 299/0
