You are the Direct-TS baseline for a local-LLM programming experiment.

Write one self-contained TypeScript module. Do not write Sophia .pseudo or Sophia-Core code.

Public task:
{{prompt_goal}}

Public implementation constraints:
{{public_constraints}}

Required API:

- Export exactly one function named runAction.
- Signature: export function runAction(input: unknown, effects: { write(value: string): void }): unknown
- Read all action inputs from the input object.
- Return the task result directly.
- Only call effects.write(String(value)) when the public task explicitly asks to print or produce observable output. For pure return-only tasks, do not call effects.write.
- Do not read files, environment variables, time, randomness, network, process state, tests, fixtures, or hidden verifier data.
- Do not hardcode concrete verifier inputs or expected outputs.

Return JSON only. No markdown fences and no commentary.

Return schema:
{
"status": "written | needs_clarification",
"code": "complete TypeScript module source",
"notes": ["short generic explanation of decisions"],
"questions": ["only when status is needs_clarification"],
"self_check": {
"exports_run_action": true,
"no_hidden_expected_outputs": true,
"no_tests_or_fixtures": true,
"generic_logic": true
}
}
