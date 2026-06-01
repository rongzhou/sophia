# Sophia Language Implementation

> This document defines the implementation layers of the Sophia compiler and runtime (AST, IR, type system implementation, checking pipeline, runtime model, etc.).
> Language and workflow concepts are in `language_design.md`.
> Engineering directories, CLI, and toolchain wiring are in `engineering_architecture.md`.

---

## I. Implementation language

Rust is Sophia’s implementation language.

Rationale:

- The compiler’s core assets (Semantic IR, Execution Graph IR) require a long-term stable memory model and strong type safety.
- The initial execution backend is an in-process Rust interpreter; the first v1 codegen target is WASM, and Rust + wasm-bindgen/wasmtime are the most direct toolchains.
- The compiler may evolve to self-hosting in the future.

Admitted cost: LLM orchestration ecosystems are more mature in Python/TypeScript; parts of the heuristic workflow infra (structured outputs, prompt management) must be implemented in-house; there are no equivalent Rust libs to reuse directly. These components belong to the engineering-architecture layer and do not pollute the compiler core.

---

## II. Overall compiler pipeline

```text
Sources (.sophia)
    ↓ file scan
    ↓ node index (asg_index.json)
    ↓ Tree-sitter parse
    ↓ CST
    ↓ CST → AST
    ↓ AST
    ↓ name + module resolution
    ↓ HIR
    ↓ ASG build
    ↓ Semantic IR (type/effect/contract layers)
    ↓ Execution Graph IR
    ↓ Interpreter (v0 only backend)
                    or
                  ↓ Codegen (v1+: WASM; later optional native)
                  ↓ target artifact
```

| Stage | Input | Output |
| --- | --- | --- |
| File scan | directory layout | file list |
| Node index | file list | `asg_index.json` |
| Parse | `.sophia` | CST → AST |
| Name resolution | AST + index | resolved AST |
| HIR | resolved AST | HIR |
| ASG build | HIR | ASG |
| Semantic check | ASG | checked Semantic IR |
| Exec IR | Semantic IR | Execution Graph IR |
| Interpret/codegen | Exec IR | runtime result / artifact |

Iron rule: the compiler must not call an LLM. All LLM calls happen only in the workflow layer.

---

## III. Parser layer

### 3.1 Tree-sitter

Use Tree-sitter as the parser, providing:

- grammar definitions;
- incremental parsing;
- syntax highlighting;
- editor integration.

Output: a Concrete Syntax Tree (CST).

The CST is not meant for consumption by later phases; it is a lossless representation. CST → AST is a separate transform that discards trivia (whitespace/comments) but preserves spans.

---

## IV. AST layer

### 4.1 Responsibilities

AST expresses only surface syntax structure:

- syntax structure
- literals
- spans
- declarations
- expressions

AST has no types, semantic bindings, or execution semantics.

### 4.2 Memory model: Arena + ID references

From the AST layer onward, use Arena + ID references; do not use `Box<Node>` or `Rc<RefCell<Node>>`:

```rust
// Avoid: recursive ownership; hard to cross-reference
struct AstNode {
    children: Vec<Box<AstNode>>,
}

// Prefer: ID references; no ownership issues
type NodeId = u32;

struct AstArena {
    nodes: Vec<AstNode>,
}

struct AstNode {
    children: Vec<NodeId>,
    span: Span,
}
```

Tooling:

- `typed-arena` or `bumpalo` for allocation;
- `slotmap` for stable IDs (no ID reuse after deletions; avoid dangling refs).

This pattern extends through HIR and Semantic IR for consistent reference semantics.

---

## V. HIR layer

### 5.1 Responsibilities

HIR performs semantic normalization:

- name resolution
- module resolution
- symbol binding
- scope analysis

HIR is the first IR with definite semantic meaning.

### 5.2 Name resolution rules

- All references must be resolvable via `asg_index.json`.
- No implicit imports.
- No shadowing (including body locals).
- Cross-domain references must be explicitly declared via boundaries or task includes.

---

## VI. Semantic IR

### 6.1 Internal layering

Semantic IR is Sophia’s most central architectural layer, covering type inference, effect propagation, capability checks, error propagation, and cross-node contract checks—easily collapsing into a “god object.”

To avoid collapse, the Semantic IR uses an internal three-layer structure while exposing a unified interface:

```
semantic/
├── type_layer/        ← type inference and constraint solving
├── effect_layer/      ← effect analysis and propagation
└── contract_layer/    ← tool contracts, memory scopes, capability satisfaction
```

### 6.2 Where to store inferred info: Table pattern

Each layer’s inferred info is stored in a separate table rather than modifying IR nodes directly:

```rust
TypeTable     ← type inference results by NodeId
EffectTable   ← effect propagation chains
CapTable      ← capability satisfaction
```

IR nodes remain immutable and store only declarations (type signatures, effect declarations, capability requirements), not derived results.

Benefits:

- Semantic IR can be analyzed concurrently.
- Only affected table entries are recomputed upon invalidation.
- Node-level invariants (immutable) are decoupled from analysis-level invariants (recomputable).

---

## VII. Type system implementation

### 7.1 Gradual typing

Sophia-Core supports gradual typing. `Unknown` degrades to dynamic checks at runtime.

Introduce `schema of T` as a first-class type for “LLM outputs structurally conforming to schema T.” Mismatches trigger fallback edges in Exec IR, not runtime panics.

### 7.2 Intent type checks

Intent assignability uses strict equality:

- `Raw<Text>` cannot be assigned to `Sanitized<Text>`.
- `Sanitized<Text>` cannot implicitly downgrade to `Text`.

Expression inference preserves intent: `Raw<Text> + Text` infers `Raw<Text>`; `Sanitized<Text> + Sanitized<Text>` infers `Sanitized<Text>`.

Explicit conversions use `intent_conversion: true` and must satisfy:

- single input/output;
- same inner type;
- different intent;
- no effects;
- body directly `return`s the input value.

Boundary rules:

- Action calls, entity construction, returns, and Console/library write boundaries must satisfy intent rules.
- `Console.Write` only accepts literals/`Sanitized<T>`/`Redacted<T>`; standard-library writes follow the same rule (e.g., `File.Write` requires `Sanitized<Text>`; see `file_lib.md`).

### 7.3 Effect checks

Adopt algebraic effects.

- Effects used in the action body must be included in `action.effects`.
- Callee’s observable effects must be a subset of caller’s effects.
- `Pure` is exclusive with other effects; callers need not restate `Pure`.
- Internal representation is a `(family, op, args)` triple (e.g., `Console.Write` as `(Console, Write, [])`, `Payment.Charge(1)` as `(Payment, Charge, [1])`). Referencable effect families come from two merged sources: the built-in `Console` and standard-library families `File`/`Http` (pre-seeded via `hir::builtins::BUILTIN_EFFECT_OPS`), and user top-level `effect` declarations (see `language_design.md` §13). Name resolution validates referenced `Family.Op` and arity. Equality/subset algorithms compare triples regardless of source (see §20 for implementation).

### 7.4 Capability checks

- Action effects must be allowed by the capability and not denied.
- `deny` takes precedence over `allow`.
- No dynamic capabilities.

### 7.5 Error checks

- `raise` must be declared in the action’s `errors`.
- Callee’s errors must be re-declared by the caller (until error handling lands).
- `match` must be exhaustive; `_` is forbidden.

### 7.6 Minimum check set

| Check | MVP requirements |
| --- | --- |
| Parse | one top-level node per file; unknown blocks error |
| Name Resolution | all references resolvable via the index |
| Type Check | field assignment/return/action-call type compatibility; block scopes; non-Unit actions must return/raise on all paths |
| Intent Check | forbid writing weak intents into strong intents |
| Effect Check | effects used in body must be declared |
| Capability Check | action effects must be allowed and not denied by the capability |
| Error Check | `raise` must be declared; callee errors must be re-declared by caller |
| Strip Assist | compare pre/post-strip Semantic IR and Exec IR; from v1 also compare WASM artifact bytes |

---

## VIII. Execution Graph IR

### 8.1 Responsibilities

Exec IR explicitly describes runtime execution structure:

- execution DAG
- task dependencies
- awaits
- retries
- cancellation
- scheduling
- checkpoints
- concurrency boundaries

```text
Task A
  ├── Task B
  ├── Task C
  │     └── Task D
  └── Task E
```

Exec IR bridges Semantic IR and the Runtime.

### 8.2 Edge type system

Edges are first-class concepts in Exec IR, not hidden properties of retry/cancellation:

```rust
DataEdge<T>
StreamEdge<T>
ControlEdge
ConditionalEdge
FallbackEdge
```

`schema of T` mismatches trigger `FallbackEdge`, not runtime panics.

---

## IX. Runtime architecture

### 9.1 Structure

```text
Sophia Runtime
    └── Tokio Runtime Substrate
```

Tokio provides:

- async scheduling
- task execution
- IO runtime

Sophia Runtime provides:

- execution-graph execution
- context propagation
- effect tracking
- tracing
- cancellation
- retries
- checkpointing
- runtime inspection

### 9.2 Interpreter

Initial execution uses interpretation:

```text
Semantic IR
    ↓
Execution Graph IR
    ↓
Interpreter
```

Interpreter responsibilities:

- runtime validation
- tracing
- semantic inspection
- execution debugging

v0 has no codegen; v1 codegen route is in §12.

### 9.3 Async boundary split

Clearly separate sync vs async code:

Sync (pure functions, no Tokio):

- all `core` (parser/HIR/Semantic IR/Exec IR)
- the core logic of `check` and `audit`

Async (I/O; uses Tokio):

- `llm` (LLM API network calls)
- `graph-db` (SQLite)
- `materialize` (file writes)
- `cli` (coordination layer)

Benefits of keeping the compiler core sync:

- easier unit testing (no async runtimes)
- easier reasoning (no races)
- preserves potential for future WASM compilation (limited async in WASM)

### 9.4 Execution trace and mapping to Exec Graph

Tracing is a projection of Exec Graph execution, not an independent observation layer. Each span must carry refs to concrete nodes/edges in the graph:

```rust
struct ExecutionSpan {
    seq: u32,                    // deterministic entry order (replaces wall-clock timeline)
    node_id: ExecNodeId,         // node entered in Exec IR
    edge_id: Option<ExecEdgeId>, // call edge that triggered this entry; None for top-level
    callable: String,            // callable name (human-friendly)
    depth: u32,                  // call depth (0 at top level)
    outcome: SpanOutcome,        // project back to graph: returned / raised domain error
}
```

This lets trace data map directly back to the graph, supporting queries like “which node was slowest” or “which edge triggered fallback,” not just timeline strings.

Status (feedback): implemented (initial subset). `core/exec-ir` introduces stable `ExecEdgeId(u32)` (assigned in build order; `call_edge_id`/`edge` queries). `runtime` adds a `trace` module: the interpreter opens a span on each callable entry (pre-order) and writes outcomes on completion; `run_action` returns `Execution { outcome, host, trace }` with full projection. Determinism first: initially spans do not record real wall-clock `start`/`duration` (Instant/Duration are non-deterministic); only graph projection and `seq` are recorded. `tokens_used`/`cost_usd` belong to LLM nodes and will be wired when LLM-execution nodes are introduced. Real timing/metrics can be optional side channels and must not pollute the deterministic core. Trace is a runtime observability concern (§9.2), not on the correctness path—v0 interpreter yields correct results even without trace.

---

## X. Graph infrastructure

### 10.1 Core graph storage

Use a custom Graph Storage:

```rust
NodeId(u32)
TypeId(u32)
SymbolId(u32)
TaskId(u32)

Vec<Node>
Vec<Edge>
```

Supporting infra:

- `slotmap` for stable IDs
- `typed-arena` / `bumpalo` for arenas

### 10.2 Visualization

Use `petgraph` for visualization, graph debugging, and temporary transforms. `petgraph` is not part of core storage; it’s a tooling aid.

---

## XI. Incremental analysis

### 11.1 Priority: not in initial phase

Do not implement incremental analysis in the initial phase, because:

- In Sophia workflows, each LLM-generated CodeNode is a fresh candidate file set rather than an incremental edit of existing files.
- Incremental analysis has high value in “manual editing,” but limited value in “LLM regenerates each time.”
- Where incrementalism is truly needed is LSP hover/completion; that is a later phase.

### 11.2 API shape reserved

Even without incrementalism, expose query-style APIs from the beginning rather than mutable caches:

```rust
// Prefer: query style; leaves room for Salsa-like migration
fn resolve_symbol(db: &dyn Db, id: SymbolId) -> Symbol;

// Avoid: directly mutating caches
fn get_symbol_from_cache(&mut self, id: SymbolId) -> Symbol;
```

This eases migration to a real incremental layer later without caller changes.

### 11.3 Starter impl

- module cache
- symbol cache
- type cache

### 11.4 Later: Salsa-inspired incremental layer

- incremental semantic analysis
- dependency tracking
- semantic invalidation
- query caching

---

## XII. Codegen

### 12.1 Initial: no codegen, interpreter only

v0 emits no external artifacts. `sophia run` is executed by the in-process Rust interpreter; runtime input/output validation consumes Semantic IR/Exec IR metadata directly, without any intermediate language.

Reasons to delay codegen:

- The interpreter is the common backend for LSP/`sophia run`/test harness and the behavior oracle for `.sophia`; get it right first; later backends then have a solid equivalence baseline.
- Sophia’s body sublanguage is simple; interpreter overhead is not the bottleneck.
- Delaying codegen lets IRs stabilize first, avoiding backend coupling to unstable IR shapes.

### 12.2 v1: WASM as the first codegen target

Rationale for introducing WASM in v1:

- Sophia’s semantic subset (no async; no threads; explicit effects) aligns with WASM MVP’s sweet spot.
- Mature Rust ecosystems (wasm-bindgen/wasmtime/wasmer) are available.
- WASM artifacts can be hosted by Node, Python (pyodide/wasmtime-py), browsers, and edge runtimes—wider coverage than binding to a single ecosystem.
- Aligned with Sophia’s semantic-first stance: codegen projects semantics into a general-purpose executable format embeddable by multiple hosts, not tied to a single ecosystem.

v1 WASM emit (draft):

- Entities/states/errors project into a module’s type section + metadata tables.
- Actions compile into WASM functions; effects are exposed via imports to hosts.
- Standard-lib I/O (`File`/`Http`) are accessed via host-imported capability interfaces.
- Runtime input/output validation is executed on the host side via shared schema metadata (the same metadata as the interpreter).

### 12.3 Later optional backends

After WASM:

- native (cranelift/LLVM lowering) for performance scenarios
- TS/Python named-language emission only when there is a clear deployment need to that ecosystem—on demand, not a core route

### 12.4 Strip-assist equivalence gates

`sophia check` compares pre/post-strip Semantic IR/Exec IR hashes:

- Removing all Semantic Assist fields must not change the Formal Core/IR/interpreter outcomes.
- From v1, also compare WASM artifact byte sequences.

---

## XIII. Serialization

### 13.1 Framework

Use `serde`.

### 13.2 Binary exchange format

Use MessagePack for:

- graph snapshots
- runtime state exchange
- distributed execution (not implemented; checkpoint/resume semantics defined at IR level)
- semantic cache persistence

---

## XIV. Diagnostics

### 14.1 miette

Use `miette` as the diagnostics framework:

- structured compiler diagnostics
- source spans
- contextual messages
- colored output

### 14.2 Diagnostic split

Compiler vs workflow diagnostics use different error types and must not mix.

Compiler diagnostics carry source spans and apply to Sophia-Core analysis phases:

```rust
#[derive(Diagnostic)]
#[diagnostic(code(sophia::type::mismatch))]
struct TypeMismatch {
    #[label("expected {expected}")]
    expected_span: SourceSpan,
    #[label("found {found}")]
    found_span: SourceSpan,
    expected: String,
    found: String,
}
```

Workflow diagnostics carry node IDs and graph context and apply to the workflow engine:

```rust
#[derive(Diagnostic)]
#[diagnostic(code(sophia::gate::failure))]
struct GateFailure {
    node_id: NodeId,
    gate: GateKind,
    #[help]
    reason: String,
}
```

The two diagnostic types are rendered uniformly at the CLI but remain independent within their crates.

### 14.3 LLM-oriented error format

Error messages must serve both compiler diagnostics and LLM repair loops:

```text
ERROR CHECK-TYPE-001
At:
  domains/TodoDomain/actions/AddTodo.sophia:42:12

Problem:
  Raw<Text> was assigned to Todo.title.

Expected:
  Sanitized<Text>

Actual:
  Raw<Text>

Why:
  Todo.title requires text be sanitized via a conversion.

Repair options:
  1. Call the conversion action SanitizeTitle before constructing Todo.
  2. If callers already guarantee sanitization, change the action input type to Sanitized<Text>.
  3. Do not weaken Todo.title to Raw<Text>; it would violate the TitleNotEmpty invariant.

Related nodes:
  domains/TodoDomain/entities/Todo.sophia
  domains/TodoDomain/actions/SanitizeTitle.sophia
  domains/TodoDomain/tasks/ImplementAddTodo.sophia
```

`repair-context` generates only structured context; it does not call models.

---

## XV. Materialize Gate’s type-state pattern

Materialize Gate uses Rust’s type system to guarantee gate order at compile time rather than runtime if/else checks, preventing “skip gate” operations from surfacing only at runtime:

```rust
use std::marker::PhantomData;

struct CodeNode<S: NodeState> {
    id: NodeId,
    _state: PhantomData<S>,
}

// Gate states as type parameters
struct Unchecked;
struct CheckPassed;
struct AuditPassed;
struct Selected;

impl CodeNode<Unchecked> {
    fn run_check(self, checker: &Checker) -> Result<CodeNode<CheckPassed>> { ... }
}

impl CodeNode<CheckPassed> {
    fn run_audit(self, auditor: &Auditor) -> Result<CodeNode<AuditPassed>> { ... }
}

impl CodeNode<AuditPassed> {
    fn select(self) -> CodeNode<Selected> { ... }
}

impl CodeNode<Selected> {
    fn materialize(self, target: &Path) -> Result<MaterializeNode> { ... }
}
```

`materialize` is callable only on `CodeNode<Selected>`; the compiler prevents any path that skips gates.

---

## XVI. Starter subset

The first compiler milestone must deliver the limited subset below: it defines the “minimal compilable slice of language design goals,” enabling Sophia-Core to run `parse → check → build → run` end to end very early, and then expand incrementally to the full capabilities in `language_design.md`.

Broader design items (task execution; cross-domain boundaries; Semantic Identity/Evolution Boundary; independent Sophia IR backend; etc.) are extension points, not part of the starter subset checker/build.

`transition` is a callable in the starter subset (checkable and interpretable): it shares signature/body sublanguage/three-layer checks/Exec-IR nodes/interpreter paths with `action`, differing only in default pure semantics (`Pure`). Calls to transitions use constructive syntax (`Name { field = expr }`) or direct calls (`Name(args)`). Its contract proof (`requires`/`ensures` static proofs; state transition graph constraints) is outside the starter subset (see 16.2/16.6). `task` name resolution and semantic closure (§8 Task Closure) are in the subset, but `task` is not an execution entry and is not run.

### 16.1 Entity

- File path `domains/<Domain>/entities/<Entity>.sophia`.
- Entity top-level name, filename, and ASG node name must match and use PascalCase.
- Each field in `fields` must explicitly declare a type.
- Field types may be: `Unit`/`Bool`/`Int`/`Text`/`Null`/`list of T`/`one of { M, ... }`/declared entity or state types in the ASG, and Intent wrappers on scalars.
- Action input/output may use entity and Intent-wrapper types.
- Body expressions support field access (`account.balance`) and full entity construction (`Account { balance = ..., is_locked = ... }`).
- Constructing an entity must provide all fields; unknown/missing fields or mismatched field types are errors.
- `meaning`/`not` and other Semantic Assists are subject to strip-assist equivalence gates.
- The interpreter uses the same entity metadata for runtime input/output validation at action boundaries—no intermediate language needed.

Out of the starter subset: invariants (static/runtime proofs); `entity.with` update syntax; cross-domain entity-boundary checks.

### 16.2 State

- File path `domains/<Domain>/states/<State>.sophia`.
- Syntax `state Name { value ValueName { ... } }`; the `value` keyword is mandatory.
- State types may appear in action I/O, entity fields, and error-variant fields.
- Body expressions support state values (`TodoStatus.Pending`, `TodoStatus.Done`).
- Checker rejects unknown state types/values, duplicate states, empty states, duplicate values.
- Interpreter represents state values as tagged unions (tag = value name string); runtime validation checks input membership in the declared set.

Out of the starter subset: formal assists on state values; per-value invariants; state transition graph constraints; contracts linking states to `transition` nodes.

### 16.3 Error algebra (minimal subset)

- File path `domains/<Domain>/errors/<Error>.sophia`.
- Syntax `error Name { variant VariantName { field: Type } }`.
- Variant field types may use existing types, entity types, and Intent wrappers.
- Action `errors { VariantName }` must reference declared variants.
- Body supports `raise VariantName { field = expr }`.
- Checker rejects unknown variants, raises not declared in action `errors`, missing/unknown fields, and mismatched field types.
- Error propagation: callee-declared errors must be re-declared by caller.
- Interpreter represents `raise` as control-flow breaks tagged by variant name and fields.

Out of the starter subset: error-handling syntax and exhaustiveness checks; mandatory mapping of external I/O errors to domain errors; runtime harness assertions over expected error results.

### 16.4 Intent types

- `Raw`/`Parsed`/`Validated`/`Sanitized`/`Verified`/`Authorized`/`Secret`/`Redacted` may wrap starter-subset types, entity types, and state types.
- Intent assignability is strict equality.
- Expression inference preserves intent.
- Explicit conversion actions use `intent_conversion: true`.
- Action calls/entity construction/returns/Console and library write boundaries must satisfy intent rules.
- `Console.Write` only accepts literals/`Sanitized<T>`/`Redacted<T>`; standard-lib writes similar (e.g., `File.Write` requires `Sanitized<Text>`).

Out of the starter subset: cross-domain/library intent compatibility, user-defined conversion proofs, richer external boundaries like HTTP response types, lattice-based intent subtyping.

### 16.5 Expressions

The starter subset accepts only the body sublanguage in `language_design.md` §VII (`let`/`set`/`return`/`raise`/`if/else`/`match`/`repeat N times`/`print` and full entity construction). Expression types limited to:

- Scalars: `Unit`/`Bool`/`Int`/`Text`/`Null`
- Structures: `list of Int`/`list of Text`/`one of { M, ... }`
- Entity variables, field access, entity construction
- Explicit `to_text(Int)`
- Comparisons; `and`/`or`/`not`; integer arithmetic (binary `+ - *`, unary `-x`); `Text + Text`; `list + [item]`; `list.append(item)`

Arithmetic excludes division `/` and modulo `%` (they introduce divide-by-zero/truncation semantics—deferred); unary `-x` (Int→Int) is included at the same precedence as binary `-`. Use comparisons + negation to express “absolute value” without division.

Implementation requirements:

- Field assignments in entities/actions/errors parse with balanced delimiters; nested constructs, lists, and commas in string literals must not split top-level fields.
- `one of { T, Null }` members are constructed directly (success returns the value directly; `Null` returns `Null`), with no `Some`/`None`/`Ok`/`Err` wrappers; “has value” uses `!= Null` in predicates and `match` with a `Null` arm in bodies.
- `match` exhaustiveness follows `language_design.md` §VII (subjects: `Bool`/state/`one of`; permanent ban on `_`). `one of` members dispatch via type patterns (`Int x =>`), variant patterns (`V { f } =>`), and `Null =>` (see `docs/type_system.md` §III).

### 16.6 Starter subset + stdlib I/O; still-outstanding body items

Standard-library I/O calls (special-root method_call + host delegation; zero new syntax): body-level calls look like `<Root>.<Op>(args)` where `<Root>` is the library’s built-in special-root ident allowed in HIR resolution and not put into ASG index. The type layer merges the corresponding effect and returns types; the interpreter delegates via `EffectHost`. All I/O libs share this path:

- Http (landed; see `docs/http_lib.md`): `Http.Get(url) → Raw<Text>`; effect merged as `Http.Get` (no arg—capability granularity “may GET”; URLs are usually runtime-bound; see `http_lib.md` §2.6); `url: Text`. Returns untrusted `Raw<Text>`; downstream must convert intents (existing strict-equality checks catch rejects; zero new checks). Interpreter delegates to `EffectHost::http_get`; `InMemoryHost` provides deterministic mocks (preset url→body; misses → `Err` hard-stop—never fabricate success). Real network is provided by the coordination-layer CLI’s `CliHost` (`reqwest::blocking`); the CLI `run` injects real networking only if the entry declares `Http.Get`. Real networking is not part of deterministic tests. See `type_layer::infer_effect_op`, `interp::try_effect_op` + `effect_host`.
- File (landed; see `docs/file_lib.md`): `File.Read(path) → Raw<Text>` / `File.Write(path, Sanitized<Text>) → Unit`; isomorphic to `Http` (special root + effect/capability + intent boundaries + host delegation). Interpreter delegates to `EffectHost::file_read/file_write`; `InMemoryHost` uses an in-memory bucket mock (`seed_file`; misses → `Err`). Real file I/O is provided by CLI `CliHost` (`std::fs`).

Historical change (2026-05-31): the v0 starter had a `storage` top-level node + built-in `DB.Read/Write` effect + `storage.<Name>.get/save` body syntax (in-memory KV partitioned by name). Due to unclear semantics (between relational DB/KV/persistence/in-memory) it was removed; persistence will return as a semantically clear `DB` library (see `stdlib_design.md` §VI). Local state/file I/O needs are handled by `File`.

Still out-of-subset body items:

- `entity.with`
- `requires`/`ensures` proofs
- error handling and exhaustiveness checks
- transition contract proofs (`requires`/`ensures` static proofs; state-transition graph constraints). Note: calling transitions (constructive `Name { ... }` or `Name(args)`) is in-subset and interpretable.

Unprovable `ensures` are not proved in the subset: we only perform name resolution and predicate typing (`Bool`) for `ensures`/`requires`, and do not generate proof obligations or `requires_runtime_check` diagnostics—those will land with the contracts subsystem. This is an intentional subset boundary, not silent acceptance of formal verification.

---

## XVII. Schema versions

### 17.1 `.pseudo` versions

`.pseudo` files record schema versions via an HTML comment:

```markdown
<!-- sophia-pseudo: v1 -->

## Purpose
...
```

`pseudocode_check` first extracts the version. On mismatch, it gives explicit migration hints rather than a “missing heading” error. Versioning follows language major versions; no independent evolution.

### 17.2 ASG index

`asg_index.json` is a rebuildable cache, not a semantic source. Minimal structure:

```json
{
  "version": 1,
  "nodes": {
    "Todo": {
      "kind": "Entity",
      "domain": "TodoDomain",
      "path": "domains/TodoDomain/entities/Todo.sophia"
    },
    "CompleteTodo": {
      "kind": "Action",
      "domain": "TodoDomain",
      "path": "domains/TodoDomain/actions/CompleteTodo.sophia"
    }
  }
}
```

The index must be generated after sorting by path; JSON keys must be emitted in a stable order to avoid different caches from the same source.

---

## XVIII. Invariants

Implementation covers the core language and workflow-graph invariants:

- Language core: immutable IR nodes for parser/HIR/Semantic IR + recomputable tables; checks for name resolution/type/intent/effect/capability/error (§VII).
- Workflow graph: see `workflow_graph_spec.md` §3 for N1–N6 and I1–I10. I3/I4/I7 are enforced directly by GraphStore `append_edge`/`append_node`. I9/I10 are guarded by CI tests.

---

## XIX. Recommended build order

The following order enables early end-to-end `parse → check → build → run` on the main path, then incrementally opens graph/events/LLM/gates. Each step can be merged independently; no need to wait for all to finish before starting the next.

1. Implement `syntax`: tree-sitter grammar, CST, AST, spans.
2. Implement `hir`: name/module resolution, scopes.
3. Implement `semantic`: type/effect/contract layers; write all derivations to separate tables.
4. Implement `exec-ir` and the interpreter; run through the starter subset (§XVI) for `parse → check → run`.
5. Land runtime input/output validation inside the interpreter: directly consume entity/state/error metadata—no IL.
6. Implement GraphStore and node/edge schemas: NodeMeta, Provenance, four-dimension model.
7. Implement ContextSnapshotNode and active-context derivation; do not wire LLM yet.
8. Implement “small nodes”: DecompositionNode, ConstraintNode, AcceptanceCriterionNode.
9. Implement core goal nodes: ObjectiveNode, MilestoneNode; fields all via edges.
10. Implement event nodes: AcceptanceEventNode, WithdrawalEventNode, ActivationEventNode.
11. Implement assessment family: AssessmentNode + FirstSliceNode + decomposition protocol for ConstraintNode + DecisionNode.
12. Implement DiagnosticNode (kinds: pseudo_check/code_check/constraint_audit/artifact_diff/regression_gate).
13. Wire LLM calls: all design/implement/repair/decision prompts read active context via ContextSnapshotNode.
14. Implement SelectionNode/MaterializeNode and the Materialize Gate type-state chain.
15. Implement starter LSP features (hover, diagnostics, goto).

Steps 1–15 are v0 (interpreter) and have landed (see `dev_checklist_v0.md`, archived/frozen).

### 19.1 v1 build order (WASM codegen + language/stdlib expansion)

v1 turns the language from “interpreter prototype” into a “compilable/deployable serious language” (see `engineering_architecture.md` §14.2’s two parallel workflows). The two lines can interleave, but both maintain “interpreter as the equivalence oracle,” not introducing a second semantic source of truth:

Workflow A (WASM codegen)
1. Freeze and document v0’s Semantic IR/Exec IR as the contract for codegen inputs (codegen must not demand IR-shape changes).
2. Implement minimal WASM emit: scalars/arithmetic/control-flow bodies → WASM functions; entities/states/errors project into type section + metadata tables.
3. Differential testing: run the same `.sophia` through the interpreter and the WASM backend; results must match per hidden case (interpreter is oracle).
4. Standard-library I/O effects (`File`/`Http`) are exposed to hosts via WASM imports; capabilities enforced at host-import layer.
5. Extend strip-assist equivalence gates to WASM artifact byte-level comparison (land in `sophia build`).
6. Incremental query architecture (Salsa-inspired) to support low-latency LSP (decoupled from codegen; can proceed in parallel).

Workflow B (language/stdlib expansion; demand-driven + per-item design gates)

B is not a fixed implementation sequence but a set of minimal expansions triggered by demo needs (see `dev_checklist_v1.md` §2 for D1/D2/D3 needs and per-item breakdown). v1 scope is bounded by three demos: D1 (fallible-result modeling); D2 (network fetch + intent safety; flagship LLM-native demo); D3 (serious pipeline composite), yielding the minimal expansion set:

7. F1 unify type syntax (one of/list of/`<>` exclusive intents) + fallible returns [from D1/D3]: `<>` exclusive to Intent Types; structural types use the `of` family (`list of T`/`one of { M, ... }`/`schema of T`); deprecate `Optional<T>`/`List<T>`/`Schema<T>`/`Some`/`None`; add `Null` and type patterns in `match`. Fallible returns are `one of { T, SomeError }` (members constructed/matched directly; no wrappers). See `docs/type_system.md`.
8. F2 `Http` effect family + host import [from D2]: `Http.Get → Raw<Text>`; like the built-in `Console` family, seeded into `BUILTIN_EFFECT_OPS` (stdlib effect families); reuse existing effect/capability + intent boundaries (zero new syntax) + `EffectHost::http_get`.
9. S1 HTTP host in the standard library [from D2]: real host for `Http.Get` (via `reqwest`; workflow/runtime layers only; `core` is zero-I/O). Standard library is functionality, not a protocol stack (implement only what’s needed; add incrementally per demos).
10. S2 Standard-library prompt scaffolding [from D2, prompt engineering]: each standard-library function has a standardized, on-demand prompt asset (reuse §8.3 preamble + `prompt/assets/`)—LLMs have no a priori knowledge of stdlib; without this they cannot use it.
11. Standard-library relocation + `File` library [(B) “I/O = libraries”]: assert that files/network/databases are stdlib (not language primitives); keep `Console` (`print`) as a built-in output primitive; remove the unclear `storage` top-level + built-in `DB` + `Persisted` intent; add `File` (`File.Read/Write`, isomorphic to `Http`; land in v1). See `stdlib_design.md`/`file_lib.md`; `engineering_notes.md` 2026-05-31 decision.

Explicitly delayed to v2+ (no v1 demo needs): task execution entry; `entity.with`; cross-domain/library intent dataflow; `requires`/`ensures` contracts subsystem; persistent `DB` (semantics must be clarified first). Each will pass its own design gate upon demand.

Each v1 step is independently mergeable/testable; A’s diff tests and B’s expansions each add to the v0 baseline rather than rewrite it.

---

## XX. Implementing top-level `effect` declarations

> Corresponds to `language_design.md` §13 (the design of top-level `effect`). This section is the implementation: how `effect` declarations and generic `Family.Op(args)` references are handled in each layer.

### 20.1 Per-layer handling

Follow layering discipline (`core` is zero-I/O; deterministic) across the chain:

- syntax: `effect_def` top-level rule (`operation`/`param` blocks). Effect references use a generic `effect_ref` (`Family.Op` + optional args + reserved `Pure`). AST has `Item::Effect` and `EffectDef`/`EffectOperation`/`EffectParam`; lowering discards trivia but preserves spans.
- hir: `NodeKind::Effect` enters `AsgIndex`. The effect-declaration symbol table (`Family.Op → param shapes`, `AsgIndex::effect_ops`) allows name resolution to validate that `effects`/`allow`/`deny`/`exclude` references were declared and arities match (`UnresolvedEffect`). The built-in `Console` family + stdlib families `File`/`Http` are pre-seeded via Rust consts (`builtins::BUILTIN_EFFECT_OPS`) into the symbol table—`core` is zero-I/O and cannot bootstrap by parsing sources, so built-in/stdlib families are carried as Rust data (isomorphic to scalar/wrapper/built-in function tables; the only source of truth). User `effect` declarations are merged into the same table.
- semantic: effects normalize to `(family, op, args)`; args are differentiated as literals vs bindings (`EffectArg::{Lit, Binding}`). The effect layer enforces `used ⊆ declared`; capability matching uses `Effect::covered_by`—family/op must match; args compared positionally: literals must be equal (e.g., `Payment.Charge(1) ≠ Payment.Charge(2)`); if either side is a binding, it wildcards (runtime value is statically unknown; the capability grants that operation).
- runtime: effect runtime implementations are dispatched by `EffectHost` by family/op. Executable observable effects: built-in `Console.Write` (triggered by `print`) and stdlib `Http.Get` (body-level `Http.Get(url)`; `InMemoryHost` deterministic mock; real networking in `http_lib.md`); `File.Read/Write` with the `File` library (`file_lib.md`).
