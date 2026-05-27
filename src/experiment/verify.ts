import { mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { runTypeScriptAction } from "../backend/ts_runner.js";
import {
  type TypeScriptTypecheckResult,
  typecheckGeneratedTypeScript,
} from "../backend/ts_typecheck.js";
import { checkSophiaFiles } from "../lang/checker.js";
import type { CheckResult, Diagnostic } from "../lang/diagnostics.js";
import { sophiaTomlTemplate } from "../workspace/workspace.js";
import { createScratchDirectory, SOPHIA_BUILD_DIR } from "../workspace/fs_layout.js";
import { buildTypeScript, type TypeScriptBuildResult } from "../backend/ts_codegen.js";
import { deepEqualJson } from "../util/json.js";
import type { BenchmarkCase, BenchmarkTask } from "./task.js";

export interface BenchmarkCaseResult {
  name: string;
  ok: boolean;
  result_ok: boolean;
  effects_ok: boolean;
  run_ok: boolean;
  actual_result: unknown;
  actual_effects: string[];
  diagnostics: Diagnostic[];
}

export interface BenchmarkVerificationResult {
  ok: boolean;
  action: string;
  check: CheckResult;
  build: TypeScriptBuildResult | null;
  typecheck: TypeScriptTypecheckResult | null;
  cases: BenchmarkCaseResult[];
}

export async function verifySophiaFilesAgainstTask(
  files: Record<string, string>,
  task: BenchmarkTask,
  options: { action?: string; scratchRoot?: string } = {},
): Promise<BenchmarkVerificationResult> {
  const action = options.action ?? task.scaffold.action;
  const check = checkSophiaFiles(files);
  if (!check.ok) {
    return {
      ok: false,
      action,
      check,
      build: null,
      typecheck: null,
      cases: [],
    };
  }

  const root = await createScratchDirectory(options.scratchRoot ?? process.cwd(), "benchmark");
  try {
    await writeFile(path.join(root, "sophia.toml"), `${sophiaTomlTemplate("benchmark")}\n`, "utf8");
    for (const [filePath, content] of Object.entries(files)) {
      const absolutePath = path.join(root, filePath);
      await mkdir(path.dirname(absolutePath), { recursive: true });
      await writeFile(absolutePath, content, "utf8");
    }

    const build = await buildTypeScript(root);
    if (!build.ok) {
      return {
        ok: false,
        action,
        check,
        build,
        typecheck: null,
        cases: [],
      };
    }

    const typecheck = typecheckGeneratedTypeScript(
      root,
      build.files[0] ?? `${SOPHIA_BUILD_DIR}/index.ts`,
    );
    if (!typecheck.ok) {
      return {
        ok: false,
        action,
        check,
        build,
        typecheck,
        cases: [],
      };
    }

    const cases = await Promise.all(
      task.hidden_cases.map(async (testCase) => verifyCase(root, action, testCase)),
    );
    return {
      ok: cases.every((testCase) => testCase.ok),
      action,
      check,
      build,
      typecheck,
      cases,
    };
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function verifyCase(
  root: string,
  action: string,
  testCase: BenchmarkCase,
): Promise<BenchmarkCaseResult> {
  const run = await runTypeScriptAction(root, action, testCase.input);
  const resultOk = deepEqualJson(run.result, testCase.expected_result);
  const effectsOk = deepEqualJson(run.effects, testCase.expected_effects);
  return {
    name: testCase.name,
    ok: run.ok && resultOk && effectsOk,
    result_ok: resultOk,
    effects_ok: effectsOk,
    run_ok: run.ok,
    actual_result: run.result,
    actual_effects: run.effects,
    diagnostics: run.diagnostics,
  };
}
