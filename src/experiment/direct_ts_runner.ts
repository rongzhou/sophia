import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import ts from "typescript";
import { z } from "zod";
import { errorDiagnostic as diagnosticError, type Diagnostic } from "../lang/ast/diagnostics.js";
import { generateOllamaJson } from "../llm/client.js";
import { isLlmCallError } from "../llm/errors.js";
import { PROMPT_PATHS, renderPromptTemplate } from "../llm/prompt_templates.js";
import {
  createRunDirectory,
  ensureRunStageDirectories,
  withScratchDirectory,
} from "../workspace/fs_layout.js";
import { deepEqualJson } from "../util/json.js";
import type { BenchmarkCase, BenchmarkTask } from "./task.js";
import type { DirectTsExperimentResult } from "./result.js";

export const DirectTsOutputSchema = z
  .object({
    status: z.enum(["written", "needs_clarification"]),
    code: z.string().min(1),
    notes: z.array(z.string()),
    questions: z.array(z.string()),
    self_check: z
      .object({
        exports_run_action: z.boolean(),
        no_hidden_expected_outputs: z.boolean(),
        no_tests_or_fixtures: z.boolean(),
        generic_logic: z.boolean(),
      })
      .strict(),
  })
  .strict();

export type DirectTsOutput = z.infer<typeof DirectTsOutputSchema>;

export interface DirectTsExperimentOptions {
  task: BenchmarkTask;
  model: string;
}

export interface DirectTsCaseResult {
  name: string;
  ok: boolean;
  result_ok: boolean;
  effects_ok: boolean;
  run_ok: boolean;
  actual_result: unknown;
  actual_effects: string[];
  diagnostics: Diagnostic[];
}

export interface DirectTsVerificationResult {
  ok: boolean;
  source: string;
  typecheck: {
    ok: boolean;
    diagnostics: Diagnostic[];
  };
  cases: DirectTsCaseResult[];
}

type DirectTsAction = (
  input: unknown,
  effects: {
    write(value: string): void;
  },
) => unknown;

export async function runDirectTsExperiment(
  options: DirectTsExperimentOptions,
): Promise<DirectTsExperimentResult> {
  const workspace = await createRunDirectory(process.cwd(), options.task.id);
  await ensureRunStageDirectories(workspace);
  const steps: Array<Record<string, unknown>> = [];
  const prompt = buildDirectTsPrompt(options.task);
  await writeFile(path.join(workspace, "goal", "prompt.txt"), prompt, "utf8");
  steps.push({ step: "start", artifact: "goal/prompt.txt" });

  let rawResponse = "";
  let output: DirectTsOutput | null = null;
  try {
    const result = await generateOllamaJson({
      model: options.model,
      prompt,
      operation: "direct TypeScript baseline",
      schema: DirectTsOutputSchema,
      validate: validateDirectTsOutput,
      temperature: 0.2,
      topP: 0.9,
    });
    rawResponse = result.rawResponse;
    output = result.output;
    await writeFile(path.join(workspace, "executable", "response.txt"), rawResponse, "utf8");
    await writeFile(path.join(workspace, "executable", "candidate.ts"), output.code, "utf8");
    steps.push({
      step: "write_direct_ts",
      status: output.status,
      artifact: "executable/candidate.ts",
    });
  } catch (error) {
    if (isLlmCallError(error)) {
      steps.push({ step: "llm_error", message: error.message });
      if (error.rawResponse) {
        await writeFile(
          path.join(workspace, "executable", "response.txt"),
          error.rawResponse,
          "utf8",
        );
      }
    }
    return failedDirectTsResult({
      options,
      workspace,
      steps,
      failureType: "direct_ts_write_failed",
    });
  }

  if (output.status !== "written") {
    return failedDirectTsResult({
      options,
      workspace,
      steps,
      failureType: "direct_ts_needs_clarification",
    });
  }

  const verification = await verifyDirectTsCodeAgainstTask(output.code, options.task);
  steps.push({
    step: "hidden_verify",
    ok: verification.ok,
    cases: verification.cases.length,
    diagnostics: verification.typecheck.diagnostics.length,
  });
  return {
    ok: verification.ok,
    mode: "direct-ts",
    task_id: options.task.id,
    model: options.model,
    workspace,
    graph_dir: null,
    goal_node: null,
    pseudocode_node: null,
    code_node: null,
    repairs_used: 0,
    design_revisions_used: 0,
    failure_type: verification.ok ? null : "hidden_verification_failed",
    steps,
    verification,
  };
}

export function buildDirectTsPrompt(task: BenchmarkTask): string {
  const forbidden = task.public_forbidden.map((item) => `- ${item}`).join("\n");
  return renderPromptTemplate(PROMPT_PATHS.experiment.directTs, {
    prompt_goal: task.prompt_goal,
    public_constraints: forbidden || "- No extra public constraints.",
  });
}

export function validateDirectTsOutput(output: DirectTsOutput): DirectTsOutput {
  const failedChecks = Object.entries(output.self_check)
    .filter(([, value]) => value === false)
    .map(([key]) => key);
  if (failedChecks.length > 0) {
    throw new Error(`Direct-TS self_check failed: ${failedChecks.join(", ")}`);
  }
  if (/```/.test(output.code)) {
    throw new Error("Direct-TS output must not contain markdown fences.");
  }
  if (!/\bexport\s+function\s+runAction\s*\(/.test(output.code)) {
    throw new Error("Direct-TS output must export function runAction.");
  }
  if (/\b(import|require)\s*(?:\(|["'])/.test(output.code)) {
    throw new Error("Direct-TS output must be self-contained and must not import dependencies.");
  }
  if (
    /\b(process|Date|Math\.random|fetch|XMLHttpRequest|localStorage|sessionStorage)\b/.test(
      output.code,
    )
  ) {
    throw new Error("Direct-TS output uses a forbidden ambient API.");
  }
  return output;
}

export async function verifyDirectTsCodeAgainstTask(
  code: string,
  task: BenchmarkTask,
): Promise<DirectTsVerificationResult> {
  return withScratchDirectory({
    root: process.cwd(),
    label: "direct-ts-verify",
    run: async (root) => {
      const sourcePath = "candidate.ts";
      const absoluteSourcePath = path.join(root, sourcePath);
      await writeFile(absoluteSourcePath, code, "utf8");
      const typecheck = typecheckDirectTs(root, sourcePath);
      if (!typecheck.ok) {
        return {
          ok: false,
          source: sourcePath,
          typecheck,
          cases: [],
        };
      }

      const source = await importDirectTsModule(root, sourcePath);
      const exported = source.runAction;
      if (typeof exported !== "function") {
        return {
          ok: false,
          source: sourcePath,
          typecheck: {
            ok: false,
            diagnostics: [
              diagnosticError(
                "DIRECT-TS-EXPORT-001",
                sourcePath,
                "Module does not export runAction.",
              ),
            ],
          },
          cases: [],
        };
      }

      const cases = await Promise.all(
        task.hidden_cases.map(async (testCase) =>
          verifyDirectTsCase(exported as DirectTsAction, testCase),
        ),
      );
      return {
        ok: cases.every((testCase) => testCase.ok),
        source: sourcePath,
        typecheck,
        cases,
      };
    },
  });
}

function typecheckDirectTs(
  root: string,
  sourcePath: string,
): { ok: boolean; diagnostics: Diagnostic[] } {
  const program = ts.createProgram([path.join(root, sourcePath)], {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ES2022,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    strict: true,
    skipLibCheck: true,
    noEmit: true,
  });
  const diagnostics = ts
    .getPreEmitDiagnostics(program)
    .filter((item) => item.category === ts.DiagnosticCategory.Error)
    .map((item) =>
      diagnosticError(
        "DIRECT-TS-TYPECHECK-001",
        item.file ? path.relative(root, item.file.fileName) : sourcePath,
        ts.flattenDiagnosticMessageText(item.messageText, "\n"),
      ),
    );
  return { ok: diagnostics.length === 0, diagnostics };
}

async function importDirectTsModule(
  root: string,
  sourcePath: string,
): Promise<Record<string, unknown>> {
  const source = await readFile(path.join(root, sourcePath), "utf8");
  const transpiled = ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ES2022,
      moduleResolution: ts.ModuleResolutionKind.Bundler,
      strict: true,
    },
  });
  const moduleUrl = `data:text/javascript;base64,${Buffer.from(transpiled.outputText, "utf8").toString("base64")}`;
  return (await import(moduleUrl)) as Record<string, unknown>;
}

async function verifyDirectTsCase(
  action: DirectTsAction,
  testCase: BenchmarkCase,
): Promise<DirectTsCaseResult> {
  const effects: string[] = [];
  try {
    const rawResult = action(testCase.input, {
      write(value: string): void {
        effects.push(value);
      },
    });
    const result = rawResult === undefined ? null : rawResult;
    const resultOk = deepEqualJson(result, testCase.expected_result);
    const effectsOk = deepEqualJson(effects, testCase.expected_effects);
    return {
      name: testCase.name,
      ok: resultOk && effectsOk,
      result_ok: resultOk,
      effects_ok: effectsOk,
      run_ok: true,
      actual_result: result,
      actual_effects: effects,
      diagnostics: [],
    };
  } catch (error) {
    return {
      name: testCase.name,
      ok: false,
      result_ok: false,
      effects_ok: false,
      run_ok: false,
      actual_result: null,
      actual_effects: effects,
      diagnostics: [
        diagnosticError(
          "DIRECT-TS-RUN-001",
          "candidate.ts",
          error instanceof Error ? error.message : String(error),
        ),
      ],
    };
  }
}

function failedDirectTsResult(options: {
  options: DirectTsExperimentOptions;
  workspace: string;
  steps: Array<Record<string, unknown>>;
  failureType: string;
}): DirectTsExperimentResult {
  return {
    ok: false,
    mode: "direct-ts",
    task_id: options.options.task.id,
    model: options.options.model,
    workspace: options.workspace,
    graph_dir: null,
    goal_node: null,
    pseudocode_node: null,
    code_node: null,
    repairs_used: 0,
    design_revisions_used: 0,
    failure_type: options.failureType,
    steps: options.steps,
    verification: null,
  };
}
