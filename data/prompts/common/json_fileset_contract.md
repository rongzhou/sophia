Return JSON only. No markdown fences and no commentary.

Return schema:
{
"files": {
"domains/<Domain>/domain.sophia": "...",
"domains/<Domain>/entities/<Entity>.sophia": "...",
"domains/<Domain>/capabilities/<Capability>.sophia": "...",
"domains/<Domain>/actions/<Action>.sophia": "..."
},
"notes": ["short explanation of semantic choices"],
"self_check": {
"no_var": true,
"no_direct_console_write": true,
"no_for_or_while": true,
"preserved_constraints": true
}
}
