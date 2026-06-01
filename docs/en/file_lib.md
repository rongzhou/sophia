# Standard Library · `File` (Local File Access)

> This document defines the complete design of the `File` standard library: the language contract (effects/types/capabilities/intent boundaries) and the real host. It is one of the libraries listed in §VI of `stdlib_design.md`; the overall framework for the standard library (prompt scaffolding, generic host-injection mechanism) is in `stdlib_design.md` / `stdlib_implementation.md`.
>
> Status: landed (2026-05-31). Motivation: local file access is a foundational I/O capability on par with networking (priority no lower than `Http`). After removing the semantically unclear `storage` node (see `engineering_notes.md`, 2026-05-31 decision), file read/write is provided as a library, the second landing under model (B) “I/O = libraries.” Design principles mirror `Http`: zero new syntax (isomorphic with `Console`/`Http`, reusing the effect/capability/intent trio), functionality library not a protocol stack (only read/write files; no low-level FS like perms/symlinks/mmap), and honest hosts (failures return `Err`, never fabricated). §2.6’s four decisions are adopted; both `File.Read` and `File.Write` are implemented, with the full chain (parse→check→run) + intent accept/reject tests passing.
>
> Library plugin refactor (2026-05-31): `File` has been moved out of hardcoded `core` and is now manifest-driven—the contract is declared in `sophia-stdlib/libs/file/library.toml` and injected via `LibraryRegistry`; the real host is registered into `HostRegistry` via `sophia-stdlib::register_native_hosts` (Plan B). The semantics described in §2.x/§3.x remain unchanged, but the landing points shifted from “`BUILTIN_EFFECT_OPS` + imperative `infer_effect_op` match + `EffectHost` methods + CLI `CliHost`” to “manifest op contracts + table-driven semantic checks + `HostRegistry` closures (native: `std::fs`)”. See `stdlib_design.md` / `stdlib_implementation.md` for the authoritative mechanism.

---

## I. Motivation and positioning

File read/write is a foundational capability for “serious programs” (config/data/logs). After removing `storage`, the demo needs for persistence/local state are handled by the `File` library (end-to-end validation in `e2e_test.md` G5-01: `File.Write` → `File.Read` → intent conversion round-trip using real temp files). `File` and `Http` form a pair: both obtain untrusted data (`Raw<Text>`) from outside and both enforce downstream usage via static intent boundaries—this extends the intent-safety argument beyond networking to local files (still “machine-checkable facts about what data has gone through”).

`File` vs `Http` symmetry:

| | `Http` | `File` |
| --- | --- | --- |
| Read | `Http.Get(url) -> Raw<Text>` | `File.Read(path) -> Raw<Text>` |
| Write | (v1 none) | `File.Write(path, content)` (see §2.3 decision) |
| Resource id | url (runtime-bound) | path (runtime-bound) |
| Returns untrusted | Network body is untrusted | File contents are untrusted (external source) |
| Capability granularity | `allow { Http.Get }` | `allow { File.Read }` / `allow { File.Write }` |

---

## II. Language contract

### 2.1 Syntax shape (zero new syntax; isomorphic to `Http`)

`File.Read` / `File.Write` are effect operations, reusing the “special-root method_call + host delegation” path (fully isomorphic to `Http.Get`):

```sophia
action LoadConfig {
  capability: FileCapability
  input  { path: Text }
  output { content: Sanitized<Text> }
  effects { File.Read }
  body {
    let raw = File.Read(path)        # method_call: base=File, method=Read, args=[path]
    let clean = Trust(raw)           # convert via intent_conversion to trusted
    return clean
  }
}
```

- `File` is a special root identifier (like `Http`/previously `storage`), allowed by HIR name resolution and not entered into the ASG index.
- Grammar/AST/lowering are unchanged—`File.Read(path)` is parsed by the existing `method_call` rule.

### 2.2 Operation set (minimal, on demand)

| op | signature | effect | note |
| --- | --- | --- | --- |
| `File.Read(path)` | `(Text) -> Raw<Text>` | `File.Read` | Read full file as untrusted `Raw<Text>` |
| `File.Write(path, content)` | `(Text, Sanitized<Text>) -> Unit` | `File.Write` | Write trusted text to a file (overwrite) |

- `File.Read` is a must (symmetric to `Http.Get`; local version of the intent-safety demo).
- `File.Write` is included with §2.6’s decisions: `content` must be `Sanitized<Text>`—you cannot write unprocessed `Raw` to disk; this models a write boundary akin to `Console.Write` accepting only literals/`Sanitized`/`Redacted`.
- Append/binary/dir traversal/delete/metadata are not predesigned; add on demand.

### 2.3 Effect layer

`File.Read`/`File.Write` appear in `BUILTIN_EFFECT_OPS` with arity=0 (effect identity carries no path arg, consistent with `Http.Get`; see §2.4). Declarations are `effects { File.Read }` / `allow { File.Write }`. “used ⊆ declared” and `Pure` conflicts are fully reused.

Note: arity=0 constrains declarations only; body calls `File.Read(path)` (with path) follow the `File` special-root method_call path (HIR `resolve_value_ident` allows it + semantic `infer_effect_op` checks `path: Text`), not the arity table.

### 2.4 Capability layer

`capability FileCapability { allow { File.Read; File.Write } }`. Effect identity is only `File.Read`/`File.Write` without path args—the same rationale as `Http.Get` (see `http_lib.md` §2.6): path is usually runtime-bound; treating it as an arg would be wildcarded by `covered_by`, yielding no control. Capabilities authorize “may read/write files.” Future “directory whitelist” constraints, if ever needed, belong to host policies, not language capabilities.

### 2.5 Intent boundaries (core; reuse existing checks; no changes required)

- `File.Read(path)` returns `Raw<Text>` (untrusted; external source)—directly using it as `Sanitized<Text>` triggers `IntentMismatch`; the only legal path is via an `intent_conversion: true` action.
- `File.Write(path, content)` requires `content: Sanitized<Text>`—you cannot write `Raw<Text>` directly (mirrors `Console.Write` boundaries; a write-out boundary).

This enables `File` to also form an accept/reject matrix entry (local-file version of intent safety), mirroring the `Http` flagship demo.

### 2.6 Confirmed decisions (2026-05-31; all adopted)

1. Include both `File.Read` and `File.Write` in v1 (D3 redo needs Write)—adopted.
2. `File.Read` returns `Raw<Text>` and `File.Write` accepts `Sanitized<Text>` (intent boundaries symmetric to Http/Console)—adopted.
3. Effect identity carries no path arg (capability granularity is “may read/write”)—adopted (consistent with Http).
4. Special root `File` (same as Http) as a body built-in—adopted.

---

## III. Hosts: mock and real files

### 3.1 Host interface

EffectHost trait adds methods (alongside `http_get`):

```rust
/// Read entire file (untrusted text). Failures (missing/perm/invalid UTF-8 etc.) → Err; hard-stop.
fn file_read(&mut self, path: &str) -> Result<String, String>;
/// Write file (overwrite). Failures → Err. (for `File.Write`)
fn file_write(&mut self, path: &str, content: &str) -> Result<(), String>;
```

### 3.2 InMemoryHost deterministic mock

`InMemoryHost` maintains an in-memory path→content bucket (`seed_file`), used for all deterministic tests:
- `file_read(path)`: hit → return; miss → `Err` (honest; no fabrication).
- `file_write(path, content)`: writes the in-memory bucket (no real FS), enabling read-after-write tests.

Mock nature is labeled “not real filesystem.”

### 3.3 Real host: CLI coordination layer `CliHost`

Real file I/O belongs to the coordination layer (CLI), not `runtime` (interpreter remains zero I/O). `CliHost` composes delegation—reusing `InMemoryHost` for console; overriding `file_read`/`file_write` to real `std::fs`:
- `file_read`: `std::fs::read_to_string(path)`, failures → `Err`.
- `file_write`: `std::fs::write(path, content)`, failures → `Err`.
- Injection predicate: the CLI’s `run` injects the real file host only when the entry action declares `File.Read`/`File.Write` (programs without files stay zero-overhead; see `stdlib_implementation.md` §III).

Note: Real file I/O is not part of `cargo test` (same strategy as real networking). Safety: the real host operates under restricted paths (documented; a future sandbox-root policy can be added).

---

## IV. Prompt assets

`File`’s LLM prompt asset is `workflow/prompt/assets/stdlib/file.md` (purpose / `File.Read(path) -> Raw<Text>` + `File.Write` ops / effect+capability / intent boundaries / neutral example). It follows `stdlib_design.md` §3.1. It is not included in the resident syntax baseline (injected on demand). During design, the library catalog `stdlib_catalog` lets the LLM choose (catalog adds a `file — local file read/write` line); during implement, the full asset is injected.

---

## V. End-to-end landing (implementation order)

| Step | Layer | Key changes |
| --- | --- | --- |
| F.1 | hir | Add `File.Read`/`File.Write` to `BUILTIN_EFFECT_OPS` (arity=0); allow special root `File` |
| F.2 | semantic | `infer_effect_op` recognizes `File.Read(path)`/`File.Write(path, content)`: validate path/content types; merge effect; return `Raw<Text>`/`Unit`; reuse intent boundaries |
| F.3 | runtime | `EffectHost::file_read`/`file_write`; `InMemoryHost` path→content mock bucket + `seed_file`; `interp::try_effect_op` recognizes `File.*` and delegates |
| F.4 | CLI host | `CliHost` overrides `file_read`/`file_write` to real `std::fs`; inject based on entry effects |
| F.5 | Assets + tests | `assets/stdlib/file.md` + `stdlib_catalog` line; semantic/runtime regressions + intent reject/accept; seam tests |
| F.6 | Docs | Finalize this doc; register in `stdlib_design.md` §VIII; add `File` family to language design/implementation effect tables |

---

## VI. Change log

- 2026-05-31 — Design-gate draft. `File` local-file library, isomorphic with `Http` (special-root method_call + effect/capability + intent boundaries; zero new syntax); `File.Read(path) -> Raw<Text>` (untrusted; must convert via intent) + `File.Write(path, Sanitized<Text>)` (write boundary). Mock host (`seed_file`) + real `std::fs` host (CLI coordination layer). Handles persistence needs after removing storage (D3 redo uses this). Pending confirmation of §2.6’s four decisions.
- 2026-05-31 — Landed (all four decisions in §2.6 adopted). HIR (add `File.Read/Write` to `BUILTIN_EFFECT_OPS` arity=0; allow special root `File`); semantic (recognize `File.Read`→`Raw<Text>` / `File.Write`→`Unit`; validate path:Text, content:Sanitized<Text>; reuse intent boundaries); runtime (`EffectHost::file_read/file_write` + `InMemoryHost` in-memory bucket + `seed_file` + `interp::try_effect_op` unifies File/Http); CLI host (`CliHost` overrides `file_read/write` to real `std::fs`; injection based on `File.*` effects); prompt assets `assets/stdlib/file.md` + `stdlib_catalog` entry). Regressions: semantic (clean / undeclared effect / reject direct Raw read / reject Raw write content) + runtime (write→read round trip / seed_file read / honest Err on missing file) + CLI seams (delegation / missing-file Err / real write-read round trip). Workspace: 336 passed / 0 failed. Real file I/O not in `cargo test`.
- 2026-05-31 — Demo acceptance (R3): D3 + e2e cases landed. `File` passes two integration demos: (i) benchmark L6 `archive_or_reject` (D3 serious pipeline combo: `File.Read` → validate via `one of` match → intent conversion → `File.Write` → `File.Read` read-back via mock host `seed_file` deterministically); (ii) e2e G5-01 note write and read back (self-contained write→read round trip with default host). Manual verification shows both are expressible and executable (D3 success returns Int 5 + reject returns `Rejected{amount}`; G5 returns Int 5). The benchmark `Problem` added `file_seed` (path→content mock shared by both modes, not in prompts); the baseline runner injects symmetric mock `file_read`/`file_write`. See `integration_demos.md` / `benchmark_design.md` / `e2e_test_design.md`. No changes to the library core (demos reuse the already-landed `File`).
- 2026-05-31 — Test triaging: end-to-end acceptance of `File` moved to e2e (using real I/O). Establish the three test categories (unit/e2e/benchmark); only unit tests may use mocks; e2e/benchmark must use real I/O. The prior D3 benchmark mock-file task `archive_or_reject` was removed (network/file tasks do not belong in benchmarks—no mocks; real I/O is uncertain/unfair). `File` end-to-end acceptance converges to e2e G5-01 (write→read round trip with intent conversion), now using real temp files (harness injects real `CliHost` when the entry declares `File.*`, not in-memory mock). The benchmark `Problem.file_seed` and runner’s mock `file_read`/`file_write` injections are both removed. For test organization, see `e2e_test.md` (G5-01) / `benchmark_test.md` (§I.4 no mocks) / `unit_test.md`. No changes to the `File` library code.
