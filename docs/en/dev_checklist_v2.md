# Sophia Engineering Progress · v2 (dev_checklist_v2)

> **v2 development tracking SSOT.** This document tracks v2: the prerequisite language extensions (`Text` / `while`) pulled by the JSON library demand, then a pure Sophia JSON third-party library, then an `Http` + JSON agent-like example. v0 / v1 progress is archived in `dev_checklist_v0.md` / `dev_checklist_v1.md`; cross-version engineering decisions continue to live in `engineering_notes.md`.
>
> v2 positioning is in `engineering_architecture.md` §14.3; the JSON library design draft is `json_lib_design.md`.
> Status: Completed / In Progress / Not Started / Deferred.

---

## I. Overview

**Phase goal (v2):** around the single main line “implement JSON validate / parse as a pure Sophia third-party library and connect it to an agent example,” add the necessary but minimal language capabilities so Sophia can process real external text data.

v2 is not primarily about adding more backends or starting the evolution subsystem; those directions move to v3+. The v2 value test is: once Sophia can deploy through WASM and interact with the outside world through `Http` / `File`, the next step is to prove it can turn external `Raw<Text>` into checkable structured semantics. JSON is the smallest sufficiently real test.

**v2 completion criteria:**

1. Minimal `Text` parsing capabilities (`char_at` / `slice`, optionally `starts_with`) land end to end and pass interpreter / WASM differential tests.
2. `while condition { ... }` lands end to end and passes interpreter / WASM differential tests.
3. The `json` third-party library can be discovered, checked, and executed by a project, with legal / illegal JSON hidden cases.
4. At least one `Http.Get → Raw<Text> → ValidateJson/ParseJson → domain action` agent-like example passes end to end.
5. The JSON MVP is implemented by default as a pure Sophia library; a host op is only a later performance / full-JSON option, not a v2 completion condition.

**Starting state:** v1 has landed WASM codegen, interpreter/WASM differential tests, the library plugin model, standard `File` / `Http` libraries, third-party discovery, and host-provider mechanics. `json_lib_design.md` exists as a draft and identifies the main missing pieces as `Text` operations and loop expressiveness.

---

## II. Work Items

### D0 — Design Freeze and Scope Narrowing

- [ ] **D0.1 Freeze the JSON MVP subset:** object / array / string / int / bool / null / whitespace; defer float / exponent / `\uXXXX` / JSON Schema.
- [ ] **D0.2 Freeze `Text` semantics:** decide Unicode scalar vs byte offsets; define out-of-bounds behavior; define `slice(start, length)` bounds behavior.
- [ ] **D0.3 Freeze `while` semantics:** syntax, scope, condition must be `Bool`, no `break` / `continue` in the MVP, no termination proof, runtime/WASM loop shape.
- [ ] **D0.4 Freeze JSON return model:** `ValidateJson` returns `one of { JsonValid, JsonInvalid }`; evaluate the recursive `JsonValue` model before parser work.
- [ ] **D0.5 Draft library asset:** write a `json.md` prompt asset covering capabilities, public actions, example calls, and the error model.

### F1 — Minimal `Text` Parsing Capability

- [ ] **F1.1 syntax / AST:** support `text.char_at(index)`, `text.slice(start, length)`, and, if approved by design review, `text.starts_with(prefix)`.
- [ ] **F1.2 HIR / semantic:** receiver must be `Text`; parameters must be `Int` / `Text`; return type is `Text` / `Bool`; diagnostics cover wrong receiver, arity, and parameter types.
- [ ] **F1.3 interpreter:** implement deterministic runtime semantics; out-of-bounds behavior must match D0.2.
- [ ] **F1.4 WASM codegen:** implement equivalent string-operation ABI / helpers.
- [ ] **F1.5 differential tests and docs:** cover normal characters, empty text, out-of-bounds, Unicode/byte boundaries per D0.2, and slice composition; update `language_design.md`, syntax baseline, and prompt assets.

### F2 — `while` Control Flow

- [ ] **F2.1 grammar / AST lowering:** add `while condition { ... }`, regenerate the tree-sitter parser, update CST / AST snapshots.
- [ ] **F2.2 HIR scope:** reuse block scope for the body; shadowing rules stay consistent; name resolution inside the condition works.
- [ ] **F2.3 semantic:** condition must be `Bool`; body type / effect / contract analysis joins the callable; flow analysis supports `return` / `raise` but does not prove loop termination.
- [ ] **F2.4 interpreter:** implement a synchronous loop and preserve honest runtime-error propagation.
- [ ] **F2.5 WASM codegen:** emit loop / branch structure and maintain case-by-case equivalence with the interpreter.
- [ ] **F2.6 differential tests and docs:** cover zero iterations, multiple iterations, state-driven early finish, nested `while`, return/raise inside `while`; update `language_design.md`, syntax baseline, and prompt assets.

### L1 — JSON Validator Third-Party Library

- [ ] **L1.1 fixture layout:** add `stdlib/tests/fixtures/sophia_libs/json/` with `library.toml`, `json.md`, and `src/*.sophia`.
- [ ] **L1.2 public API:** implement `ValidateJson`, input `Raw<Text>`, output `one of { JsonValid, JsonInvalid }`.
- [ ] **L1.3 internal parser state:** use `Text` + `while` to implement cursor state, whitespace skipping, and value/object/array/string/int validation.
- [ ] **L1.4 hidden cases:** cover `{}`, `[]`, `{"ok":true}`, nested object/array, missing closing bracket, trailing comma, unclosed string, invalid token, and trailing garbage.
- [ ] **L1.5 toolchain validation:** third-party discovery, `sophia check`, interpreter run, WASM run, and strip-assist artifact equivalence all pass.
- [ ] **L1.6 LLM generation evaluation:** use the Development Graph to record the `.pseudo → .sophia → hidden cases` success/failure path.

### L2 — Parser and Structured Access

- [ ] **L2.1 recursive data-model evaluation:** confirm whether recursive union/list fields are supported by checker, runtime, and WASM.
- [ ] **L2.2 parser MVP decision:** if recursive `JsonValue` is feasible, implement `ParseJson`; otherwise start with limited field extraction or a flat-object parser.
- [ ] **L2.3 structured return tests:** cover strings, integers, booleans, null, arrays, and object member reads; define the return or raise strategy for illegal JSON.
- [ ] **L2.4 intent boundary evaluation:** decide whether parser returns should carry “derived from validated JSON” intent information.

### E1 — HTTP + JSON Agent Example

- [ ] **E1.1 example goal:** define a real but stable agent-like case, such as fetching an HTTP API response, validating JSON, reading fields, and making a domain decision.
- [ ] **E1.2 capability / effect:** explicitly declare `Http.Get` and required capability; keep the `Raw<Text>` to trusted-structure intent boundary clear.
- [ ] **E1.3 graph / e2e path:** verify that the LLM can select `Http` + `json` from the design-stage catalog, receive the corresponding prompt assets during implement, and generate a passing candidate.
- [ ] **E1.4 record results:** record an accept/reject or success/failure matrix: whether the LLM misses intent conversion, misuses the JSON API, and which language capabilities block hidden cases.

### Deferred

- [ ] **JSON host op:** only a later performance or full-compatibility option; not in the v2 MVP.
- [ ] **Full JSON Schema:** evaluate after parser and structured access stabilize.
- [ ] **`break` / `continue`:** keep out of the `while` MVP unless the JSON library proves the absence creates substantial complexity.
- [ ] **Optional backends / evolution capabilities:** native / TS / Python emit, Evolution Boundary, Semantic Identity, etc. move to v3+.

---

## III. Verification

Each v2 step must be independently mergable and testable. Before merge, all must be green:

- Build: `cargo build --workspace`
- Test: `cargo test --workspace`
- Format: `cargo fmt --all -- --check`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`

Additional requirements:

- `Text` / `while` must enter interpreter and WASM differential tests.
- JSON library hidden cases must cover both legal and illegal inputs.
- Real-IO agent examples belong in e2e/examples, not in deterministic `cargo test` network-dependent paths.

---

## IV. Change Log

- 2026-06-04 — Establish v2 tracking document. v2 is positioned as a JSON third-party-library end-to-end phase: first add `Text` and `while` prerequisite language extensions, then implement a pure Sophia JSON validator/parser, and finally connect an `Http` + JSON agent-like example.
