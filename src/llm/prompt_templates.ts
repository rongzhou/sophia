import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const TEMPLATE_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
  "data",
  "prompts",
);

export const PROMPT_PATHS = {
  common: {
    sophiaV0SyntaxGuide: "common/sophia_v0_syntax_guide.md",
    jsonFilesetContract: "common/json_fileset_contract.md",
    antiCheatRules: "common/anti_cheat_rules.md",
    repairDiagnosticGuide: "common/repair_diagnostic_guide.md",
  },
  client: {
    jsonOnlyRetry: "client/json_only_retry.md",
    validationRetry: "client/validation_retry.md",
  },
  task: {
    designSolution: "task/design_solution.md",
    implementDesign: "task/implement_design.md",
    repair: "task/repair.md",
    reviseDesign: "task/revise_design.md",
  },
  decision: {
    llmDecision: "decision/templates/llm_decision.md",
    decisionScaffold: "decision/data/decision_scaffold.json",
  },
  experiment: {
    directTs: "experiment/direct_ts.md",
    publicGoal: "experiment/public_goal.md",
  },
} as const;

export const SOPHIA_V0_SYNTAX_GUIDE = loadPromptTemplate(PROMPT_PATHS.common.sophiaV0SyntaxGuide);
export const JSON_FILESET_CONTRACT = loadPromptTemplate(PROMPT_PATHS.common.jsonFilesetContract);
export const ANTI_CHEAT_RULES = loadPromptTemplate(PROMPT_PATHS.common.antiCheatRules);
export const REPAIR_DIAGNOSTIC_GUIDE = loadPromptTemplate(
  PROMPT_PATHS.common.repairDiagnosticGuide,
);

export function loadPromptTemplate(name: string): string {
  const templatePath = path.join(TEMPLATE_ROOT, name);
  const relative = path.relative(TEMPLATE_ROOT, templatePath);
  if (relative.startsWith("..") || path.isAbsolute(relative)) {
    throw new Error(`Prompt template path escapes data/prompts: ${name}`);
  }
  return readFileSync(templatePath, "utf8").trimEnd();
}

export function loadPromptData<T>(name: string): T {
  return JSON.parse(loadPromptTemplate(name)) as T;
}

export function renderPromptTemplate(
  name: string,
  values: Record<string, string | number | boolean>,
): string {
  const template = loadPromptTemplate(name);
  return template.replace(/\{\{([a-zA-Z0-9_]+)\}\}/g, (_: string, key: string) => {
    const value = values[key];
    if (value === undefined) {
      throw new Error(`Prompt template ${name} has no value for placeholder: ${key}`);
    }
    return String(value);
  });
}
