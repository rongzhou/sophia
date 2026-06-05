# JSON Library

The `json` library validates external JSON text. It is a pure Sophia library; do not call a Rust, JavaScript, or host JSON parser for the v2 MVP.

## Public Actions

- `ValidateJson(text: Raw<Text>) -> one of { JsonValid, JsonInvalid }`
  - Accepts object, array, string, int, bool, null, and whitespace.
  - Rejects floats, exponents, `\uXXXX`, trailing garbage, unclosed containers, unclosed strings, illegal escapes, repeated separators, and unknown tokens.
  - Returns `JsonInvalid` for malformed JSON instead of raising.

## Types

- `JsonValid`
  - Fields:
    - `position: Int`
- `JsonInvalid`
  - Fields:
    - `reason: Text`
    - `position: Int`

## Implementation Guidance

- Use `text.length`, `text.char_at(index)`, `text.slice(start, length)`, and `text.starts_with(prefix)`.
- Use `while condition { ... }` for cursor loops.
- Treat `Raw<Text>` as untrusted input. Validation may inspect it, but downstream code must not assume semantic trust unless it checks the `JsonValid` result.
- Do not implement JSON Schema, float/exponent numbers, or unicode escape decoding in v2 MVP.
