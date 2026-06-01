# F1 Design: Unified Type Syntax — `of` keyword family + `<>` exclusive to Intent

> Status: design finalized (decision locked on 2026-05-30), pending implementation. Corresponds to `dev_checklist_v1.md` workflow B · F1, originating from demo needs D1 (modeling fallible results) / D3 (serious pipelines). This document fixes the unified rules for the entire type syntax and their end-to-end landing points. Single path, no legacy compatibility, no “syntactic sugar” excuses—this is a one-off, thorough refactor at the v1 starting phase.
>
> Design tenets (`language_design.md` §1.1 / §3): strong semantics, few symbolic conventions, no exceptions; do not imitate generics/templates/macros; semantics should be intuitive, explicit, with no elisions, not fearing verbosity.

---

## I. The one core rule (no exceptions)

> `Wrapper<T>` forms belong exclusively to Intent Types. All structural types are expressed with the `of` keyword family.

This rule eliminates v0’s implicit exceptions (where `List<T>` / `Optional<T>` borrowed `<>` yet were not intents). From now on, `<>` means only one thing—intent wrappers; structural types all use readable keyword forms. This delivers “strong semantics, few conventions, no exceptions”: one rule, zero special cases. LLMs need not memorize “which `<>` is for intent and which is for containers.”

### 1.1 The `of` keyword family (structural types)

| Type | Syntax | Meaning |
| --- | --- | --- |
| list | `list of T` | Homogeneous list of elements (replaces `List<T>`) |
| union | `one of { A, B, ... }` | One of multiple mutually exclusive outcomes (replaces `Optional<T>` and expresses fallible returns) |
| gradual | `schema of T` | Gradual type (replaces `Schema<T>`) |

`Unknown` remains a bare keyword (no parameters, unchanged). In the future, if truly needed, extend with `set of T` / `map of K to V`, etc., in the same family—but introduce on demand; do not predesign.

### 1.2 `<>` exclusive to Intent Types (unchanged)

Eight intent wrappers keep the `<>` form, and only they use `<>`:
`Raw<T>` / `Parsed<T>` / `Validated<T>` / `Sanitized<T>` / `Verified<T>` / `Authorized<T>` / `Secret<T>` / `Redacted<T>`. Intents remain strictly one layer and strictly equal (see language design §7.2; unchanged).

### 1.3 `Null` — built-in single-value type for “absence”

Introduce a built-in type `Null` (single value, similar to `Unit` but semantically “absent/no result”). It mainly serves as a member in `one of` to express nullability: `one of { Todo, Null }` = “either a Todo or none.” `Null` is a bare type keyword and may appear in any type position (not limited to inside `one of`, though that is typical). The literal is written `Null`.

---

## II. `one of` union types (core for D1)

### 2.1 Form and semantics

`one of { M1, M2, ... }`: at runtime the value is exactly one of the members. Members can be scalars (`Int`/`Bool`/`Text`/…), `Null`, declared entities/states, or error variants (reusing the existing error algebra variants).

```sophia
action Withdraw {
  input  { balance: Int; amount: Int }
  output { result: one of { Int, InsufficientFunds } }   # enumerate outcomes directly, no wrapper
  body {
    if amount > balance {
      return InsufficientFunds { shortfall = amount - balance }   # return failure directly, no Err()
    }
    return balance - amount                                       # return success directly, no Ok()
  }
}
```

- No wrapper constructors: Success returns `return <Int value>`; failure returns `return <Variant { ... }>` directly. There is no `Ok`/`Err`/`Some`/`None`-like wrapper—the member is itself. This is the key difference from Rust’s `Result`/`Option`: Sophia already has named variants and a tagged union; there’s no need for an extra generic container layer.
- Fallible returns vs `raise`: `one of {..., SomeError}` denotes recoverable failures—it is a return value that the caller must handle explicitly. `raise` remains the channel for unrecoverable failures that auto-propagate (`errors {}` + `raise`, unchanged). A variant may be returned as a `one of` member by one action and be `raise`d elsewhere—the difference lies in how it is handed off.
- Error variants returned do not need to appear in `errors {}`: they are returned, not raised; the two channels are orthogonal. `errors {}` only constrains `raise` propagation.

### 2.2 Distinguishability (members must be tag-distinguishable)

Members of a `one of` must be pairwise distinguishable by match tags, otherwise the checker errors:
- Scalars are distinguished by type name (`Int`/`Bool`/`Text`/…);
- Entities/states by their names;
- Error variants by the variant name;
- `Null` is the unique literal.

Therefore `one of { Int, Int }` or `one of { Int, Text }` have scalar members that are distinguishable (by type), but `one of { Todo, Todo }` or two members of the same type are not distinguishable → error. (Error variants always differ in name and are naturally distinguishable; the constraint mainly applies to multiple non-error members of the same type.)

### 2.3 `one of` replaces `Optional`

`Optional<T>` is removed. “Nullable” is uniformly written as `one of { T, Null }`. The `Some(x)`/`None` expressions and patterns, and the `<optional>.exists` pseudo-field are removed together (single path, no compatibility layer).

---

## III. match: type patterns (new mechanism, honest annotation)

In v0, `match` only handled Bool/state-value/Some-None. `one of` requires that `match` dispatch by member tag—this is new:

```sophia
match Withdraw(b, a) {
  Int remaining                   => return remaining            # match Int member, bind remaining
  InsufficientFunds { shortfall } => return 0 - shortfall        # match variant, bind fields
}
```

```sophia
match find_todo(id) {            # returns one of { Todo, Null }
  Todo t => return t.status
  Null   => raise NotFound { id = id }
}
```

Pattern forms:
- Type pattern `<TypeName> <binding>`: match that type member and bind the value to `binding` (scalars/entities/states).
- Variant pattern `<VariantName> { f1, f2, ... }`: match that error variant and bind by field name (reuses existing error-variant field binding form).
- `Null`: matches the `Null` member, no binding.
- State-value pattern `StateName.Value`: when the member is a state, further match by value (reusing existing state match).
- Bool literals `true`/`false`: when the `match` subject is `Bool` (existing, unchanged).

Exhaustiveness (permanent ban on `_`, unchanged): Matching a `one of` must cover all members; matching a `Bool` must cover `true` and `false`; matching a state must cover all values. Missing a member yields a `NonExhaustiveMatch` diagnostic.

> This is the only new-mechanism cost in this design: `match` evolves from “a few fixed subjects” to “dispatch by `one of` member tags + type-pattern binding.” The semantics are intuitive (the pattern reads as what it matches), and it is an honest, necessary addition.

---

## IV. Unified type vocabulary (complete set)

```
Scalars:   Unit | Bool | Int | Text | Uuid | Time | Null
Structural: list of T | one of { M, ... } | schema of T | Unknown
Intent:     Raw<T> | Parsed<T> | Validated<T> | Sanitized<T> | Verified<T>
            | Authorized<T> | Secret<T> | Redacted<T>
Named:      Declared entity/state/error variant
```

`<>` ⇔ intent, `of` ⇔ structural, bare names ⇔ scalars/gradual/named. One rule covers all, with no exceptions.

---

## V. Deletions list (single path, completely removed, no compatibility)

| Removed | Replaced by |
| --- | --- |
| `List<T>` syntax + `Ty::List` parsing via `<>` | `list of T` |
| `Optional<T>` syntax + `Ty::Optional` | `one of { T, Null }` |
| `Some(expr)` expression / `Some(x)` pattern | Direct member construction / type patterns |
| `None` expression / pattern | `Null` literal / pattern |
| `Schema<T>` syntax | `schema of T` |
| `<optional>.exists` pseudo-field | In predicate contexts, use `!= Null`; in bodies, use `match ... { Null => ... }` |

`Value::Optional` → replaced by the runtime representation of `one of` (see §VI). The `<text>.length` pseudo-field is retained (unrelated to this change).

---

## VI. End-to-end landing points (implementation order F1.1–F1.6)

| Step | Layer | Key changes |
| --- | --- | --- |
| F1.1 | syntax (grammar + parser.c + AST + lower) | New `list_of` / `one_of` / `schema_of` type rules; remove `generic_type`’s handling of List/Optional/Schema (`generic_type` keeps only intents); remove `some_expr`/`None`; add `Null` literal; extend match patterns with type patterns; AST: `TypeRef` adds `ListOf`/`OneOf`/`SchemaOf`, `Expr` removes Some/None and adds `Null`, `Pattern` removes Some/None and adds `Type{ty_name,binding}`/`Null` |
| F1.2 | hir | Name resolution: resolve each `one of` member (types/variants); `Null` is built-in; remove wrapper names for List/Optional/Schema; bind type-pattern variables into scope |
| F1.3 | semantic | Add `Ty::OneOf(Vec<Ty>)` / rename to `ListOf` and `SchemaOf`; remove `Ty::Optional`; check `one of` distinguishability; extend match exhaustiveness to `one of` members; determine ownership of type-pattern members and binding types; assignability rules (`one of` subset/member compatibility, see §VII) |
| F1.4 | runtime | Remove `Value::Optional`; a `one of` value is directly the member’s own `Value` (Int is `Value::Int`, a variant is an error value with a tag)—no wrapper variant; match dispatches by the value’s actual tag; `Null` → `Value::Null` |
| F1.5 | I/O libs | Library read operations return `one of { ValueTy, Null }` (hit → the value itself; miss → `Null`, must `match` to extract). This is the typical use of `one of` in the standard library (e.g., future `File`/`DB` lookups). [Note: when F1 landed this rule was validated on the then-existing `storage` node; `storage` has since been removed, see `stdlib_design.md`.] |
| F1.6 | Repo-wide rewrite + tests + docs | Shared syntax baseline, example `.sophia`, e2e/benchmark cases, snapshots, and synchronized updates to `language_design`/`language_implementation` |

Key runtime simplification: `one of` does not need a new wrapper `Value` variant. A `one of { Int, InsufficientFunds }` at runtime is simply `Value::Int(...)` or an error value with a variant tag—the member is itself. Matching dispatches by the actual shape of the value. This is leaner than Rust’s `Result`/`Option` at runtime (no discriminant wrapper) and better aligned with the “member is itself” semantics. Error-variant runtime values need a `Value` form carrying the variant tag and fields—reuse the existing `RaisedError` structure, but as a return value (implement as `Value::ErrorValue { variant, fields }`), distinct from `raise` control flow.

---

## VII. Type rule details (semantic)

- Assignability: Positions of type `one of { A, B }` accept values of any member (member → union upcast). Union → union when target’s member set ⊇ source’s member set. Members do not auto-convert between each other. `Null` is only compatible with `Null` members.
- Return checking: For an action returning `one of {...}`, each `return e` must have `e` of some member type (or upcastable to a member). Overall return/raise termination properties remain unchanged.
- Match subjects: Only `Bool`/state/`one of` are matchable; matching other types yields `InvalidMatchSubject`.
- Distinguishability: see §2.2; checked statically when building unions.
- List: `list of T` is covariant and compares inners (same as the old `List`).

---

## VIII. Impact on D1 / D3 / benchmarks

- D1 demo: fallible computations return `one of { Int, SomeError }`; the caller matches the two members—directly demonstrating “recoverable failure + mandatory handling,” with zero wrapper boilerplate.
- D3 pipeline: pipeline steps return `one of {...}`; after matching, the caller decides to continue/abort.
- Benchmarks: hidden-case `ExpectedOutcome` remains `Returns(Value)` / `Raises(variant)`. Success members of `one of` use `Returns(Value::Int(..))`; returned error variants use `Returns(Value::ErrorValue{..})` (recoverable return value), distinct from `raise` (`Raises`, unrecoverable).

---

## IX. Decision record (locked on 2026-05-30)

1. Unified rule: `<>` exclusive to intents; structural types use the `of` family (`list of` / `one of` / `schema of`). No exceptions.
2. Remove the `<>` forms of `Optional<T>` / `List<T>` / `Schema<T>`, and `Some`/`None`, and `<optional>.exists`—single path, thorough removal, no compatibility layer, no syntactic sugar.
3. `one of` members are constructed and matched directly, no `Ok`/`Err`/`Some`/`None` wrappers.
4. Add built-in single-value `Null` to express “none/absent.”
5. Type patterns in `match` are a necessary new mechanism; exhaustiveness + ban on `_` remain; distinguishability is checked statically.
6. Nullable lookups return `one of { ValueTy, Null }`: lookup-like operations in the standard library (hit value / miss `Null`) are the typical use of `one of` (e.g., future `File`/`DB` reads). [Note: When F1 landed, this was validated on the then-existing `storage` node; `storage` has been removed.]
7. Scope: `one of` can be used in any type position (output/input/field/let), not limited to outputs—it is a fundamental type constructor; limiting it would add extra rules (violating “few conventions”).

> This is a one-off, thorough refactor at the v1 starting phase: the overall surface syntax is smaller (less `<>`/`of` ambiguity, no Some/None/wrappers) and the rules are stronger (one exception-less rule). It is a strongly argued LLM-native gain, not airy theory.

---

## X. Change log

- 2026-05-30 — Replaces the now-deprecated `result_type.md`. After discussion, the Rust-style `Result<T,E>` was rejected (it biases thinking via Rust mental models, and under plan C the wrapper carries no information). Instead: the `one of {...}` union directly expresses fallible/nullable returns (members are themselves, no wrappers), and we unify the entire type syntax: `<>` exclusive to intents; structural types use the `of` keyword family; deprecate `Optional`/`List<>`/`Schema<>`/`Some`/`None`; add `Null`; introduce type patterns to `match`. Single path, thorough refactor, no compatibility, no syntactic sugar.
