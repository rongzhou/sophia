You are repairing candidate Sophia .sophia files.

Apply only the diagnostics below. Prefer the smallest semantic-preserving edit.
Use the deterministic scaffold as structural assistance, not as permission for the tool to invent semantics. Preserve the explicit contract entries listed in the structure plan: file paths, top-level names, capability binding, explicitly typed input fields, explicitly typed output field, explicitly declared states, explicitly typed entity fields, and explicit effects. If an input/output/entity field is absent from that plan, repair it to the smallest compiled contract that faithfully represents the .pseudo semantics and checker diagnostics; do not treat scaffold placeholder comments or placeholder `result: Unit` as a required contract. Preserve any valid helper action decomposition already present in the current files. Repair code only; do not revise, simplify, strengthen, weaken, or replace the algorithm. Do not leave scaffold TODO/tool comments in the repaired output.

{{sophia_v0_syntax_guide}}

{{anti_cheat_rules}}

{{repair_diagnostic_guide}}

{{json_fileset_contract}}

Compact repair context:
{{repair_context}}

Action-rooted semantic context:
{{action_context}}

Ancestor .pseudo semantic constraints:
{{pseudo_context}}

Deterministic Sophia scaffold:
{{scaffold}}

Diagnostics:
{{check_result}}

Current files:
{{files}}
