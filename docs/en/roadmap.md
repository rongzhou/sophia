# Sophia Roadmap

This is the current effective roadmap. Historical planning notes and paper drafts remain in `../archive/` as context only.

## 1. Core Thesis

Sophia is an LLM-native programming language and workflow for unattended automatic programming. It explores two claims:

1. Large-scale code pretraining is valuable but not absolutely required for programming competence.
2. A graph-shaped programming language and heuristic node workflow designed for LLMs is a viable programming substrate.

Sophia externalizes programming discipline into typed semantic artifacts, deterministic checks, capability/effect boundaries, intent types, action-rooted context, and versioned repair loops.

Sophia is not optimized for human handwriting, code review, or IDE habits. LLMs and humans have different strengths and failure modes: LLMs are strong at local semantic understanding, task decomposition, and explicit repeated structure, but weak at long-context memory, implicit convention tracking, ambient authority, and unattended constraint preservation. A language built for LLMs should not simply inherit human-first language design.

Scaffold, context, diagnostics, and gates reduce cognitive load and restrict dangerous freedom. They do not replace LLM pseudocode design, checkable source generation, or heuristic graph decisions.

New language features must prove value that ordinary languages, linters, tests, or prompt discipline cannot provide with enough machine-checkable force. A feature belongs in Sophia-Core only if it reduces LLM memory/guessing burden, creates explicit ASG edges, supports deterministic checks, or improves automatic repair/materialize gates.

## 2. Current v0.2 Boundary

v0.2 is a committed, testable prototype boundary, not the complete Sophia language.

Implemented language core:

- Top-level ASG nodes: `domain`, `entity`, `state`, `storage`, `error`, `capability`, `action`.
- Body statements: `let`, `let mutable`, `set`, `return`, `raise`, `if/else`, `match`, `repeat N times`, `print`.
- Types: `Unit`, `Bool`, `Int`, `Text`, `List<Int>`, `List<Text>`, `Optional<T>`, declared entity/state types, and intent wrappers.
- Expressions: literals, variables, field access, arithmetic, comparisons, Boolean operations, Text/List concatenation, `to_text(Int)`, `Some`, `None`, full entity construction, and direct action calls.
- Scope/control flow: block-scoped locals, no visible-variable shadowing, child blocks may update outer mutable variables, exhaustive `match` over Bool/state/Optional, and all-path `return` or `raise` for non-`Unit` actions.

Implemented checks and gates:

- File layout, top-level naming, duplicate declarations, and supported type checks.
- Intent wrapper assignability, explicit `intent_conversion: true` action contracts, Console boundary, and DB.Write storage value boundary.
- Effect, capability, error, raise, action-call, recursion, and block-scope checks.
- Strip-assist TypeScript artifact equivalence gate.
- Runtime input/output validation from generated metadata.
- Action-rooted semantic context with files, source payloads, nodes, edges, summary, and diagnostics.

Implemented workflow/tooling:

- `.pseudo` structure checks, outline, repair context, and LLM-facing scaffold.
- Implementation/repair prompts connected to action-rooted semantic context.
- Append-only graph workflow: design, implementation, repair, LLM decision, audit, diff, verify, select, materialize.
- TypeScript backend, `run`, `smoke`, hidden benchmark verifier, and benchmark suite runner.

## 3. Explicitly Out of Scope

These remain future language design and must not be described as v0.2 capabilities:

- `task` top-level syntax and `context --task`.
- `transition` top-level syntax.
- Error handling / error exhaustiveness.
- Body-level storage operations.
- `requires` / `ensures` proofs.
- `invariants`.
- `Result<T,E>` / `Ok` / `Err`.
- `entity.with`.
- Cross-domain library protocols.
- Cross-domain/library intent compatibility.
- Evolution Boundary enforcement.
- Independent Sophia IR backend and IR hash.

## 4. Near-Term Priorities

P0 is making v0.2 more defensible, not expanding syntax.

1. Generate reproducible benchmark reports.
2. Turn intent-safety checker fixtures into an adversarial benchmark suite: Raw to Sanitized, Secret to Redacted, DB.Write mismatch, Console boundary mismatch.
3. Convert new v0.2 success criteria into focused regression tests.
4. Keep syntax guide, language design, diagnostics, and tests synchronized for every language change.
5. Audit prompt inputs so implementation/repair receive deterministic closure/context payloads only.

## 5. Non-Toy Milestones

Sophia should not prove itself by chasing large benchmarks designed for existing language ecosystems. The stronger qualitative claim is that there should be code categories accepted by a traditional TypeScript pipeline but deterministically rejected by Sophia because they violate language-level discipline.

### S1: Intent Safety Tasks

Goal: show that intent types reject data-flow errors that TypeScript typechecking, ordinary linting, and non-adversarial tests may miss.

Candidate patterns:

- External Raw input written where `Sanitized<T>` is required.
- `Secret<T>` sent to Console without `Redacted<T>`.
- Authorization/validation conversion skipped across action boundaries.
- Storage value intent mismatch.

Stop condition: in a small benchmark suite, Sophia statically rejects unsafe candidates while a direct TypeScript baseline can typecheck and pass non-adversarial tests.

Current state: checker-level regression fixtures cover Raw/Secret conversion, Console boundary, DB.Write storage mismatch, capability deny, undeclared raise, and called-error propagation.

### S2: Edit Transitions and Semantic Drift

Goal: make the development graph more than an append-only synthesis log by representing edits as typed transitions between semantic artifacts.

Candidate work:

- Explicit edit nodes describing intended semantic changes.
- ASG summary and artifact comparison before/after edits.
- Responsibility drift detection across entity/action/capability boundaries.

Stop condition: repair/edit workflows can reject unauthorized semantic drift without human review.

### S3: Cross-Domain and Library Boundaries

Goal: leave single-domain toy projects.

Candidate work:

- Cross-domain imports through explicit ASG/library manifests.
- Capability/effect boundary checks across domains.
- Intent compatibility checks across library APIs.

Stop condition: in a multi-domain task, closure, checks, and codegen remain deterministic, finite, and reproducible.

## 6. Evaluation Principles

- Treat current benchmark results as pilot signals unless runs are repeated with fixed prompts and comparable baselines.
- Prefer sharp tasks that expose Sophia-specific guarantees over broad algorithm coverage.
- Report success, failure type, wall time, LLM calls, repair attempts, and deterministic gate failures.
- Distinguish language value from workflow hygiene. Redaction, anti-cheat rules, scaffold validation, and repair loops are useful engineering practices, but the language itself must provide machine-checkable guarantees that the traditional stack lacks.

## 7. Parking Lot

Potentially useful but not current priority:

- Dict and richer string operations.
- Broader L4/L5 algorithm benchmarks.
- Large multi-model matrices beyond minimum validity checks.
- Standalone `sophia strip-assist` CLI.
- Full runtime library packaging beyond the current TypeScript harness.
- Real external DB / network / filesystem effects.
