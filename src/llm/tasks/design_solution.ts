import { z } from "zod";
import { parsePseudocodeJson } from "../../pseudo/document.js";
import { generateOllamaJson } from "../client.js";
import { PROMPT_PATHS, renderPromptTemplate } from "../prompt_templates.js";

export const DesignSolutionOutputSchema = z
  .object({
    status: z.enum(["designed", "needs_clarification"]),
    pseudocode: z.string().min(1),
    notes: z.array(z.string()),
    questions: z.array(z.string()),
    self_check: z
      .object({
        has_required_sections: z.boolean(),
        no_program_code: z.boolean(),
        no_hidden_expected_outputs: z.boolean(),
        concrete_algorithm_steps: z.boolean(),
      })
      .strict(),
  })
  .strict();

export type DesignSolutionOutput = z.infer<typeof DesignSolutionOutputSchema>;

export interface DesignSolutionWithOllamaResult {
  prompt: string;
  rawResponse: string;
  output: DesignSolutionOutput;
}

export async function designSolutionWithOllama(options: {
  goal: string;
  model: string;
}): Promise<DesignSolutionWithOllamaResult> {
  const prompt = buildDesignSolutionPrompt(options.goal);
  return generateOllamaJson({
    model: options.model,
    prompt,
    operation: "solution design",
    schema: DesignSolutionOutputSchema,
    validationRetry: true,
    validate: validateDesignSolutionOutput,
  });
}

export function buildDesignSolutionPrompt(goal: string): string {
  return renderPromptTemplate(PROMPT_PATHS.task.designSolution, { goal });
}

export function validateDesignSolutionOutput(output: DesignSolutionOutput): DesignSolutionOutput {
  const failedChecks = Object.entries(output.self_check)
    .filter(([, value]) => value === false)
    .map(([key]) => key);
  if (failedChecks.length > 0) {
    throw new Error(`Solution design self_check failed: ${failedChecks.join(", ")}`);
  }
  const pseudocode = parsePseudocodeJson(output.pseudocode);
  if (!pseudocode) {
    throw new Error("Solution design pseudocode must be a JSON object encoded as a string.");
  }
  if (containsProgramLikePseudocodeSyntax(output.pseudocode)) {
    throw new Error("Solution design output contains program-like top-level code.");
  }
  if (containsFormalPseudoSyntax(output.pseudocode)) {
    throw new Error("Solution design output contains formal Sophia syntax in .pseudo.");
  }
  if (/```/.test(output.pseudocode)) {
    throw new Error("Solution design output must not contain markdown fences.");
  }
  return output;
}

export function containsProgramLikePseudocodeSyntax(pseudocode: string): boolean {
  return parsePseudocodeJson(pseudocode) === null;
}

export function containsFormalPseudoSyntax(pseudocode: string): boolean {
  const parsed = parsePseudocodeJson(pseudocode);
  if (!parsed) return true;
  return collectStrings(parsed).some((value) =>
    /\b(?:Console\.Write|DB\.(?:Read|Write)\s*\()/.test(value) ||
    /\b(?:Unit|Bool|Int|Text|List\s*<|Optional\s*<|Raw\s*<|Parsed\s*<|Validated\s*<|Sanitized\s*<|Verified\s*<|Authorized\s*<|Persisted\s*<|Secret\s*<|Redacted\s*<)\b/.test(
      value,
    ),
  );
}

function collectStrings(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (Array.isArray(value)) return value.flatMap(collectStrings);
  if (value && typeof value === "object") {
    return Object.values(value as Record<string, unknown>).flatMap(collectStrings);
  }
  return [];
}
