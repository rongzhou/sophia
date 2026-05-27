import { z } from "zod";
import { extractNamedSection } from "../../lang/braces.js";
import { generateOllamaJson } from "../client.js";
import { renderPromptTemplate } from "../prompt_templates.js";

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
  return renderPromptTemplate("tasks/design_solution.md", { goal });
}

export function validateDesignSolutionOutput(output: DesignSolutionOutput): DesignSolutionOutput {
  for (const [key, value] of Object.entries(output.self_check)) {
    if (value === false) {
      throw new Error(`Solution design self_check failed: ${key}`);
    }
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
  return (
    /^\s*(program|action|capability|domain)\s+[A-Z][A-Za-z0-9]*\s*\{/m.test(pseudocode) ||
    /\b(?:subaction|main_flow)\s+[A-Z][A-Za-z0-9]*\s*\{/m.test(pseudocode)
  );
}

export function containsFormalPseudoSyntax(pseudocode: string): boolean {
  if (
    /\b(?:inputs|outputs|entities)\s*\{[^}]*\b[a-z_]\w*\s*:\s*(?:Unit|Bool|Int|Text|List\s*<|Optional\s*<|Raw\s*<|Parsed\s*<|Validated\s*<|Sanitized\s*<|Verified\s*<|Authorized\s*<|Persisted\s*<|Secret\s*<|Redacted\s*<|[A-Z][A-Za-z0-9]*)\b/im.test(
      pseudocode,
    )
  ) {
    return true;
  }
  if (/\beffects\s*\{[^}]*\b(?:Console\.Write|DB\.(?:Read|Write)\s*\()/im.test(pseudocode)) {
    return true;
  }
  const typedSections = ["inputs", "outputs", "entities"] as const;
  for (const section of typedSections) {
    const body = extractNamedSection(pseudocode, section) ?? "";
    if (
      /^\s*[a-z_]\w*\s*:\s*(?:Unit|Bool|Int|Text|List\s*<|Optional\s*<|Raw\s*<|Parsed\s*<|Validated\s*<|Sanitized\s*<|Verified\s*<|Authorized\s*<|Persisted\s*<|Secret\s*<|Redacted\s*<|[A-Z][A-Za-z0-9]*)\b/im.test(
        body,
      )
    ) {
      return true;
    }
  }
  const effects = extractNamedSection(pseudocode, "effects") ?? "";
  if (/\bConsole\.Write\b|\bDB\.(?:Read|Write)\s*\(/.test(effects)) return true;
  return false;
}
