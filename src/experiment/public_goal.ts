import type { BenchmarkTask } from "./task.js";
import { PROMPT_PATHS, renderPromptTemplate } from "../llm/prompt_templates.js";

export function buildPublicGoalForTask(task: BenchmarkTask): string {
  const forbidden = task.public_forbidden.map(sanitizeGoalText).map((item) => `- ${item}`).join("\n");
  return renderPromptTemplate(PROMPT_PATHS.experiment.publicGoal, {
    prompt_goal: sanitizeGoalText(task.prompt_goal),
    constraints_block: forbidden.length > 0 ? `\nPublic constraints:\n${forbidden}` : "",
  });
}

function sanitizeGoalText(value: string): string {
  const orderedReplacements: Array<[RegExp, string]> = [
    [/\bOptional\s*<\s*Text\s*>/g, "optional text"],
    [/\bOptional\s*<\s*Int\s*>/g, "optional integer"],
    [/\bList\s*<\s*Int\s*>/g, "list of integers"],
    [/\bList\s*<\s*Text\s*>/g, "list of text values"],
    [/\bSome\s*\(\s*value\s*\)/g, "present"],
    [/\bNone\b/g, "absent"],
    [/\bof type optional (text|integer)\b/g, "with optional $1"],
    [/\boptional (text|integer) input named ([a-z_]\w*) with optional \1\b/g, "optional $1 input named $2"],
    [/\bof type ([A-Z][A-Za-z0-9]*)\b/g, "in the $1 category"],
    [/\bUse an explicit exhaustive match over ([a-z_]\w*)\./gi, "Cover every possible value of $1 explicitly."],
    [/\bThe action accepts\b/g, "It accepts"],
    [/\bImplement an action\b/g, "Design behavior"],
    [/\bImplement a pure action\b/g, "Design pure behavior"],
    [/\bexplicit exhaustive match\b/gi, "explicit exhaustive branching"],
    [/\bcatch-all match case\b/gi, "catch-all branch"],
    [/\bSophia match cases must be explicit\b/g, "Every branch case must be explicit"],
    [/\bConsole\.Write\b/g, "console output"],
    [/\b([A-Z][A-Za-z0-9]*)\.([A-Z][A-Za-z0-9]*)\b/g, "$2"],
    [/\bInt\b/g, "integer"],
    [/\bText\b/g, "text"],
    [/\bBool\b/g, "boolean"],
    [/\bUnit\b/g, "no returned value"],
  ];
  let result = value;
  for (const [pattern, replacement] of orderedReplacements) {
    result = result.replace(pattern, replacement);
  }
  return result;
}
