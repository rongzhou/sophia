Diagnostic repair guide:

- CHECK-FILE-003: every .sophia file must contain one complete top-level node, such as domain Name {}.
- CHECK-FILE-004: split multiple top-level nodes into separate files.
- CHECK-FILE-005: make the file path kind match the top-level node kind.
- CHECK-FILE-006: make the top-level declaration name match the file path node name exactly.
- CHECK-BODY-003: replace append(list, item) with list + [item] or list.append(item).
- CHECK-BODY-004: rewrite invented body statements using only allowed Sophia v0 body statements.
- CHECK-BODY-005: replace list == [] or list != [] with an explicit Int counter comparison.
- CHECK-SYNTAX-009: remove the unsupported call keyword. Use ActionName { input = value }.
- CHECK-SYNTAX-010: remove local variable type annotations. Use let name = expr or let mutable name = expr.
- CHECK-SYNTAX-011: initialize mutable locals when declaring them. Use let mutable name = initial_expr.
- CHECK-SYNTAX-012: replace empty List<T> expressions with [].
- CHECK-SYNTAX-013: replace bare return with return unit for Unit actions.
- CHECK-SYNTAX-014: replace return Unit with return unit.
- CHECK-SYNTAX-015: put bare action call statements inside let ignored = ActionName { ... }.
- CHECK-SYNTAX-016: remove unsupported conversion helpers such as Int.toText; print Int expressions directly when printing.
- CHECK-VAR-001: declare identifiers before use or use action input fields.
- CHECK-VAR-003: only assign with set to variables declared with let mutable.
- CHECK-RETURN-001: add a return expression matching the declared output type. Do not assign to an output field name unless it was declared as a mutable local variable first.
- CHECK-MATCH-002: match only Bool, declared state values, or Optional<T>.
- CHECK-MATCH-003: make each match case pattern match the expression type: true/false for Bool, StateName.Value for states, Some(name)/None for Optional<T>.
- CHECK-MATCH-004: remove duplicate match cases.
- CHECK-MATCH-005: add every missing exhaustive match case.
- CHECK-MATCH-006: rename the Some binding so it does not shadow a visible variable.
- CHECK-CAPABILITY-004: remove the action effect or pick a capability whose allow block contains it; deny always wins over allow.
- CHECK-ACTION-CALL-007: break the recursive action-call cycle. Make one action consume an explicit input value instead of calling back into the caller.
- CHECK-ACTION-CALL-008: declare every called error variant on the calling action's errors list, or stop calling the action that raises it.
- CHECK-ERROR-005: declare the raised variant on the action's errors list, or remove the raise.
- CHECK-ERROR-006: rewrite raise with field = expression assignments inside braces, like raise NotFound { id = item_id }.
- CHECK-ERROR-007: only assign fields that the error variant declares. Remove unknown fields from the raise.
- CHECK-ERROR-008: assign every field declared on the error variant in the raise expression.

Use this guide only to repair checker diagnostics. Do not add task-specific behavior that is not present in the current files or .pseudo constraints.
