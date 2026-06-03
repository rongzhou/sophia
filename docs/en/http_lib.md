# Standard Library · `Http` (Network Fetch)

> This document defines the complete design of the `Http` standard library: the language contract (effects/types/capabilities/intent boundaries) and the real host. It is the first library listed in §VI of `stdlib_design.md`; the overall framework for the standard library (prompt scaffolding, generic host-injection mechanism) is in `stdlib_design.md` / `stdlib_implementation.md`.
>
> Status: landed (2026-05-30). Origin: demo need D2 (network fetch + intent safety, flagship LLM-native demo). Both the language contract and the real host are implemented; the full chain (parse→check→run) + accept/reject tests pass.
>
> Library plugin refactor (2026-05-31): `Http` has been decoupled from hardcoded `core` and is now manifest-driven—the contract is declared by `sophia-stdlib/libs/http/library.toml` and injected into each layer via `LibraryRegistry`; the real host is registered into `HostRegistry` via `sophia-stdlib::register_native_hosts` (Plan B). The semantics of effects/types/intents/host described in §2.x/§3.x below are unchanged, but the implementation pivoted from “BUILTIN_EFFECT_OPS + imperative infer_effect_op match + EffectHost::http_get method” to “manifest op contract + table-driven checks in the type layer + HostRegistry closures.” The authoritative mechanism lives in `stdlib_design.md` / `stdlib_implementation.md`.
>
> Design principles: zero new syntax—`Http` is isomorphic to existing `Console`, reusing the `(family, op, args)` effect triple + capability + intent mechanisms; a functionality library, not a protocol stack (only implements `Http.Get`, not TCP/TLS); hosts must be honest (never fabricate network success).

---

## I. Demo goal (D2 flagship)

Landing the flagship claims of the tech report §7/§8: turn “what the data has gone through” into machine-checkable language facts, forming a real accept/reject matrix entry:

- Sophia (accept positive + reject negative): Data returned by `Http.Get(url)` has type `Raw<Text>` (untrusted). If a downstream directly uses it as a trusted value (writing across a boundary requiring `Sanitized<T>`, or assigning to a `Validated<T>` field) → the checker statically rejects it (intent equality is strict; `language_design.md` §7.2). Only after explicit intent-conversion actions may it be used → accept.
- Baseline (TS + tsc): `fetch(url)` returns `string`; using it directly as trusted passes tsc (no intent concept in the type system).

This is the strong argument for “why an LLM-focused language/workflow”: for the same unsafe pattern, Sophia intercepts at compile time while mainstream languages allow it. End-to-end validation is in `e2e_test.md` (G2-03 network fetch + intent safety, real site) + deterministic matrix `cli/tests/intent_matrix.rs` (reject half statically rejected; see `unit_test.md`).

---

## II. Language contract

### 2.1 Syntax shape (zero new syntax; isomorphic to storage)

`Http.Get` is an effect operation: it both registers a side effect (for effect/capability checks) and returns a value (`Raw<Text>`). This is fully isomorphic to storage operations at body level where `storage.X.get(key)` returns a value and contributes to `DB.Read`—reuse the existing “special root + method call” path, with no new syntax:

```sophia
action FetchProfile {
  capability: NetCapability
  input  { url: Text }
  output { body: Raw<Text> }
  effects { Http.Get }
  body {
    let raw = Http.Get(url)        # method_call: base=Http, method=Get, args=[url]
    return raw                     # raw : Raw<Text>
  }
}
```

- Parse shape: `Http.Get(url)` is parsed by the existing `method_call` rule as `MethodCall { base: Ident("Http"), method: "Get", args: [url] }`—same shape as `storage.Todos.get(k)` (the only difference is the base: bare `Http` ident vs `storage.<Name>` field access). Grammar/AST/lowering need zero changes.
- `Http` is a special root identifier (like `storage`), allowed by HIR name resolution (no “undeclared” error) and not entered into the ASG index.

### 2.2 Why not a top-level `effect Http {...}` user declaration

`Http` is a built-in effect family (listed with `Console`/`DB` in `hir::builtins::BUILTIN_EFFECT_OPS`), not a user-declared `effect`. Reasons: (i) it has a fixed host semantics (real network calls), not a domain-defined contract; (ii) its return type (`Raw<Text>`) is a language-built intent convention requiring special-casing at the type layer (similar to storage get returning `one of {V, Null}`). User-declared `effect` operations have no return-value binding semantics, so they are not suited to representing “fetch untrusted data.”

### 2.3 Operation set (minimal, on demand)

| op | signature | effect | note |
| --- | --- | --- | --- |
| `Http.Get(url)` | `(Text) -> Raw<Text>` | `Http.Get` | GET returns the response body as untrusted `Raw<Text>` |

Only `Http.Get` (minimum needed for D2). We do not predesign `Http.Post`/headers/status/JSON parsing, etc.—introduce if/when needed by demos. The `url` parameter is `Text` (a scalar in the starting subset; no `Url` specialized type is introduced as there is no need).

> Tradeoff record: `Http.Get` returns `Raw<Text>` instead of `one of { Raw<Text>, HttpError }`. Rationale: D2 focuses on intent safety (untrusted data under type control), not on modeling network failures; mapping network failures to domain errors belongs to host error semantics and can be extended later, if demos require, to `one of {...}` (F1 is ready, zero language change then). Currently failures return `RuntimeError` (a hard error), never fabricating success nor swallowing errors.

### 2.4 Type layer

Inference for `Http.Get(url)` lives in the type layer’s effect-op path (formerly `infer_effect_op`, now table-driven via registry):

- Recognize `MethodCall { base: Ident("Http"), method: "Get", args: [url] }`.
- Check `args.len() == 1` and infer `url` as `Text` (otherwise emit `TypeMismatch`).
- Merge effect `Http.Get` (no args—see capability in §2.6).
- Return type `Ty::Intent(IntentKind::Raw, Box::new(Ty::Text))`.

Downstream strict-intent-equality checks (already implemented) require no changes to intercept “using `Raw<Text>` directly as `Sanitized<Text>`”—this is precisely the D2 reject interception point.

### 2.5 Effect layer

`Http.Get` appears in `BUILTIN_EFFECT_OPS` as `("Http", "Get", 0)` with arity=0—the effect identity carries no URL arg (see decision 1 in §2.6); declarations `effects { Http.Get }` / `allow { Http.Get }` mirror `Console.Write`’s zero-arg form.

The “used ⊆ declared” and “conflict with `Pure`” checks are fully reused—if the body uses `Http.Get` it must be declared in `effects { Http.Get }` or we raise `UndeclaredEffect`.

Note: arity=0 constrains only declaration positions (the `effects {}`/`allow {}` effect references, validated by HIR `resolve_effect`). The body call `Http.Get(url)` (with url argument) goes through the special-root method_call path (HIR `resolve_value_ident` allows it + semantic checks validate `url: Text`), not via `resolve_effect`’s arity table—so arity=0 does not conflict with a call carrying url at body level.

### 2.6 Capability layer

`capability NetCapability { allow { Http.Get } }`. Capability matching reuses `Effect::covered_by`—key decision: the effect identity is only `Http.Get`, without the URL argument. That is, the registered effect is `Http.Get` (no arg); writing `allow { Http.Get }` suffices to authorize it.

Why not include the URL as an effect arg (contrasting storage’s `DB.Read("Todos")` that takes the store name as an arg)? The storage name is a statically known resource identifier (capability can target a specific table); URLs are typically runtime-bound (input params), unknown statically—if treated as an arg it degenerates into a wildcard in `covered_by`, providing no practical control and making capability declarations unstable. Therefore `Http.Get`’s identity carries no arg—capability authorizes the ability “may perform HTTP GET,” which matches capability’s granularity. If we need a domain whitelist in the future, that’s a host-level policy, not a language-level capability.

### 2.7 Intent boundaries (core of D2; already implemented; no changes required)

- `Console.Write` only accepts literals / `Sanitized<T>` / `Redacted<T>` → directly printing the result of `Http.Get(url)` is rejected.
- Assigning to `Sanitized<Text>`/`Validated<Text>` fields or outputs → strict intent equality rejects it.
- The only legal path is via an `intent_conversion: true` action (e.g., `Sanitize(Raw<Text>) -> Sanitized<Text>`).

### 2.8 Confirmed decisions (2026-05-30)

1. `Http.Get` carries no URL as an effect arg (capability granularity is “may GET”; §2.6)—adopted.
2. `Http.Get` returns bare `Raw<Text>` rather than `one of { Raw<Text>, HttpError }` (network failures remain hard runtime errors in v1; §2.3)—adopted.
3. Special root `Http` alongside `storage` as a body built-in root—adopted.

---

## III. Hosts: mock and real network

### 3.1 Host interface (contract independent of specific hosts)

EffectHost trait method:

```rust
/// Handle `Http.Get(url)`: return the response body (untrusted text).
/// Returns Ok(body) or Err (network/I/O failure; hard error; never fabricate success).
fn http_get(&mut self, url: &str) -> Result<String, String>;
```

The interpreter recognizes the `Http.Get` special root during `MethodCall` evaluation (alongside `try_storage_op` there is `try_effect_op`), delegates to `host.http_get(url)`, and wraps the returned text as `Value::Text(body)` (runtime does not carry intent tags—intent is a static, compile-time attribute; runtime only preserves structure). The generic injection mechanism for hosts (`run_action` / composing hosts / effect-based injection) is described in `stdlib_implementation.md` §III.

### 3.2 InMemoryHost deterministic mock

`InMemoryHost` provides a deterministic mock and does not perform real networking:

- Maintains a `BTreeMap<String, String>` (url → preset response) pre-seeded via `seed_http` by tests/harness.
- `http_get(url)`: on preset hit returns the preset; on miss returns `Err` (hard error; honest—never invent a “default success”).
- The mock nature is clearly labeled “not real network” in host docs and traces.

The mock host is used for all deterministic tests (including D2’s benchmark accept tasks and integration demos). Real networking always uses §3.3.

### 3.3 Real host: CLI coordination layer `CliHost`

The real networking host belongs in the coordination layer (CLI), not in `runtime` (the interpreter remains zero-I/O). `CliHost` composes by delegation—console/storage reuse `InMemoryHost`; only `http_get` is overridden to real `reqwest::blocking`:

```rust
fn http_get(&mut self, url: &str) -> Result<String, String> {
    let resp = self.http.get(url).send().map_err(|e| format!("Http.Get request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Http.Get non-2xx status: {status}"));   // honest failure; do not count as success
    }
    resp.text().map_err(|e| format!("Http.Get failed to read body: {e}"))
}
```

- Synchronous: use `reqwest::blocking` (matches the sync signature of `http_get`; the `run` command is synchronous; no need to thread a tokio runtime through the interpreter). Enable `blocking` feature on workspace `reqwest` (used only by the CLI).
- Timeouts: set a fixed client timeout (e.g., 10s) to avoid hangs; timeouts → `Err` (honest failure).
- Error mapping (current stage): network failures / non-2xx / body-read failures all → `Err(String)`, materialized by the interpreter as `RuntimeError` (hard-stop). Do not map network failures into domain `one of {..., HttpError}` at this stage—consistent with §2.3; if we need recoverable failures later, extend `Http.Get`’s return type via F1 (language-layer change via the design review).
- Injection predicate: the CLI’s `run` uses `InMemoryHost` by default; only when the entry action’s `declared_effects` includes `Http.Get` do we construct a `CliHost` and inject real networking—programs without networking incur zero overhead and zero behavior change (see `stdlib_implementation.md` §3.2).

### 3.4 Real-host decisions (2026-05-30)

1. Real host lives in the CLI (coordination layer), composing/delegating to reuse `InMemoryHost` (runtime stays zero-I/O)—adopted.
2. Sync `reqwest::blocking` with fixed timeout (do not thread tokio into the interpreter)—adopted.
3. Network failures map to hard `RuntimeError` (not modeled as recoverable `one of` returns; consistent with §2.3)—adopted.
4. CLI `run` injects `CliHost` only if the entry action’s `declared_effects` contains `Http.Get` (zero overhead otherwise)—adopted.

---

## IV. Consistency check with existing mechanisms

| Mechanism | storage (existing) | `Http` (this lib) | Reused? |
| --- | --- | --- | --- |
| Call shape | `storage.X.get(k)` (special-root method_call) | `Http.Get(url)` (special-root method_call) | Yes; isomorphic; zero new syntax |
| Effect registration | `DB.Read("X")` (arg=store name) | `Http.Get` (no arg; see §2.6) | Yes; same triple mechanism |
| Return special-casing | `infer_storage_op` → `one of {V,Null}` | `infer_effect_op` → `Raw<Text>` | Yes; peer special-casing |
| Host delegation | `storage_get/save` | `http_get` | Yes; same trait extension |
| Capability | `allow { DB.Read("X") }` | `allow { Http.Get }` | Yes; `covered_by` reused |
| Honesty | in-memory bucket (non-persistent) | in-memory mock / real network failures as hard errors | Yes; same honesty discipline |

We add only “one built-in effect triple + one host method + one type-layer special-case + one prompt asset + one CLI real host”—small surface increase, large explanatory power; aligns with the strong-justification admission bar for standard libraries.

---

## V. Prompt assets

`Http`’s LLM prompt asset is `workflow/prompt/assets/stdlib/http.md` (purpose / `Http.Get(url) -> Raw<Text>` operation / effect+capability / `Raw<Text>` intent boundary / neutral example). It follows the structure in `stdlib_design.md` §3.1 and does not go into the resident syntax baseline (library knowledge is injected on demand; see `stdlib_design.md` §3). Injection mechanism (design sees the `stdlib_catalog` and chooses; implement injects `stdlib_preamble(libraries)`) is in `stdlib_implementation.md` §II.

---

## VI. Change log

- 2026-05-30 — `Http` language contract landed: built-in effect family, reuse the storage “special-root method_call + host delegation” path (zero new syntax); `Http.Get(url) -> Raw<Text>`, with intent boundary intercepting untrusted data (D2 reject case). Mock host is honest (miss → `Err`). Three decisions in §2.8 adopted; HIR/semantic/runtime implemented end-to-end + 9 tests.
- 2026-05-30 — Real host landed: `CliHost` in the CLI coordination layer composes by delegating to `InMemoryHost`, overriding only `http_get` to real `reqwest::blocking` (fixed timeout + honest errors). Runtime exposes `run_action` as the injection seam. CLI `run` injects based on the entry’s `Http.Get` effect. Four decisions in §3.4 adopted. Seam tests cover delegation equivalence/injection path/effect detection; real networking is not part of `cargo test`.
