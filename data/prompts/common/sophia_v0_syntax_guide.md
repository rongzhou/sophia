Sophia v0 syntax guide:

Top-level files:

- One file contains one top-level node.
- Use domain, entity, state, storage, error, capability, and action nodes for this v0 experiment.
- A domain file must be a complete block, for example: domain DemoDomain {}
- An entity file may declare explicit fields, for example: entity Item { fields { value: Int is_active: Bool } }
- A state file may declare explicit values, for example: state StateName { value ValueA { } value ValueB { } }
- A storage file may declare key/value types, for example: storage Items { key: Persisted<Text> value: Sanitized<Text> }
- An error file may declare variants, for example: error AccountError { variant InvalidAmount { amount: Int } }
- A capability with no effects is valid as: capability PureCapability { allow { } }

Action file structure:
action ActionName {
meaning: "short semantic description"
capability: CapabilityName
input { }
output { result: Unit | Bool | Int | Text | List<Int> | List<Text> | Optional<Text> }
effects { Console.Write }
errors { }
body {
...
}
}

Entity file structure:
entity EntityName {
fields {
field_name: Unit | Bool | Int | Text | List<Int> | List<Text> | Optional<Text> | OtherEntityName | Raw<Text> | Sanitized<Text> | Redacted<Text>
}

State file structure:
state StateName {
value ValueA { }
value ValueB { }
}
}

Storage file structure:
storage StorageName {
key: Persisted<Text>
value: Sanitized<Text> | Redacted<Text> | OtherEntityName
}

Allowed body statements:

- let name = expr
- let mutable name = expr
- set name = expr
- print expr
- repeat N times { ... }
- if condition { ... } else { ... }
- match expr { Pattern { ... } ... }
- raise ErrorVariant { field = expr }
- return expr

Forbidden body statement forms:

- var
- Console.Write(...)
- append(list, item)
- list == [] or list != [] emptiness checks
- for / while
- bare expression statements, including bare action expressions
- prose or natural-language instructions
- hardcoded full expected result when .pseudo forbids hardcoding

Unit rules:

- Unit is a type name; unit is the only Unit value.
- A Unit action must return unit. Correct: return unit. Incorrect: return Unit. Incorrect: return.
- To invoke a Unit helper action for its effects, bind the result: let ignored = HelperAction { input = value }. Do not write a bare action expression statement.

Optional rules:

- Use Optional<T> for absent-or-present values.
- None is the absent value and may only be returned or assigned where the expected type is Optional<T>.
- Some(expr) wraps a present value; its type is Optional<T> when expr has type T.
- Some(expr) and None may appear in return expressions and entity construction fields.
- match over Optional<T> is supported with exhaustive Some(name) and None cases. Some(name) binds the inner value only inside that case body.
- Do not write unwrap, exists, or default-value helpers.

Match rules:

- Match expressions may be Bool, a declared state type, or Optional<T>.
- Bool match must include both true and false cases.
- State match must include every declared StateName.Value case.
- Optional match must include both Some(name) and None cases.
- Sophia match never has a catch-all `_` case. Write every case explicitly.
- Match case bodies are block-scoped like if/repeat bodies.

Match examples:
body {
match flag {
true {
return "literal for true case"
}
false {
return "literal for false case"
}
}
}

body {
match optional_value {
Some(value) {
return value
}
None {
return "literal for absent case"
}
}
}

body {
match state_value {
StateName.ValueA {
return "literal for first state"
}
StateName.ValueB {
return "literal for second state"
}
}
}

Local variable rules:

- Local let declarations never use type annotations. Correct: let allowed = true. Incorrect: let allowed: Bool = true.
- Local mutable declarations must be initialized. Correct: let mutable result = item. Incorrect: let mutable result: Item.
- Type annotations appear only in input, output, entity fields, and action signatures, not in body-local variables.
- If the pseudo outline marks a name as a mutable_state_candidate, initialize it with let mutable before later set statements.

Boolean expressions allowed:

- true / false
- Int comparisons: count > 0, count <= limit, left == right, left != right
- Text and Bool equality: label == "ready", is_valid != false
- Bool composition: is_valid and not is_blocked
- State equality: item.state == StateName.ValueA

State expressions allowed:

- State values: StateName.ValueA, StateName.ValueB
- State values are only valid when both the state and the value are declared.

Text expressions allowed:

- string literals: "ready"
- Text variables and fields: label, item.title
- Text concatenation with no implicit conversion: left + right, "prefix: " + label
- Explicit Int-to-Text conversion: to_text(count)
- If .pseudo says to print a literal, print the literal directly: print "ready".
- If .pseudo says to print a variable, the printed expression must have type Sanitized<T> or Redacted<T>. Do not print Raw<T>, Secret<T>, or ordinary non-intent variables.
- If .pseudo says "print an Int value as text" or "write a generated number as text", use to_text(value) at the print boundary. Do not invent Int.toText, Text.fromInt, String(...), or temporary text variables.

Entity expressions allowed:

- field access: item.value
- construction with all fields: Item { value = item.value + delta, is_active = item.is_active }

Action call expressions allowed:

- Invoke another action with all inputs by writing the action expression directly: OtherAction { item = item, delta = delta }
- Do not write a "call" keyword in Sophia-Core. Incorrect: call OtherAction { item = item, delta = delta }
- Action expressions may appear in let, return, print, set, or if condition expressions; they are not standalone body statements.
- the caller must declare every effect used by called actions.
- do not recursively call the same action in v0.

Direct helper-action example:
body {
let allowed = OtherAction { item = item, delta = delta }
let mutable result = item
if allowed {
set result = item
} else {
set result = item
}
return result
}

Effect-only helper-action example:
body {
let ignored = PrintValue { value = value }
return unit
}

List update forms allowed:

- empty list initialization: let mutable values = []
- set list = list + [item]
- set list = list.append(item)
- Do not write empty List<Int> or empty List<Text> in Sophia-Core bodies.

Variable rules:

- A variable used in an expression must be declared first or be an input field.
- A variable updated with set must have been declared with let mutable.
- Do not update a value declared with plain let.
- Action output fields are not implicit variables. For output { result: T }, either return expr directly, or declare let mutable result = initial_value before any set result = expr, then return result.
- Every non-Unit action body must reach a return expr or raise on every control-flow path.

Error rules:

- Declare error variants in an error file with variant VariantName { field: Type }.
- An action may declare raised variants in errors { VariantName }.
- A body may raise only variants declared by its action errors block.
- Raise expressions must provide every variant field exactly once with the declared type.
- If an action calls another action that declares errors, the caller must declare those same variants until match/handle support exists.

Effect rules:

- If body uses print, effects must include Console.Write.
- If action effects include Console.Write, its capability allow block must include Console.Write.
- DB effects must name a PascalCase storage target. Correct: DB.Write("Items"). Incorrect: DB.Write.
- If action effects include DB.Read("Items") or DB.Write("Items"), its capability allow block must include the exact same effect string.
- If action effects include DB.Write("Items"), the action output type must exactly match storage Items value type.
- Do not remove effects or capability declarations to satisfy diagnostics.

Implementation discipline:

- Build declarations first, then bodies, then check each body against the declared inputs and outputs.
- For every action expression, verify the invoked action exists and all input names/types match.
- If the .pseudo explicitly asks for subactions or decomposition, preserve that structure with separate action files and direct action expressions.
- If the .pseudo does not ask for decomposition, do not invent extra actions unless it reduces real repeated logic without changing behavior.
