You are implementing Sophia .pseudo into candidate .sophia source files.

The .pseudo is not executable. Lower its existing algorithm into formal Sophia v0 syntax without revising, improving, or replacing the algorithm.
Validation-only expected outputs are hidden during implementation. Preserve the algorithm, constraints, and forbidden behavior without hardcoding hidden validation values.

Use the deterministic scaffold below as structural assistance, not as permission for the tool to invent semantics. It reduces mechanical burden but does not implement the algorithm for you. You are responsible for translating the .pseudo semantics into valid, checkable Sophia v0 source. Preserve only the explicit contract entries listed in `structure_plan.action_contract_hints`: file paths, top-level names, capability binding, explicitly typed input fields, explicitly typed output field, explicitly declared states, explicitly typed entity fields, and explicit effects. If an input/output/entity field is absent from that plan, choose the smallest compiled contract that faithfully represents the .pseudo semantics; do not add, remove, reorder, strengthen, weaken, or reinterpret algorithm steps. Do not treat scaffold placeholder comments or placeholder `result: Unit` as a required contract. When JSON algorithm steps name reusable logical stages, preserve those boundaries when they carry real validation, update, orchestration, or effect semantics. Replace TODO/tool comments with valid Sophia v0 source. Do not leave scaffold comments in the final files.
Public state contracts are case-sensitive and immutable. If `structure_plan.action_contract_hints.states` contains values such as Pending and Done, emit exactly those values in the state file and in match patterns; do not lowercase, rename, translate, or replace them with entity fields.
Keep helper action contracts compatible with the main scaffold action and the declared entities. If the main action receives or returns an entity, helper actions that validate or update the same value should use that same entity type instead of converting it to Text or exposing intermediate flags as the main result.
Do not copy pseudocode branch notation into Sophia-Core bodies. `if ... then`, `else if ... then`, and `end if` are pseudocode forms; Sophia v0 uses `if condition { ... } else { ... }`, or `match` for exhaustive Bool/Optional/state branching.

Before returning JSON, internally check:

1. Every output file path is in the supported domain/entity/capability/action layout.
2. Every action body uses only allowed Sophia v0 statements.
3. Every variable is declared before use, and every set target is mutable.
4. Every name in pseudo_outline.mutable_state_candidates is initialized with let mutable before any later set.
5. Every action expression names an existing action and supplies exactly the declared inputs. Never write a "call" keyword; the expression is ActionName { input = value }.
6. The main action preserves the explicit structure_plan contract; placeholder scaffold entries may be replaced when the .pseudo semantics require it.
7. The implementation only lowers the existing algorithm; it does not invent a new algorithm, delete required steps, or change branch/loop semantics.
8. No expected-output literals or validation-only details were used as implementation shortcuts.

{{sophia_v0_syntax_guide}}

{{anti_cheat_rules}}

{{json_fileset_contract}}

Pseudo outline:
{{pseudo_outline}}

Deterministic structure plan:
{{structure_plan}}

Deterministic Sophia scaffold:
{{scaffold}}

Action-rooted semantic context:
{{action_context}}

Input .pseudo:
{{pseudocode}}
