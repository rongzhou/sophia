# Sophia v1 Workflow A · WASM codegen Design Gate

![Sophia WASM codegen](images/wasm_codegen.png)

> Status: design review completed + implementation W1–W5 landed (A1–A5 achieved, 2026-05-31). This document defines the implementation plan and landing record of v1 Workflow A (WASM codegen): project Sophia’s semantics into deployable WASM artifacts so the execution backend expands from “Rust in-process interpreter only” to “embeddable by Node/Python/browsers/edge runtimes.” It corresponds to `dev_checklist_v1.md` Workflow A (A1–A6), `language_implementation.md` §12.2 (emit shape), and `engineering_architecture.md` §14.2. A1–A5 have landed (contract freeze / emit / differential tests / effect host imports / artifact gate + `sophia build`). Later work also landed registry-aware build, dynamic host imports, the ValueWire provider ABI, the non-browser WASM runner, the build bundle manifest, and direct execution of build artifacts via `sophia run/smoke --backend wasm`. A6 (incremental queries, decoupled from codegen) awaits its own design review.
>
> Three-step discipline (user-established methodology): this is the design review for codegen—first fix the input contracts / value ABI / function ABI / effect ABI / toolchain / diff-test and gate locations; after confirmation, implement in phases. §10’s seven decision points are confirmed and adopted; proceed per §9 W1→W5. This document contains no implementation code.
>
> Prime invariant (throughout): the interpreter is the only semantic source of truth (oracle). Any outputs of the WASM backend must be equivalent to the interpreter per hidden case (differential tests). Codegen must not demand IR/AST shape changes (`language_implementation.md` §12.1). Introducing a second semantic source of truth is forbidden.
>
> Production boundary: the supported direct-execution path is project-root build artifacts. `sophia build .` writes `sophia-runs/build/program.wasm`, `program.sophia-build.json`, and third-party host assets; `sophia run <Action> --root . --backend wasm` / `sophia smoke --root . --action <Action> --backend wasm` validate the manifest before executing. Arbitrary `.wasm` path execution and offline bundle loading without project sources/current registry are not supported; they are future directions in §XI.

---

## I. Goals and boundaries

### 1.1 Deliverables (v1 completion criteria 1 + 3)

- Criterion 1: starter-subset programs compile via the WASM backend and are equivalent to interpreter results per hidden case (all diff tests green).
- Criterion 3: strip-assist equivalence holds at the artifact layer—after removing all Semantic Assist fields, the emitted WASM byte sequence is identical byte-for-byte (extends the existing formal-core fingerprint gate; see `language_design.md` §5.1).

After landing, `sophia build` changes from a no-op to actually emitting WASM artifacts (`engineering_architecture.md` §9.1). Landed in W5.

### 1.2 Non-goals (boundaries)

- No async/concurrency/await/threads: the starter-subset body sublanguage is synchronous/pure (`language_implementation.md` §9.3), and WASM MVP’s async support is limited—one of the selection reasons for WASM; we will not make an exception here.
- No second codegen target: native (cranelift/LLVM) or named-language emission (TS/Python) is deferred to v2+ upon clear deployment needs (`engineering_architecture.md` §14.3/`language_implementation.md` §12 end).
- No parallel body IR: see §III’s input-contract decision—codegen, like the interpreter, consumes AST + semantic metadata for bodies; do not invent a lowered body IR layer (avoid a second truth source and overdesign).
- No heavyweight external toolchain in `cargo test`: the diff-test WASM executor must be a normal Cargo dependency, pure Rust, deterministic (see §VII toolchain decisions). Real deployment hosts (wasmtime/browsers) consume artifacts downstream; not part of gates.
- No LLM calls inside codegen: `core`/`tools` remain purely deterministic (iron rule, `language_implementation.md` §2).

---

## II. Current state (starting point for codegen)

Before landing codegen, pin down “what the interpreter actually executes”—that is the semantics WASM must replicate.

### 2.1 Execution path

```
parse(.sophia) → HIR(AsgIndex) → semantic(SemanticModel + 3 layers of checks) → exec-ir(ExecGraph) → interpreter run
```

- `SemanticModel` (`core/semantic/src/model.rs`): name-indexed declaration view—entity field types; state value sets; error-variant fields; capability allow/deny; callable signatures (inputs/outputs/effects/errors/capability/intent_conversion). This is the primary input contract for codegen.
- `ExecGraph` (`core/exec-ir`): execution graph at callable granularity—one node per action/transition; calls in bodies are Control edges. Body-level statements are not expanded into the graph (by design).
- Interpreter (`runtime/src/interp.rs`): consumes AST bodies + `SemanticModel`; directly evaluates runtime `Value`. Execution Graph is used to validate calls (node exists + materialized call edge) and for trace projection.

### 2.2 Runtime value model (to be replicated exactly by WASM)

`runtime::Value` (`runtime/src/value.rs`):

| Value | Contents | Notes |
| --- | --- | --- |
| `Unit` | — | |
| `Bool(bool)` | | |
| `Int(i64)` | 64-bit signed | |
| `Text(String)` | UTF-8 | `.length` = `chars().count()` (Unicode scalar count, not bytes) |
| `Null` | — | `one of`’s `Null` member |
| `List(Vec<Value>)` | homogeneous elements | `.append(item)` |
| `ErrorValue { variant, fields }` | a returned error member (failure member of `one of`) | different from `raise` |
| `Entity { name, fields }` | field-name → value (`BTreeMap` for stable order) | |
| `State { state, value }` | tagged union (state name + value name) | |

Key fact: intents are compile-time static and erased at runtime (`Raw<Text>`/`Sanitized<Text>`/`Text` are all `Text` at runtime). WASM value representation must carry no intent tags—same as the interpreter.

### 2.3 Body sublanguage (execution semantics to be replicated by WASM)

- Statements: `let`/`set`/`return`/`raise`/`if`-`else` (every `if` has an `else`)/`match` (exhaustive; `_` forbidden)/`repeat` (bounded loop)/`print` (Console.Write)/expression statements.
- Expressions: literals (Str/Int/Bool/Null)/`Ident`/`List`/`Field` (including pseudo-fields `Text.length` and `StateName.Value`)/`MethodCall` (`list.append`, library special roots `File.*`/`Http.*`)/`Call` (cross-callables + built-in `to_text`)/`Construct` (entity/error-variant/transition call)/`Not`/`Neg`/`Binary`.
- Binary ops (`eval_binary`): `And`/`Or` (short-circuit per interpreter)/`Eq`/`Ne`/`Lt`/`Le`/`Gt`/`Ge`/`Add` (Int+Int; Text+Text concat; List append)/`Sub`/`Mul`; unary `Neg`. No division or modulo.
- Outcomes: each callable produces `Outcome::Returned(Value)` or `Outcome::Raised(RaisedError)`; `raise` bubbles across call boundaries via the error channel and materializes there.
- Runtime validation: check arg arity and types before call (`check_value`); validate output type after return.

### 2.4 Effect delegation

Side effects are delegated through `HostRegistry`: standard-library native/mock hosts and third-party `host.wasm` providers are all registered as `(family, op) -> HostFn`. Console is a built-in host import; File/Http come from the current `LibraryRegistry`; third-party libraries are registered by discovery. In WASM, these are host imports (see §VI), and codegen consumes the registry instead of hardcoding specific library branches.

### 2.5 Strip-assist status

`tools/check/src/strip_assist.rs`: parse the same sources twice (original + strip-assist) and compare the Formal Core fingerprint (`formal_fingerprint`) + semantic 3-layer diagnostics. The artifact layer adds byte-level comparison here.

---

## III. Input contracts (A1: freeze; do not reshape IR)

A1 requires freezing and documenting v0’s `SemanticModel`/`ExecGraph` as codegen input contracts. F1 (type unification) and F2/S1/S2 + File have landed; IR shape is stable (includes `OneOf`/`Null`/`ErrorValue`/`File`/`Http` effects). Freeze now.

A1 landed in W1 (2026-05-31): the contract is codified as `tools/codegen`’s `CodegenInput` (`tools/codegen/src/contract.rs`)—a single read-only entry that bundles the three inputs below; internally `CodegenInput::new(model, asts)` constructs the exec graph via `ExecGraph::from_model` (same source as `Interpreter::new`, guaranteeing both backends see the same graph). The W1 contract-freeze test (`tools/codegen/tests/contract.rs`) guards “graph matches model callables + emit honest placeholders.”

Codegen consumes (without rewriting) the following three:

1. `SemanticModel` (declaration view)—entity/state/variant/capability/callable signatures. Codegen uses this to produce function signatures, type projections, and host descriptions for runtime validation.
2. `ExecGraph` (callable-granularity execution graph)—decides which WASM functions to generate (one per node), and call relationships (Control edges → WASM `call`).
3. AST bodies + `TypeTable` (statement-level)—codegen traverses the body AST like the interpreter to generate function-body instructions; it consults `TypeTable` (by `ExprId`) where static dispatch is needed (e.g., Int vs Text for `Add`).

### Decision ①: does the body need a new lowered IR?

Adopt: no new body IR; codegen traverses AST directly (same source as interpreter). Reasons: (i) starter-subset body is minimal (no closures/complex control); AST → WASM is a direct structural induction; (ii) a second body IR risks a dual-truth “interpreter reads AST; codegen reads new IR”; (iii) the interpreter is the executable spec for “AST → behavior,” and codegen need only project “AST → instructions” in parallel with diff tests ensuring equivalence. If the body sublanguage grows significantly in the future, revisit via a design review.

### Decision ②: how are values represented in WASM (value ABI)? See §IV.

---

## IV. Value ABI: representing Sophia `Value` in WASM linear memory

This is the core of codegen design. WASM MVP has only 4 numeric types (i32/i64/f32/f64) + linear memory; no GC or aggregates. Sophia `Value` is a tagged recursive structure (with String/List/Entity/nesting).

### Decision ③: value representation scheme

- Adopt: tagged heap values + i32 handles.
  - Allocate all Sophia values in a bump-only region of linear memory. Uniform representation: `[tag: i32][payload…]`; WASM stack/locals/params/returns all pass i32 handles (offsets into memory).
  - Tag enum aligns 1:1 with `Value` variants: `Unit=0, Bool=1, Int=2, Text=3, Null=4, List=5, ErrorValue=6, Entity=7, State=8`.
  - Payload layout (deterministic; ensures strip-assist byte stability): Bool `[tag][i32 0/1]`; Int `[tag][i64]`; Null/Unit `[tag]`; Text `[tag][len:i32][utf8 bytes…]`; List `[tag][len:i32][handle…]`; Entity `[tag][name_ptr:i32][nfields:i32][(key_ptr:i32, val_handle:i32)…]` (fields sorted by field name—same as `BTreeMap`); ErrorValue `[tag][variant_ptr:i32][nfields:i32][(key_ptr,val_handle)…]` (same order); State `[tag][state_ptr:i32][value_ptr:i32]`.
  - String/name literals go into the data section (constant pool), immutable at runtime; dynamic strings (Text+Text) allocate in the bump heap.
  - Memory management: bump-only; no reclamation. Values allocated within one `run` call are not freed; the region is reset after the call (starter subset has no long-lived allocations; `repeat` is bounded). No GC/refcount (YAGNI; consistent with no concurrency).
- Reject: WASM GC/reference types (tooling/host support uneven; YAGNI for starter subset; undermines coverage aim); reject JSON serialization for value passing (loses structure; drifts from interpreter semantics; slow). Only use a byte-level contract at host boundaries (see §VI).

Equivalence red line: `Text.length` must be Unicode scalar count (like `chars().count()`), not UTF-8 byte count—store bytes in memory but compute length by scalar counting (a prelude helper). `Int` is i64 throughout.

### Decision ④: how to encode error outcomes (raise vs returned ErrorValue)?

Failure members (`ErrorValue`) of `one of` are ordinary return values (tag=6 heap value; returned normally). `raise` is control-flow break that bubbles across calls; WASM MVP has no exceptions. Adopt: each callable returns an i32 handle and uses an Outcome wrapper to distinguish returned vs raised: signature `(params...) -> i32` returning a handle to an Outcome `[kind: i32 (0=Returned,1=Raised)][value_handle: i32]`. Callers test `kind`; if raised, propagate the same Outcome; if returned, take `value_handle`. This mirrors the interpreter’s `Outcome` and bubbling without needing WASM exceptions. At compile time, known-terminating `raise` paths emit “construct ErrorValue → wrap as Raised Outcome → return.”

---

## V. Function ABI and body instruction generation (A2: minimal emit)

### 5.1 Module structure

A `.sophia` program emits as one WASM module:

- type section: function signatures (uniform starter-subset form “i32-handle params × N → i32 Outcome handle”); entities/states/errors do not enter the WASM type section (no aggregate types in WASM), but go into generated constant metadata tables (names/field names go into the data section) for value ABI and host validation to reference.
- import section: effect host functions (§VI) + required runtime helpers (§5.3 decision).
- function + code sections: one function per `ExecGraph` node (action/transition); body instructions generated by AST traversal.
- memory section: a single linear memory (initial pages + a bump-pointer global).
- export section: export each callable (`run <Action>` entry), named `action_<Name>`/`transition_<Name>`; also export the bump-reset entry and memory.
- data section: string/name constant pool.

### 5.2 Body → instructions (structural induction; mirror interpreter)

Each AST construct’s instructions mirror the interpreter’s `eval`/`exec_stmt`:

| AST | WASM instruction strategy |
| --- | --- |
| `let x = e` | eval e → store handle in a WASM local |
| `set x = e` | eval e → overwrite local (HIR has guaranteed existence + mutability) |
| `return e` | eval e → wrap as Returned Outcome → `return` |
| `raise V {..}` | construct ErrorValue → wrap as Raised Outcome → `return` |
| `if c {..} else {..}` | eval c → `if`/`else` block (WASM structured control flow) |
| `match s { arms }` | eval s → chained `block`/`br_if` by tag (exhaustive; final arm as fallback); bind fields into locals |
| `repeat n {..}` | eval n → `loop` + counter (`n.max(0)` times); return/raise inside the body exits early through block `br` |
| `print e` | eval e → string handle → `call $console_write` (host import) |
| `Binary` / `Not` / `Neg` | i64 integer ops / comparisons / bool; `Add` statically dispatches Int/Text/List via `TypeTable` |
| `Call f(args)` | eval args → `call $action_f` → inspect Outcome.kind (raised bubbles; returned value is unwrapped) |
| `MethodCall Family.Op` | eval args → call the dynamic host import derived from the current registry; failure semantics in §VI |
| `Field` / `Construct` | value-ABI field reads / heap value construction |

Control flow uses WASM structured constructs (`block`/`loop`/`if`/`br`/`br_if`), matching the starter-subset (no goto).

### Decision ⑤: where to put runtime helpers (length / string concat / value equality / value construction)?

Adopt: generate them into the module as private prelude functions (alloc/make_*/get_*/value_eq/wrap_returned/raised/outcome_*/reset). Body code calls them. Pros: artifacts are self-contained, host-agnostic, byte-deterministic (strip-assist friendly). Cons: fixed prelude cost. Reject: moving core value semantics to hosts; only true I/O (effects) go via imports (§VI).

---

## VI. Effect ABI (A4: effects via host imports)

Effects are Sophia’s only interface to the outside world and naturally map to WASM host imports—also the enforcement point for capabilities. The current implementation has converged from a fixed `EffectHost` method set to a registry-driven single path: both the interpreter and the WASM runner call host operations through the same `HostRegistry`.

### 6.1 Imports (fixed imports + registry-derived dynamic imports)

Modules always declare two fixed imports under `sophia_host`:

| import | Signature | Semantics |
| --- | --- | --- |
| `console_write(ptr:i32, len:i32)` | `Text` byte slice | Console output |
| `read_copy(dst:i32)` | copy the previous host result bytes | copy-back channel for host results |

All other library operation imports are derived from the current `LibraryRegistry`; codegen emits only the host operations actually referenced by the program:

| Import shape | Signature | Semantics |
| --- | --- | --- |
| `sophia_lib:<library>.<host_fn>` | `(args_ptr:i32, args_len:i32) -> i32` | arguments encoded as `ValueWire`; return value length; caller uses `read_copy` to copy returned bytes |

This covers both the standard library and third-party libraries. File/Http are standard-library operations, no longer codegen special cases; third-party `host.wasm` providers are registered as the same kind of `HostFn`. Capability allow/deny is still checked by the semantic layer; the runner links imports according to the already-checked registry/operation set.

### 6.2 ValueWire boundary

The host-import boundary does not expose the WASM module’s internal heap-value layout. It uses `runtime::value_wire`:

- `ArgsWire = argc:u32 + ValueWire*`.
- The current supported boundary values are `Unit` / `Bool` / `Int` / `Text`; intents erase at the boundary, matching the interpreter runtime model.
- The runner import callback decodes arguments, calls `HostRegistry::call(family, op, args)`, encodes the returned `Value`, stashes it, and exposes it to the guest through `read_copy`.
- Type mismatches, missing imports, provider traps, and host errors become hard errors/traps. Never fabricate success.

### 6.3 Host-side implementations (who provides imports)

- Standard-library native/mock providers: File/Http operations are registered as Rust host functions in `HostRegistry`. Deterministic tests register mock providers; CLI real execution registers native providers.
- Third-party `host.wasm` providers: providers must export `memory`, `sophia_alloc`, `sophia_read_copy`, and `host_fn(args_ptr,args_len)->result_len`. `runtime::WasmHostFn` loads the provider, sends/receives ValueWire values, and registers it as an ordinary `HostFn`.
- WASM program runner: `sophia-runtime::WasmProgramRunner` is the production non-browser host runner. Differential tests, `sophia run --backend wasm`, and `sophia smoke --backend wasm` reuse this path.

---

## VII. Tooling and diff tests (A3: interpreter as oracle)

### Decision ⑥: how to emit and how to execute WASM in tests?

- Emit: adopt a pure-Rust WASM encoding crate (e.g., `wasm-encoder`)—lightweight, no system deps, deterministic bytes. Do not generate `.wat` and call external `wat2wasm` (brings external toolchains; non-deterministic gaps). Exact crate/version pinned at implementation; recorded in engineering notes.
- Execute (for diff tests): adopt a pure-Rust WASM interpreter. The current path is `sophia-runtime::WasmProgramRunner` (internally based on `wasmi`)—interp-based, pure Rust, no system deps, eligible for `cargo test`. Do not use wasmtime in gates (has cranelift JIT; heavy; potential system deps)—wasmtime is for real deployments.

Selection principle: dependencies in `cargo test` must be pure Rust, deterministic, and sans heavy system deps. Real deployment toolchains (wasmtime/browsers) are artifact consumers; not in gates. The design review fixes the shape (encoding + interpreter; both pure Rust); names/versions fixed at implementation.

### 7.1 Differential testing—fulfilling Criterion 1

Add a diff-test harness (location in §IX): for each program and each hidden case:

1. Run with interpreter → `Outcome` (oracle).
2. Emit WASM + run through `sophia-runtime::WasmProgramRunner` (inject same mock seed) → `Outcome'`.
3. Assert `Outcome == Outcome'` (compare value structures: Returned values / Raised variants).

Any mismatch fails the diff test with honest attribution—no smoothing or fabricated agreement. Diff programs reuse existing executable coverage: benchmark L1–L6 tasks + e2e reference solutions (already interpreter-passing). This naturally covers scalars/aggregates/cross-calls/error algebra/`one of`/Console/File/Http.

In the initial phase, diff tests join deterministic CI (`dev_checklist_v1.md`): “Workflow A additionally requires per-hidden-case equivalence; included in deterministic gates.” Real-LLM e2e/benchmarks remain examples, not in CI.

### 7.2 Strip-assist at the artifact layer (A5 / Criterion 3)

Extend `tools/check`’s strip-assist gate: emit two `.wasm` artifacts (original vs strip-assist) for the same program and assert byte-for-byte equality. This requires deterministic emit (value layout order; stable constant-pool order; no timestamps; no HashMap iteration order)—§IV/§V layout decisions serve this. This extends “formal-core fingerprint equality” to “artifact-byte equality.”

---

## VIII. `sophia build` landing (A5; completed)

`cli/src/commands.rs::build` changed from a no-op to: (i) run `check` (includes IR-layer strip-assist); (ii) run the artifact-layer strip-assist gate (`sophia_codegen::check_artifact_strip_equivalence`—assert identical `.wasm` after stripping assists); (iii) build a registry-aware artifact using `full_registry_for(root)`; (iv) emit `sophia-runs/build/program.wasm`, `program.sophia-build.json`, and third-party `hosts/<lib>/host.wasm` assets; (v) for constructs not yet covered by codegen (`to_text`/`List`—no v1 demo need), report `NotYetImplemented` honestly—do not fabricate outputs (interpreter remains usable). `smoke` build now truly emits, and `smoke --backend wasm` reuses the same build-artifact execution path.

`program.sophia-build.json` records `wasm_sha256`, `registry_fingerprint`, the dynamic import list, provider kinds, and third-party host wasm hashes. `run --backend wasm` / `smoke --backend wasm` validate these fields before execution. Source or registry drift, missing artifacts, or host asset hash mismatches hard-fail and ask the user to rebuild.

Artifact gates live near `tools/codegen` (owner of byte emit), complementing IR-layer gates in `tools/check`: Criterion 3 = IR-fingerprint unchanged ∧ artifact bytes unchanged. The `materialize` command’s `artifact_diff` gate still compares IR-level strip-assist; including WASM-byte diff in the same gate is a possible incremental step.

---

## IX. Implementation phases and landings (execute after discussion confirmation)

Aligned with v1 Workflow A (A1–A6). Each phase is independently mergeable/testable; before merging, require fmt + clippy (-D warnings) + tests all green, and differential tests equivalent per hidden case. Interpreter as oracle throughout.

| Phase | Content | Landing | Acceptance |
| --- | --- | --- | --- |
| W1 (A1) | Freeze input contracts: document `SemanticModel`/`ExecGraph` as codegen inputs; create `tools/codegen` crate skeleton (depends on core; deterministic; no I/O) | `docs/wasm_codegen.md` (this doc) + `tools/codegen` | Contract docs + empty crate compile ✅ Completed |
| W2 (A2) | Minimal emit: value ABI (§IV) + function ABI + scalars/arithmetic/`if`/`match`/`let`-`set`/`return`-`raise`/cross-calls; entity/state/error value construction + field access | `tools/codegen/src/*` | Unit: emit a valid module (executor loads) ✅ Completed (W2a–W2d: all 8 value kinds + all operators + all control flow + entity/state/Text/`repeat`; `to_text`/`List` are YAGNI placeholders) |
| W3 (A3) | Diff-test harness: interpreter vs WASM per hidden case; reuse benchmark/e2e reference solutions | `tools/codegen/tests/diff.rs` + `sophia-runtime::WasmProgramRunner` | L1–L5 + D1 equivalence all green ✅ Completed (emit + production runner + interpreter oracle comparison; current suite has 24 equivalent cases covering L1–L6 [D1/D2/D3] + G2/G5 shapes) |
| W4 (A4) | Effect host imports + capability boundary: Console fixed import; library operations as registry-derived dynamic imports; diff tests cover D2 (Http)/D3 (File)/G2 (Console)/G5 (File) | codegen import section + `HostRegistry` + `runtime::value_wire` | Diff tests with effects equivalent ✅ Completed (fixed `console_write`/`read_copy` + dynamic `sophia_lib:<library>.<host_fn>` imports; File/Http and third-party `host.wasm` providers are isomorphic HostFns; ValueWire boundary; failures trap/hard-error) |
| W5 (A5) | Artifact-layer strip-assist byte diff; `sophia build` emits `.wasm` + manifest + host assets; `run/smoke --backend wasm` execute build artifacts | `tools/codegen` + `cli/src/commands.rs` + `sophia-runtime` | Byte-diff gate + build artifact ✅ Completed (`sophia build` check→gate→emit `program.wasm` / `program.sophia-build.json` / `hosts/<lib>/host.wasm`; WASM `run`/`smoke` validate manifest/hash/registry before execution) |

Note: A6 (incremental query architecture; Salsa-style; supports LSP) is decoupled from codegen and may proceed in parallel; it is outside this design review’s scope.

---

## X. Decision points (seven adopted, 2026-05-31)

1. No new lowered body IR; traverse AST directly (A). Adopted.
2. Value ABI: tagged heap + i32 handle + bump-only memory; no GC (A). Adopted.
3. Raising uses Outcome wrapper (kind + handle) bubbling via returns; no WASM exceptions. Adopted.
4. Pure value helpers generated into the module (prelude); only I/O goes via host imports. Adopted.
5. Emit via pure-Rust encoder; diff tests via pure-Rust interpreter (deployment hosts not in gates). Adopted.
6. New crate location: `tools/codegen` (deterministic tooling layer; depends on core; no I/O). Adopted.
7. Diff programs: reuse benchmark/e2e interpreter-passing references; do not invent new programs. Adopted.

---

## XI. Future directions (outside the current boundary)

The current goal is path consolidation and direct execution of project build artifacts, not a general-purpose WASM launcher. The supported boundary is:

```bash
sophia build .
sophia run <Action> --root . --backend wasm
sophia smoke --root . --action <Action> --backend wasm
```

These commands still use the project-root sources and current `LibraryRegistry` as semantic context, and validate the build manifest before execution. The following remain future directions and need explicit design review once demanded:

1. Entry-scoped artifacts: `sophia build --entry <Action>` packages only reachable callables/imports and records the entry signature, input/output contract, and minimal capability surface.
2. Offline bundle loader: execute a bundle without project sources/current registry. This requires an entry manifest that fully carries semantic signatures, registry fingerprint, host provider assets, and compatibility checks; no fallback is implemented today.
3. Arbitrary `.wasm` path execution: `sophia run path/to/program.wasm` would require a Sophia manifest sidecar. Bare wasm lacks Sophia types, entry metadata, registry data, and provider information, so it cannot enter the current runner directly.
4. WASM trace instrumentation: `--trace` for the wasm backend should fail honestly today. If added, prefer explicit host imports or compile-time instrumentation; the runner must not infer internal semantics.
5. Browser/Node loaders: they can consume the same manifest and ValueWire ABI, but should be downstream host implementations, not cargo deterministic gates and not alternate semantic sources.
6. ValueWire extensions: add `List` / `one of` / `Entity` / `State` across the host boundary only when a real library contract needs them. Unit/Bool/Int/Text cover current host-provider needs.

---

## XII. Change log

- 2026-05-31 — Draft (design-review proposal). Define Workflow A as v1 Criterion 1 (diff-test equivalence) + Criterion 3 (artifact-layer strip-assist); inventory interpreter semantics (value model/body sublanguage/effect delegation) as the oracle to replicate in WASM; propose value ABI (tagged heap + i32 handle + bump-only memory), function ABI (Outcome-wrapped returns + raise bubbling), effect ABI (host imports + capability-driven injection), and tooling (pure-Rust encoder + pure-Rust interpreter; deployment tools not in gates); list W1–W5 phases (aligned with A1–A5; A6 decoupled) and seven decisions. Pure documentation; no code changes.
- 2026-05-31 — Finalized (seven decisions adopted). Confirmed: (i) no lowered body IR (traverse AST; avoid dual truth); (ii)/(iii) value ABI as tagged heap + i32 handle + bump-only memory; (iv) raise via Outcome wrapper (no WASM exceptions); (v) pure value helpers in-module; I/O via host imports; (vi) pure-Rust encoder/interpreter for gates; (vi) new `tools/codegen` crate (deterministic tool layer); (vii) reuse benchmark/e2e references for diffs. Move from draft to final; proceed W1→W5. Pure docs.
- 2026-05-31 — W1 landed (A1: freeze input contracts + crate skeleton). New `tools/codegen` crate (deterministic tool layer; depends on `sophia-syntax`/`sophia-semantic`/`sophia-exec-ir`; zero I/O): `CodegenInput` (contract.rs) binds the three frozen inputs (SemanticModel/ExecGraph/full-program AST + recomputable `TypeTable`) into one read-only entry; `CodegenInput::new` internally constructs the graph via `ExecGraph::from_model` (same as interpreter). `CodegenError` (`error.rs`: `InvalidInput`/`NotYetImplemented`); `emit_module` W1 placeholder returns `NotYetImplemented` honestly (no dummy module). Workspace registers `tools/codegen` and adds `sophia-codegen` dep. W1 contract tests: (i) graph matches model callables (incl. cross-call edge Quad→Double); (ii) emit honest placeholder. Workspace 338 passed/0 failed (336+2); clippy -D warnings clean; fmt clean. Next: W2 (minimal emit: value ABI + function ABI + scalars/arithmetic/control-flow body).
- 2026-05-31 — W2a landed (A2 minimal emit: scalar core + A3 diff harness). `tools/codegen` integrates `wasm-encoder` 0.243 (rust 1.80 cap) to emit `.wasm`; `wasmi` 0.40 (dev-dep) runs diff tests. Value ABI (abi.rs): tagged heap + i32 handle + bump-only memory; Int `[tag@0][i64@8]`/Bool `[tag@0][i32@4]`/Null·Unit `[tag@0]`; Outcome `[kind@0][value@4]`. Emit (emit.rs): module with prelude (alloc/make_*/get_*/value_eq/wrap_returned/raised/outcome_*/reset), one function per callable (i32×N → i32 Outcome; deterministic naming/section order). Covers `Unit`/`Bool`/`Int`/`Null`; literals/`Ident`/`Not`/`Neg`; binary ops (`And`/`Or` via i32 and/or with pure-Bool operands; `Eq`/`Ne` via `value_eq`; `Lt`–`Ge` via i64 compare; `Add` per `TypeTable`; `Sub`/`Mul`); `if`/`else`; `let`/`set`; `return`; cross-call `Call` (Outcome-kind check; bubble raised; take returned). Honest placeholders for `match`/`repeat`/`raise`/`print`/`to_text`/Text/List/`Field`/`MethodCall`/`Construct`. Diff tests: 5 tasks all equivalent. Workspace 344 passed/0 failed; clippy clean; fmt clean. Next W2b (`match`/`repeat`/`raise` + Text/List/Entity/State + `Field`/`Construct`), then W4 (effect imports).
- 2026-05-31 — W2b landed (A2 error algebra + `one of` returns + `match`). Extends emit: ErrorValue record layout (deterministic key ordering); constant string pool; prelude helpers (`str_eq`/`rec_field`/`rec_name_eq`); emit for `raise V{..}`/failure-member `Construct`/`match` over Bool/Null/scalar Type/Variant pattern. Guard `Eq`/`Ne` to scalars per `TypeTable`. Placeholders for `repeat`/Text/List/Entity/State/Type/State patterns/entity `Construct`/nested record fields. Diff tests add 3 cases; all equivalent. Workspace 347 passed/0 failed; clippy clean; fmt clean. Next W2c (Entity/State + patterns) then W4/W5.
- 2026-05-31 — W2c landed (A2 aggregates: Entity + State). Extends emit: State layout (`[tag][state_ptr][state_len][value_ptr][value_len]`); constant pool extended (entity/state names; field/value names); prelude helpers (`make_state` + name-equality); generalized record emit; `Construct`/`Field` emit; `match` adds entity/state Type patterns and `State`-value pattern. Placeholders for `repeat`/Text/List/`Text.length`/stdlib I/O/nested records. Diff tests add 4 cases; all equivalent. Workspace 351 passed/0 failed; clippy clean; fmt clean. Next W2d (`repeat` + Text/List + `Text.length` + `to_text`) then W4/W5.
- 2026-05-31 — W2d landed (A2 Text + `repeat`). Extends emit: Text layout (`[tag][bytes_ptr][byte_len]`); intern string literals; prelude helpers (`make_text`/`text_length` with Unicode scalar counting; `text_concat`); `value_eq` adds Text; emit of `Str`/Text `Add`/`Text.length` pseudo-field/Text `Type` pattern; `repeat` as counted loop; placeholders for `print`/`to_text`/List/stdlib I/O/nested records. Diff tests add 4 cases; all equivalent. Workspace 355 passed/0 failed; clippy clean; fmt clean. Next W4 (effect imports), then W5 (artifact diff + `sophia build`).
- 2026-05-31 — W4 landed (A4 effect host imports: Console/File/Http). Initial landing mapped effects to fixed `sophia_host` imports; later production work converged this to the current registry-derived import model described in §VI: fixed `console_write`/`read_copy`, dynamic `sophia_lib:<library>.<host_fn>` imports, and ValueWire at the host boundary. Host failures trap / hard-error honestly. Diff tests cover Console, Http+intent, File roundtrip, and third-party provider routing.
- 2026-05-31 — W5 landed (A5 strip-assist artifacts + `sophia build`; A1–A5 wrapped). `tools/codegen` adds `emit_from_sources(strip)` + `check_artifact_strip_equivalence` (identical `.wasm` bytes pre/post strip; requires deterministic emit since W2). `sophia build` runs check → artifact gate → emits `sophia-runs/build/program.wasm`; uncovered constructs reported `NotYetImplemented` honestly. `sophia.toml` updated; smoke emits; `tools/codegen` gains `sophia-hir` dep; CLI gains `sophia-codegen` dep. Codegen tests + deterministic cases; CLI pipeline proves build emits + honest reporting. Workspace 362 passed/0 failed; clippy clean; fmt clean. A1–A5 achieved: v1 Criterion 1 (per-case equivalence) + Criterion 3 (artifact strip-assist). A6 awaits its own design review; real deployment hosts wired per need; `to_text`/`List` added on demand.
- 2026-06-03 — WASM production runtime path folded into this document. Third-party library context consistency, dynamic host imports, the ValueWire provider ABI, the non-browser `WasmProgramRunner`, build bundle manifests, and `run/smoke --backend wasm` have landed. The standalone runtime-plan document was deleted; completed issues moved to `dev_checklist_v1.md`, and future directions moved to §XI. Current support is project-root build artifact execution, not arbitrary bare `.wasm` paths or offline bundle loading.
