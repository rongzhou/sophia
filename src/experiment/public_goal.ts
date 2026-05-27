import type { BenchmarkTask } from "./task.js";
import { renderPromptTemplate } from "../llm/prompt_templates.js";

export function buildPublicGoalForTask(task: BenchmarkTask): string {
  const forbidden = task.public_forbidden.map(sanitizeGoalText).map((item) => `- ${item}`).join("\n");
  return renderPromptTemplate("experiment/public_goal.md", {
    prompt_goal: sanitizeGoalText(task.prompt_goal),
    constraints_block: forbidden.length > 0 ? `\nPublic constraints:\n${forbidden}` : "",
  });
}

function sanitizeGoalText(value: string): string {
  return value
    .replace(/\bOptional\s*<\s*Text\s*>/g, "optional text")
    .replace(/\bOptional\s*<\s*Int\s*>/g, "optional integer")
    .replace(/\bList\s*<\s*Int\s*>/g, "list of integers")
    .replace(/\bList\s*<\s*Text\s*>/g, "list of text values")
    .replace(/\bSome\s*\(\s*value\s*\)/g, "present")
    .replace(/\bNone\b/g, "absent")
    .replace(/\bof type optional (text|integer)\b/g, "with optional $1")
    .replace(/\boptional (text|integer) input named ([a-z_]\w*) with optional \1\b/g, "optional $1 input named $2")
    .replace(/\bof type ([A-Z][A-Za-z0-9]*)\b/g, "in the $1 category")
    .replace(
      /\bUse an explicit exhaustive match over ([a-z_]\w*)\./gi,
      "Cover every possible value of $1 explicitly.",
    )
    .replace(/\bThe action accepts\b/g, "It accepts")
    .replace(/\bImplement an action\b/g, "Design behavior")
    .replace(/\bImplement a pure action\b/g, "Design pure behavior")
    .replace(/\bexplicit exhaustive match\b/gi, "explicit exhaustive branching")
    .replace(/\bcatch-all match case\b/gi, "catch-all branch")
    .replace(/\bSophia match cases must be explicit\b/g, "Every branch case must be explicit")
    .replace(/\bConsole\.Write\b/g, "console output")
    .replace(/\b([A-Z][A-Za-z0-9]*)\.([A-Z][A-Za-z0-9]*)\b/g, "$2")
    .replace(/\bInt\b/g, "integer")
    .replace(/\bText\b/g, "text")
    .replace(/\bBool\b/g, "boolean")
    .replace(/\bUnit\b/g, "no returned value");
}
