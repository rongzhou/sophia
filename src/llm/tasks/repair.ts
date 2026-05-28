import { z } from "zod";
import type { CheckResult } from "../../lang/ast/diagnostics.js";
import { buildRepairContext } from "../../analysis/repair_context.js";
import { buildSophiaScaffold } from "../../pseudo/scaffold.js";
import {
  type ImplementationStructureOverride,
  buildImplementationStructurePlan,
  pseudocodeForImplementationPrompt,
} from "../../pseudo/structure_plan.js";
import { buildActionContext } from "../../analysis/context.js";
import { readPseudoSection } from "../../pseudo/document.js";
import { generateOllamaJson } from "../client.js";
import {
  ANTI_CHEAT_RULES,
  JSON_FILESET_CONTRACT,
  PROMPT_PATHS,
  REPAIR_DIAGNOSTIC_GUIDE,
  SOPHIA_V0_SYNTAX_GUIDE,
  renderPromptTemplate,
} from "../prompt_templates.js";
import {
  ImplementationOutputSchema,
  validateImplementationOutputForPseudocode,
} from "./implement_design.js";

export type RepairOutput = z.infer<typeof ImplementationOutputSchema>;

export interface RepairWithOllamaResult {
  prompt: string;
  rawResponse: string;
  output: RepairOutput;
}

export async function repairCodeWithOllama(options: {
  files: Record<string, string>;
  checkResult: CheckResult;
  model: string;
  pseudocode: string;
  structureOverride?: ImplementationStructureOverride;
}): Promise<RepairWithOllamaResult> {
  const prompt = buildRepairPrompt(
    options.files,
    options.checkResult,
    options.pseudocode,
    options.structureOverride,
  );
  return generateOllamaJson({
    model: options.model,
    prompt,
    operation: "repair",
    schema: ImplementationOutputSchema,
    validationRetry: true,
    validate: (output) =>
      validateImplementationOutputForPseudocode(
        output,
        options.pseudocode,
        options.structureOverride,
      ),
  });
}

export function buildRepairPrompt(
  files: Record<string, string>,
  checkResult: CheckResult,
  pseudocode: string,
  structureOverride: ImplementationStructureOverride = {},
): string {
  const repairContext = buildRepairContext({ files, checkResult });
  const pseudoContext = summarizePseudocodeForRepair(pseudocode);
  const scaffold = buildSophiaScaffold(pseudocode, structureOverride);
  const structurePlan = buildImplementationStructurePlan(pseudocode, structureOverride);
  const actionContext = buildActionContext(files, structurePlan.symbols.action);
  return renderPromptTemplate(PROMPT_PATHS.task.repair, {
    sophia_v0_syntax_guide: SOPHIA_V0_SYNTAX_GUIDE,
    anti_cheat_rules: ANTI_CHEAT_RULES,
    repair_diagnostic_guide: REPAIR_DIAGNOSTIC_GUIDE,
    json_fileset_contract: JSON_FILESET_CONTRACT,
    repair_context: JSON.stringify(repairContext, null, 2),
    action_context: JSON.stringify(actionContext, null, 2),
    pseudo_context: JSON.stringify(pseudoContext, null, 2),
    scaffold: JSON.stringify(scaffold, null, 2),
    check_result: JSON.stringify(checkResult, null, 2),
    files: JSON.stringify(files, null, 2),
  });
}

function summarizePseudocodeForRepair(pseudocode: string): Record<string, string> {
  const sanitized = pseudocodeForImplementationPrompt(pseudocode);
  const sections = [
    "purpose",
    "entities",
    "inputs",
    "outputs",
    "algorithm",
    "constraints",
    "forbidden",
    "effects",
  ] as const;
  return Object.fromEntries(
    sections.map((section) => [section, compactSection(sanitized, section)]),
  );
}

function compactSection(pseudocode: string, sectionName: string): string {
  return readPseudoSection(pseudocode, sectionName)
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .join("\n")
    .slice(0, 2000);
}
