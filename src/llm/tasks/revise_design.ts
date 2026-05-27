import { z } from "zod";
import type { PseudocodeCheckResult } from "../../pseudo/check.js";
import { buildPseudoRepairContext } from "../../pseudo/repair_context.js";
import { replaceNamedSection } from "../../lang/braces.js";
import { parsePseudocodeJson } from "../../pseudo/document.js";
import { generateOllamaJson } from "../client.js";
import { renderPromptTemplate } from "../prompt_templates.js";
import {
  containsFormalPseudoSyntax,
  containsProgramLikePseudocodeSyntax,
} from "./design_solution.js";

export const ReviseDesignOutputSchema = z
  .object({
    status: z.enum(["revised", "needs_clarification"]),
    pseudocode: z.string().min(1),
    notes: z.array(z.string()).default([]),
    questions: z.array(z.string()).default([]),
  })
  .strict();

export type ReviseDesignOutput = z.infer<typeof ReviseDesignOutputSchema>;

export interface ReviseDesignWithOllamaResult {
  prompt: string;
  rawResponse: string;
  output: ReviseDesignOutput;
}

export async function reviseDesignWithOllama(options: {
  pseudocode: string;
  checkResult: PseudocodeCheckResult;
  model: string;
}): Promise<ReviseDesignWithOllamaResult> {
  const prompt = buildReviseDesignPrompt(options.pseudocode, options.checkResult);
  return generateOllamaJson({
    model: options.model,
    prompt,
    operation: "design revision",
    schema: ReviseDesignOutputSchema,
    validationRetry: true,
    validate: validateReviseDesignOutput,
  });
}

export function validateReviseDesignOutput(output: ReviseDesignOutput): ReviseDesignOutput {
  if (containsProgramLikePseudocodeSyntax(output.pseudocode)) {
    throw new Error("Design revision output contains program-like pseudocode syntax.");
  }
  if (containsFormalPseudoSyntax(output.pseudocode)) {
    throw new Error("Design revision output contains formal Sophia syntax in .pseudo.");
  }
  if (/```/.test(output.pseudocode)) {
    throw new Error("Design revision output must not contain markdown fences.");
  }
  return output;
}

export function buildReviseDesignPrompt(
  pseudocode: string,
  checkResult: PseudocodeCheckResult,
): string {
  const sanitizedPseudocode = removeImplementationHints(pseudocode);
  const context = buildPseudoRepairContext({ pseudocode: sanitizedPseudocode, checkResult });
  return renderPromptTemplate("tasks/revise_design.md", {
    pseudo_repair_context: JSON.stringify(context, null, 2),
    check_result: JSON.stringify(checkResult, null, 2),
    pseudocode: sanitizedPseudocode,
  });
}

function removeImplementationHints(pseudocode: string): string {
  const json = parsePseudocodeJson(pseudocode);
  if (json && Object.prototype.hasOwnProperty.call(json, "implementation_hints")) {
    const rest = { ...json };
    delete rest.implementation_hints;
    return JSON.stringify(rest, null, 2);
  }
  return replaceNamedSection(pseudocode, "implementation_hints", "").trimEnd();
}
