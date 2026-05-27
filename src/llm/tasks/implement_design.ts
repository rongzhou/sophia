import { z } from "zod";
import type { SophiaRawAst } from "../../lang/parser.js";
import { parseSophiaSource } from "../../lang/parser.js";
import { parseStateAst } from "../../lang/check_model.js";
import { parseSophiaEffectNames, parseSophiaFieldDeclarations } from "../../lang/signature.js";
import {
  isSafeRelativeArtifactPath,
  isSupportedSophiaFilePath,
} from "../../workspace/sophia_paths.js";
import { generateOllamaJson } from "../client.js";
import { renderPromptTemplate } from "../prompt_templates.js";
import { ANTI_CHEAT_RULES, JSON_FILESET_CONTRACT, SOPHIA_V0_SYNTAX_GUIDE } from "./prompts.js";
import { escapeRegExp } from "../../util/strings.js";
import {
  buildImplementationStructurePlan,
  type ImplementationStructureOverride,
  pseudocodeForImplementationPrompt,
} from "../../pseudo/structure_plan.js";
import { outlinePseudocode } from "../../pseudo/outline.js";
import { buildSophiaScaffold, SCAFFOLD_TODO_PATTERN } from "../../pseudo/scaffold.js";
import { buildActionContext } from "../../analysis/context.js";

export const ImplementationOutputSchema = z
  .object({
    files: z.record(z.string(), z.string()),
    notes: z.array(z.string()),
    self_check: z
      .object({
        no_var: z.boolean(),
        no_direct_console_write: z.boolean(),
        no_for_or_while: z.boolean(),
        preserved_constraints: z.boolean(),
      })
      .strict(),
  })
  .strict();

export type ImplementationOutput = z.infer<typeof ImplementationOutputSchema>;

export interface ImplementWithOllamaResult {
  prompt: string;
  rawResponse: string;
  output: ImplementationOutput;
}

export async function implementDesignWithOllama(options: {
  pseudocode: string;
  model: string;
  structureOverride?: ImplementationStructureOverride;
}): Promise<ImplementWithOllamaResult> {
  const prompt = buildImplementDesignPrompt(options.pseudocode, options.structureOverride);
  return generateOllamaJson({
    model: options.model,
    prompt,
    operation: "implementation",
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

export function buildImplementDesignPrompt(
  pseudocode: string,
  structureOverride: ImplementationStructureOverride = {},
): string {
  const outline = outlinePseudocode(pseudocode);
  const structurePlan = buildImplementationStructurePlan(pseudocode, structureOverride);
  const scaffold = buildSophiaScaffold(pseudocode, structureOverride);
  const actionContext = buildActionContext(
    stripScaffoldComments(scaffold.files),
    structurePlan.symbols.action,
  );
  const sanitizedPseudocode = pseudocodeForImplementationPrompt(pseudocode);
  return renderPromptTemplate("tasks/implement_design.md", {
    sophia_v0_syntax_guide: SOPHIA_V0_SYNTAX_GUIDE,
    anti_cheat_rules: ANTI_CHEAT_RULES,
    json_fileset_contract: JSON_FILESET_CONTRACT,
    pseudo_outline: JSON.stringify(outline, null, 2),
    structure_plan: JSON.stringify(structurePlan, null, 2),
    scaffold: JSON.stringify(scaffold, null, 2),
    action_context: JSON.stringify(actionContext, null, 2),
    pseudocode: sanitizedPseudocode,
  });
}

function stripScaffoldComments(files: Record<string, string>): Record<string, string> {
  return Object.fromEntries(
    Object.entries(files).map(([filePath, content]) => [
      filePath,
      content
        .split("\n")
        .filter((line) => !SCAFFOLD_TODO_PATTERN.test(line))
        .join("\n"),
    ]),
  );
}

function validateImplementationOutputShape(output: ImplementationOutput): ImplementationOutput {
  if (output.self_check) {
    for (const [key, value] of Object.entries(output.self_check)) {
      if (value === false) {
        throw new Error(`Implementation self_check failed: ${key}`);
      }
    }
  }
  const paths = Object.keys(output.files);
  if (paths.length === 0) {
    throw new Error("Implementation output did not contain any files.");
  }
  for (const filePath of paths) {
    if (!isSupportedSophiaFilePath(filePath)) {
      throw new Error(`Invalid Sophia output path: ${filePath}`);
    }
    if (!isSafeRelativeArtifactPath(filePath)) {
      throw new Error(`Unsafe Sophia output path: ${filePath}`);
    }
  }
  if (!paths.some((filePath) => /\/domain\.sophia$/.test(filePath))) {
    throw new Error("Implementation output must include a domain.sophia file.");
  }
  if (!paths.some((filePath) => /\/capabilities\/.+\.sophia$/.test(filePath))) {
    throw new Error("Implementation output must include at least one capability file.");
  }
  if (!paths.some((filePath) => /\/actions\/.+\.sophia$/.test(filePath))) {
    throw new Error("Implementation output must include at least one action file.");
  }
  return output;
}

export function validateImplementationOutputForPseudocode(
  output: ImplementationOutput,
  pseudocode: string,
  structureOverride: ImplementationStructureOverride = {},
): ImplementationOutput {
  const validOutput = validateImplementationOutputShape(output);
  const plan = buildImplementationStructurePlan(pseudocode, structureOverride);
  const requiredPaths = [
    plan.files.domain,
    ...plan.files.entities,
    ...plan.files.states,
    plan.files.capability,
    plan.files.action,
  ].sort();
  const actualPaths = Object.keys(validOutput.files).sort();
  const missingPaths = requiredPaths.filter((filePath) => !validOutput.files[filePath]);
  if (missingPaths.length > 0) {
    throw new Error(
      `Implementation output must preserve deterministic scaffold file paths: missing ${missingPaths.join(", ")}; got ${actualPaths.join(", ")}.`,
    );
  }

  for (const [filePath, content] of Object.entries(validOutput.files)) {
    if (SCAFFOLD_TODO_PATTERN.test(content)) {
      throw new Error(`Implementation output left scaffold TODO/tool comment in ${filePath}.`);
    }
  }

  const contractFiles = preserveExplicitStateContractFiles(validOutput.files, plan);
  const contractOutput = { ...validOutput, files: contractFiles };
  const parsedFiles = parseImplementationFiles(contractOutput.files);
  const actionAst = requireParsedNode(parsedFiles, plan.files.action, "action");
  const capabilityAst = requireParsedNode(parsedFiles, plan.files.capability, "capability");
  const actionBody = contractOutput.files[plan.files.action] ?? "";
  for (const [index, entity] of plan.action_contract_hints.entities.entries()) {
    const filePath = plan.files.entities[index] ?? "";
    const entityAst = requireParsedNode(parsedFiles, filePath, "entity");
    if (entityAst.name !== entity.name) {
      throw new Error(`Implementation output must preserve entity ${entity.name}.`);
    }
    for (const field of entity.fields) {
      assertField(blockBody(entityAst, "fields"), "entity fields", field.name, field.type);
    }
  }
  for (const [index, state] of plan.action_contract_hints.states.entries()) {
    const filePath = plan.files.states[index] ?? "";
    const stateAst = requireParsedNode(parsedFiles, filePath, "state");
    const parsedState = parseStateAst(stateAst);
    if (parsedState.name !== state.name) {
      throw new Error(`Implementation output must preserve state ${state.name}.`);
    }
    if ([...parsedState.values].sort().join("\n") !== [...state.values].sort().join("\n")) {
      throw new Error(
        `Implementation output state ${state.name} must preserve values ${state.values.join(", ")}.`,
      );
    }
  }
  if (
    !attributeValue(actionAst, "capability")?.match(
      new RegExp(`^${escapeRegExp(plan.symbols.capability)}$`),
    )
  ) {
    throw new Error(`Implementation output action must preserve scaffold capability binding.`);
  }
  for (const field of plan.action_contract_hints.inputs) {
    assertField(blockBody(actionAst, "input"), "action input", field.name, field.type);
  }
  if (plan.action_contract_hints.output) {
    assertField(
      blockBody(actionAst, "output"),
      "action output",
      plan.action_contract_hints.output.name,
      plan.action_contract_hints.output.type,
    );
  }
  for (const effect of plan.action_contract_hints.effects) {
    if (!parseSophiaEffectNames(blockBody(actionAst, "effects")).includes(effect)) {
      throw new Error(`Implementation output action effects must preserve ${effect}.`);
    }
    if (!parseSophiaEffectNames(blockBody(capabilityAst, "allow")).includes(effect)) {
      throw new Error(`Implementation output capability allow block must preserve ${effect}.`);
    }
  }
  for (const subactionName of extractPseudoSubactionNames(pseudocode)) {
    const subactionPath = actionPathForName(actualPaths, subactionName);
    if (!subactionPath) {
      throw new Error(`Implementation output must preserve pseudo subaction ${subactionName}.`);
    }
    const subactionAst = requireParsedNode(parsedFiles, subactionPath, "action");
    if (subactionAst.name !== subactionName) {
      throw new Error(
        `Implementation output file ${subactionPath} must declare action ${subactionName}.`,
      );
    }
    if (!new RegExp(`\\b${escapeRegExp(subactionName)}\\s*\\{`).test(actionBody)) {
      throw new Error(
        `Main scaffold action must explicitly invoke pseudo subaction ${subactionName} with a direct action expression.`,
      );
    }
  }
  return contractOutput;
}

function preserveExplicitStateContractFiles(
  files: Record<string, string>,
  plan: ReturnType<typeof buildImplementationStructurePlan>,
): Record<string, string> {
  if (plan.action_contract_hints.states.length === 0) return files;
  const next = { ...files };
  for (const [index, state] of plan.action_contract_hints.states.entries()) {
    const filePath = plan.files.states[index];
    if (!filePath) continue;
    next[filePath] = `state ${state.name} {
${state.values.map((value) => `  value ${value} { }`).join("\n")}
}`;
  }
  return next;
}

function extractPseudoSubactionNames(pseudocode: string): string[] {
  return [
    ...new Set(
      [...pseudocode.matchAll(/\bsubaction\s+([A-Z][A-Za-z0-9]*)\s*\{/g)]
        .map((match) => match[1])
        .filter((name): name is string => Boolean(name)),
    ),
  ].sort();
}

function actionPathForName(paths: string[], actionName: string): string | null {
  return paths.find((filePath) => filePath.endsWith(`/actions/${actionName}.sophia`)) ?? null;
}

function parseImplementationFiles(files: Record<string, string>): Map<string, SophiaRawAst> {
  const parsedFiles = new Map<string, SophiaRawAst>();
  for (const [filePath, content] of Object.entries(files)) {
    const parsed = parseSophiaSource(content, filePath);
    if (!parsed.ok || !parsed.ast) {
      const problems = parsed.diagnostics.map((diagnostic) => diagnostic.problem).join("; ");
      throw new Error(`Implementation output file ${filePath} must parse as Sophia: ${problems}`);
    }
    parsedFiles.set(filePath, parsed.ast);
  }
  return parsedFiles;
}

function requireParsedNode(
  files: Map<string, SophiaRawAst>,
  filePath: string,
  kind: SophiaRawAst["kind"],
): SophiaRawAst {
  const ast = files.get(filePath);
  if (!ast) throw new Error(`Implementation output missing parsed file ${filePath}.`);
  if (ast.kind !== kind) {
    throw new Error(
      `Implementation output file ${filePath} must declare ${kind}, found ${ast.kind}.`,
    );
  }
  return ast;
}

function assertField(sectionBody: string, label: string, name: string, type: string): void {
  const field = parseSophiaFieldDeclarations(sectionBody).find(
    (candidate) => candidate.name === name,
  );
  if (field?.type !== type) {
    throw new Error(`Implementation output ${label} must preserve ${name}: ${type}.`);
  }
}

function blockBody(ast: SophiaRawAst, blockName: string): string {
  return ast.blocks.find((block) => block.name === blockName)?.body ?? "";
}

function attributeValue(ast: SophiaRawAst, attributeName: string): string | null {
  return ast.attributes.find((attribute) => attribute.name === attributeName)?.value ?? null;
}
