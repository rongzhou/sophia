# JSON Library Design Draft

> Status: v2 design draft. This document records the goal, current constraints, and roadmap for “fill prerequisite language extensions (`Text` / `while`) around the JSON third-party-library demand, then run JSON validate / parse through to an agent example.”
> It is not yet a confirmed language specification. Before implementation, every required language capability should be split into an independent design review and engineering task.
>
> Related documents: `language_design.md`, `stdlib_design.md`, `stdlib_implementation.md`, `type_system.md`, `wasm_codegen.md`, `engineering_architecture.md` §14.3, and `dev_checklist_v2.md`.

---

## I. Goal

Sophia already has `File` / `Http` standard libraries and can obtain text from files or networks, but it still lacks the ability to turn external text into a checkable structure. JSON is the smallest representative next step: after `Http.Get(url)` returns `Raw<Text>`, if Sophia code can validate or parse JSON, it can express more realistic agent programs, for example:

1. fetch a response from an HTTP API;
2. validate that the response is JSON;
3. parse the required fields;
4. make a domain decision or call a downstream action based on those fields.

The core goals of this draft are:

- narrow v2 to one end-to-end main line: prerequisite `Text` / `while` language extensions → JSON third-party library → agent example;
- practice the library plugin model with a third-party library instead of putting JSON directly into the language core;
- test whether an LLM can implement a non-trivial parser / validator under Sophia’s explicit syntax and checker constraints;
- produce a usable `Http` + JSON + domain action example, moving Sophia from “executable examples” toward “programs that process real data.”

Priority: **validator first, parser later**; start with a verifiable minimal JSON subset before full JSON / schema.

---

## II. Why This Should Be a Third-Party Library

JSON is not a core language mechanism. The core language should provide deterministic primitive value operations, the type system, effects / capabilities, and intent mechanics. JSON syntax, error categories, parsing strategy, and data model are reusable functional units and belong in a library.

This matches the boundary in `stdlib_design.md`:

- pure logic should preferably be expressed as Sophia source libraries;
- library knowledge is supplied to the LLM through `library.toml` + prompt assets;
- library sources enter the index / semantic model / runtime as additional ASG inputs;
- third-party libraries are discovered under project-root `sophia_libs/` or `$SOPHIA_LIB_PATH`, without changing core.

If a JSON parser can ultimately be implemented in pure Sophia, it will become a more valuable third-party-library template than `hash_sophia`.

---

## III. Current State

### 3.1 Available Foundations

The project already has:

- third-party discovery: `sophia-stdlib::full_registry_for(project_root)` merges standard and third-party libraries;
- pure Sophia libraries: `[surface].sophia_sources` in `library.toml` can merge library `.sophia` files into the user program;
- library domain isolation: library source domain = library name, and HIR exempts user references to library nodes from cross-domain diagnostics;
- runtime execution: library actions / transitions enter the `SemanticModel` and `ExecGraph` alongside user actions;
- an existing third-party fixture: `stdlib/tests/fixtures/sophia_libs/hash_sophia`.

This shows that a “pure Sophia JSON library” is feasible across loading, indexing, checking, and execution.

### 3.2 Main Gaps

The current language cannot directly express a reliable JSON parser. The key gaps are concentrated in `Text` and loop expressiveness:

1. **`Text` lacks positional access**
   - Current capabilities are mainly literals, concatenation, equality, and `.length`.
   - A parser must read the character at index `i`, or extract a range.

2. **`Text` lacks slice / substring**
   - Parsing string literals, numbers, and object keys requires extracting text ranges.
   - Without slice, code must accumulate by concatenation, which is complex and inefficient.

3. **`Text` lacks character classification**
   - A JSON validator at least needs to classify whitespace, digits, quotes, backslash, and structural characters.
   - This can start with comparisons such as `char == " "`, but `char_at` must return single-character `Text`.

4. **Loops only support `repeat n times`**
   - JSON parsers are commonly written as “while cursor < length and condition holds.”
   - The starter subset can simulate this with `repeat text.length times` plus internal state, but that is cumbersome.
   - v2 treats `while condition { ... }` as an explicit prerequisite language-extension target, rather than waiting for repeated LLM failures.
   - This is not about syntactic sugar; it provides a direct, honest, checkable control-flow shape for cursor-style parsers.

5. **Recursive data models need careful validation**
   - A JSON value is recursive: objects / arrays can contain values.
   - Sophia has entities, lists, and `one of`, but recursive entity / recursive union support in checker, runtime, and codegen must be verified separately.

6. **The library-op signature DSL is not suitable for complex returns**
   - `TypeDesc` currently supports `Int` / `Bool` / `Text` / `Unit` and intent wrappers.
   - Pure Sophia libraries are not limited by `TypeDesc`; they can define their own entities / errors / actions.
   - If JSON parse starts as a host op, it immediately requires extending `TypeDesc`, so host-op-first should be avoided.

---

## IV. Route Choice

### 4.1 Recommended Route: Add `Text` + `while`, Then Implement a Pure Sophia Library

Recommended path:

1. add minimal deterministic `Text` value operations to the language core;
2. add `while condition { ... }` for cursor-style parsing loops;
3. implement a pure Sophia JSON validator using those operations;
4. extend that into a parser or limited structured access;
5. connect an `Http` demo that handles real data.

This maximizes validation of Sophia’s own expressiveness and avoids turning JSON into an early Rust / WASM host black box.

### 4.2 Not Recommended Initially: JSON as a Host Op

JSON parsing could be implemented as a `Json.Parse(text)` host op, but it is not suitable as the first step:

- it bypasses the validation goal of “LLM writes validator/parser”;
- it requires `TypeDesc` to support library-defined complex types, or else returns `Text` for a second pass;
- it hides the most valuable parser logic inside the host, weakening the case for the Sophia language itself.

A host op can remain a later performance or full-JSON compatibility option, but it should not be the MVP.

---

## V. Prerequisite Language Capabilities

### 5.1 Minimal `Text` Capabilities

Introduce these pure value capabilities first. They are not effects, do not need capabilities, and should behave like `Text.length` as core value operations with symmetric interpreter and WASM codegen support.

| Capability | Shape | Return | Purpose |
| --- | --- | --- | --- |
| character read | `text.char_at(index)` | `Text` | read one Unicode scalar or byte-sized character |
| slice | `text.slice(start, length)` | `Text` | extract a text segment |
| prefix check | `text.starts_with(prefix)` | `Bool` | simplify fixed-token checks; optional |

The MVP can start with `char_at` + `slice`. `starts_with` can be implemented by library code, but as a primitive it would significantly reduce LLM generation difficulty.

Boundaries to decide:

- Indexing should use Unicode scalar or UTF-8 byte offset. Current `.length` is Unicode scalar count; for consistency, `char_at` / `slice` should prefer Unicode scalar indexing.
- Out-of-bounds behavior. Returning `""` versus a runtime error needs design review. Parser code benefits from empty text because it avoids repeated pre-access branching; Sophia generally favors honest errors, so this needs a tradeoff decision.
- WASM codegen must support the same semantics; interpreter-only support is not acceptable.

### 5.2 `while` Control Flow Target

v2 explicitly adds `while condition { ... }` as a prerequisite language extension for the JSON library. The old substitute is:

```sophia
let mutable cursor = 0
repeat input.length times {
  if cursor < input.length {
    // inspect input.char_at(cursor)
    // set cursor = cursor + 1
  }
}
```

This is expressive, but it buries parser logic inside “fixed-count loop + internal if + manual stop state,” increasing LLM generation mistakes and human review cost. The v2 goal for `while` is not complex concurrency or async semantics; it is a synchronous, deterministic, direct loop form.

Suggested syntax:

```sophia
while cursor < input.length {
  let ch = input.char_at(cursor)
  set cursor = cursor + 1
}
```

Design boundaries:

- the condition expression must be `Bool`;
- the body reuses `repeat` block semantics, scope rules, and `return` / `raise` flow analysis;
- no `break` / `continue` in the MVP; early finish is expressed by changing state used by the loop condition;
- runtime and WASM codegen both implement a synchronous loop;
- the checker verifies type/effect legality but does not prove termination.

---

## VI. JSON Library MVP Scope

### 6.1 First Phase: Validator

The first phase only validates whether text is JSON; it does not return a JSON AST.

Suggested public action:

```sophia
action ValidateJson {
  input { text: Raw<Text> }
  output { result: one of { JsonValid, JsonInvalid } }
  body { ... }
}
```

Suggested library definitions:

- `entity JsonValid { ... }`
- `entity JsonInvalid { reason: Text; position: Int }`
- If hard errors are needed, define `error JsonParseError`, but a validator should normally return a `one of` result rather than turn ordinary invalid input into runtime failure.

MVP JSON subset:

- object: `{}`
- array: `[]`
- string: double-quoted strings, with common escapes first;
- int: decimal integers;
- bool: `true` / `false`;
- null: `null`;
- whitespace: space, newline, carriage return, tab.

Defer:

- decimals;
- exponents;
- `\uXXXX`;
- JSON Schema;
- full key extraction.

### 6.2 Second Phase: Parser

The second phase returns a structured JSON value.

Initial model:

```sophia
entity JsonString { fields { value { type: Text } } }
entity JsonInt { fields { value { type: Int } } }
entity JsonBool { fields { value { type: Bool } } }
entity JsonNull { fields { value { type: Unit } } }
entity JsonMember { fields { key { type: Text } value { type: JsonValue } } }
entity JsonArray { fields { items { type: list of JsonValue } } }
entity JsonObject { fields { members { type: list of JsonMember } } }
```

`JsonValue` here is only a conceptual name. Sophia currently has no type aliases. To express a recursive `one of`, we must first confirm whether entity fields can directly contain recursive unions, or whether the MVP needs a more explicit non-recursive shape.

Therefore the parser phase must start with a “recursive data-model feasibility evaluation.”

### 6.3 Third Phase: HTTP Agent Example

After validator / parser is usable, add an end-to-end example:

1. `Http.Get(url)` obtains `Raw<Text>`;
2. call `ValidateJson` or `ParseJson`;
3. make a domain judgment over the parsed result;
4. return a structured entity or domain error.

This example proves Sophia can process real API responses, not only toy arithmetic / todo flows.

---

## VII. Verification Strategy

### 7.1 Deterministic Tests

The JSON library should first enter tests as a third-party fixture:

```text
stdlib/tests/fixtures/sophia_libs/json/
  library.toml
  json.md
  src/*.sophia
```

Tests should cover:

- third-party discovery;
- `sophia check` with library sources merged;
- interpreter execution of the validator;
- WASM backend equivalence with the interpreter;
- strip-assist equivalence unaffected by library sources.

### 7.2 Case Set

Starter validator cases:

- `{}`
- `[]`
- `{"ok":true}`
- `{"items":[1,2,3]}`
- `{"nested":{"a":null}}`
- missing closing bracket;
- trailing comma;
- unclosed string;
- invalid token;
- trailing garbage after legal JSON.

### 7.3 LLM Generation Capability Evaluation

One core value of this library is to evaluate the LLM’s ability to write a validator/parser. Recommended Development Graph route:

1. a human writes the goal and boundaries clearly;
2. the LLM generates `.pseudo`;
3. `.pseudo` is implemented into `.sophia`;
4. hidden cases gate the candidate;
5. preserve failure paths and analyze which language capabilities or prompt assets caused failure.

This better matches Sophia’s research goal than directly hand-writing the final library.

---

## VIII. Risks and Open Questions

1. **`Text` indexing semantics**
   - Unicode scalar or byte offset?
   - Out-of-bounds returns empty `Text` or hard error?

2. **`while` syntax details**
   - Does the MVP need `break` / `continue`? Current recommendation: no.
   - Is termination only a runtime responsibility? Current recommendation: the checker does not prove termination.
   - Are effectful action calls allowed in the condition? Current recommendation: reuse existing expression effect rules; do not open a special hole.

3. **Recursive JSON value**
   - Do current type/check/runtime/codegen layers accept recursive entities / unions?
   - If not, should the parser MVP return specific field extraction results instead of a full AST?

4. **Intent boundary**
   - Should `ValidateJson` convert `Raw<Text>` into `Validated<Text>`?
   - If the parser returns structured values, do we need to express “this structure came from validated JSON”?

5. **Error model**
   - Is illegal JSON returned as `JsonInvalid`, or raised as `JsonParseError`?
   - Recommendation: validator returns invalid input as a value; parser can raise when recovery is impossible.

6. **Whether it enters the standard library**
   - Keep it as a third-party library initially to practice the plugin mechanism.
   - If many examples later depend on it, evaluate promotion to the standard library.

---

## IX. Suggested Roadmap

### R0: Design Freeze

- define the JSON MVP subset;
- define `Text` primitive semantics;
- define `while` syntax, scope, termination stance, and codegen shape;
- define the validator return model;
- draft the library prompt asset.

### R1: Land `Text` Primitives

- syntax / lower support for `text.char_at(index)` and `text.slice(start, length)`;
- semantic signature checks;
- interpreter implementation;
- WASM codegen implementation;
- differential tests to guarantee interpreter/WASM consistency.

### R2: Land `while` Control Flow

- syntax / lower support for `while condition { ... }`;
- HIR scope and name resolution reuse block rules;
- semantic checks that the condition is `Bool`, and body type / effect / flow analysis is included;
- interpreter and WASM codegen implement synchronous loops;
- differential tests cover zero iterations, multiple iterations, state-driven early finish, return, and raise.

### R3: JSON Validator Third-Party Library

- add a `sophia_libs/json` fixture;
- use the LLM to generate or help generate `.pseudo` and `.sophia`;
- cover legal / illegal JSON hidden cases;
- manual CLI smoke: project-root discovery and execution.

### R4: HTTP + JSON Agent Example

- write an agent-like example using `Http.Get` + `ValidateJson`;
- verify capability / effect declarations are complete;
- record whether the LLM can select and use the JSON library from catalog / assets.

### R5: Parser and Structured Access

- evaluate recursive JSON value model;
- if recursive model is feasible, implement `ParseJson`;
- if recursive model is not yet feasible, implement limited field extraction or a flat-object parser first;
- then decide whether to extend type aliases / recursive unions / richer `Text` API.

---

## X. Current Judgment

“Implement JSON validator/parser as a pure Sophia third-party library” is reasonable and valuable, but it is not immediately implementable with the current language surface. It should drive v2 requirements: first add the minimal `Text` capabilities and `while` control flow, then use a third-party library and the Development Graph to evaluate the real ability of LLMs to write parsers.

The best first step is not to write the JSON parser directly, but to complete R0/R1/R2: add the minimal, deterministic, codegen-supported `Text` operations and `while` control flow needed by the parser.
