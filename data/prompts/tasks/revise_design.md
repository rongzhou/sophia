You are revising algorithm pseudocode, not writing program code.

The pseudocode may be structured JSON. That is allowed. Keep the structure, but ensure the contents are algorithm analysis and pseudocode only.

Revise only the issues described by the deterministic pseudocode check. Preserve the user's intent, algorithm, expected behavior, constraints, and forbidden behavior.

Do not introduce target-language syntax or any other source-code syntax. Do not add source files, declarations, imports, signatures, type annotations, effect declarations, source paths, implementation-stage names, or implementation hints.

Do not invent missing business logic, hidden expected values, loop counts, branch conditions, or state updates. If the current pseudocode does not contain enough semantic information to make a vague step explicit, return status "needs_clarification", keep the original pseudocode in "pseudocode", and put concise clarification questions in "questions".

Required pseudocode content:

- purpose
- inputs
- outputs
- algorithm

Revision guidance:

- Preserve clear pseudocode branch forms; do not revise solely to make branches look like source code.
- Use nested if blocks inside else only for mutually exclusive classification of the same value.
- When processing multiple inputs or items independently in order, write separate if steps so later inputs are still considered after earlier inputs match.
- Preserve the intended answer shape. If there are multiple semantic outputs, package them into one result value only when that preserves the user's intent.
- Keep helper flags, validation booleans, temporary counters, and intermediate values inside algorithm steps, not outputs.
- If the goal or current pseudocode says to update, apply, transform, or return a record-like value, keep the result described as that record-like value unless it explicitly asks to return one scalar field.
- Prefer true/false for boolean state, but do not rewrite natural 0/1 flag pseudocode unless diagnostics say it makes the condition ambiguous.
- For text composition, keep or add concrete algorithm wording such as "set combined to left followed by right" or "set label to prefix followed by name".
- Natural console wording such as "print value as text" is acceptable as semantic intent.
- Only revise text conversion wording when the deterministic check reports that it makes the logical output ambiguous.
- If inputs or outputs are record-like values with named fields, keep or add plain-language definitions. Do not add formal type syntax.
- If the current pseudocode asks for reusable steps, pipeline stages, validation/update/orchestration, effect boundaries, or explicit helper steps, keep those logical boundaries.
- Do not collapse meaningful helper steps just to make implementation easier; revise them only when they are vague or duplicate a wrapper that adds no semantic boundary.
- If a helper step only performs an effect, keep the boundary when it is intentional and state that it returns no value.
- Do not replace concrete helper boundaries with one vague monolithic algorithm step.

Return JSON only. No markdown fences and no commentary.

Return schema:
{
"status": "revised | needs_clarification",
"pseudocode": "complete revised pseudocode; JSON structure is allowed",
"notes": ["short generic explanation of changes"],
"questions": ["only when status is needs_clarification"]
}

Pseudocode repair context:
{{pseudo_repair_context}}

Pseudocode check result:
{{check_result}}

Current pseudocode:
{{pseudocode}}
