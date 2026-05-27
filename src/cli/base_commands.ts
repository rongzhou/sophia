import type { Command } from "commander";
import {
  parseJsonObjectOption,
  parseJsonOption,
  readSophiaFilesFromDomains,
  setFailedExitIf,
} from "./cli_utils.js";
import { buildTypeScript } from "../backend/ts_codegen.js";
import { runTypeScriptAction, smokeTypeScriptActions } from "../backend/ts_runner.js";
import { typecheckGeneratedTypeScript } from "../backend/ts_typecheck.js";
import { checkSophiaFiles } from "../lang/checker.js";
import { error, type CheckResult } from "../lang/diagnostics.js";
import { buildAsgIndex } from "../analysis/indexer.js";
import { buildActionContext } from "../analysis/context.js";
import { parseSophiaFile, parseSophiaSource } from "../lang/parser.js";
import { buildRepairContext } from "../analysis/repair_context.js";
import { initWorkspace, loadWorkspaceConfig } from "../workspace/workspace.js";
import { readCodeNodeFiles } from "../graph/code_workflow.js";
import { GraphStore } from "../graph/store.js";
import { checkStripAssistTypeScriptEquivalence } from "../backend/strip_assist_equivalence.js";
import type { SophiaSourceFile } from "../backend/ts_emit_module.js";

export function registerBaseCommands(program: Command): void {
  program
    .command("init")
    .description("Initialize a Sophia workspace")
    .action(async () => {
      const result = await initWorkspace(process.cwd());
      console.log(JSON.stringify(result, null, 2));
    });

  program
    .command("check")
    .description("Check materialized domains/**/*.sophia files")
    .action(async () => {
      const files = await readSophiaFilesFromDomains(process.cwd());
      const result = await checkWorkspaceFiles(process.cwd(), files);
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  program
    .command("index")
    .description("Generate the configured ASG index from materialized domains/**/*.sophia files")
    .action(async () => {
      const result = await buildAsgIndex(process.cwd());
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  program
    .command("context")
    .requiredOption("--action <ActionName>", "Action root for the deterministic semantic context")
    .description("Generate deterministic semantic context from an action root")
    .action(async (options: { action: string }) => {
      const files = await readSophiaFilesFromDomains(process.cwd());
      const result = buildActionContext(files, options.action);
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  program
    .command("build")
    .description("Build materialized domains/**/*.sophia into deterministic TypeScript")
    .action(async () => {
      const result = await buildTypeScript(process.cwd());
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  program
    .command("run")
    .argument("<ActionName>")
    .option("--input <json>", "JSON object passed to the generated action", "{}")
    .description("Build and run a generated Sophia TypeScript action")
    .action(async (action: string, options: { input: string }) => {
      const input = parseJsonOption(options.input, "--input");
      const result = await runTypeScriptAction(process.cwd(), action, input);
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  program
    .command("smoke")
    .option("--inputs <json>", "JSON object mapping action names to sample input objects", "{}")
    .option(
      "--auto-inputs",
      "Generate type-valid sample inputs for actions without explicit inputs",
    )
    .description("Build and run deterministic smoke checks for generated actions")
    .action(async (options: { inputs: string; autoInputs?: boolean }) => {
      const inputs = parseJsonObjectOption(options.inputs, "--inputs");
      const result = await smokeTypeScriptActions(process.cwd(), {
        inputs,
        autoInputs: Boolean(options.autoInputs),
      });
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  program
    .command("verify")
    .option("--inputs <json>", "JSON object mapping action names to sample input objects", "{}")
    .option(
      "--auto-inputs",
      "Generate type-valid sample inputs for actions without explicit inputs",
    )
    .description("Run deterministic check, build, generated typecheck, and smoke gates")
    .action(async (options: { inputs: string; autoInputs?: boolean }) => {
      const inputs = parseJsonObjectOption(options.inputs, "--inputs");
      const files = await readSophiaFilesFromDomains(process.cwd());
      const check = await checkWorkspaceFiles(process.cwd(), files);
      const build = check.ok ? await buildTypeScript(process.cwd()) : null;
      const typecheck =
        build?.ok && build.files[0]
          ? typecheckGeneratedTypeScript(process.cwd(), build.files[0])
          : null;
      const smoke = typecheck?.ok
        ? await smokeTypeScriptActions(process.cwd(), {
            inputs,
            autoInputs: Boolean(options.autoInputs),
          })
        : null;
      const result = {
        ok: check.ok && build?.ok === true && typecheck?.ok === true && smoke?.ok === true,
        check,
        build,
        typecheck,
        smoke,
      };
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  program
    .command("parse")
    .argument("<file>")
    .description("Parse a single .sophia file into a deterministic raw AST summary")
    .action(async (file: string) => {
      const result = await parseSophiaFile(file);
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  program
    .command("repair-context")
    .argument("<check-node>")
    .description(
      "Generate deterministic LLM repair context from a CheckResultNode without calling a model",
    )
    .action(async (checkNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const checkNode = await store.readNode(checkNodeId);
      if (checkNode.type !== "CheckResultNode") {
        throw new Error(`Expected CheckResultNode, got ${checkNode.type}.`);
      }
      if (!checkNode.created_from) {
        throw new Error(`CheckResultNode ${checkNode.id} does not reference a CodeNode.`);
      }
      const codeNode = await store.readNode(checkNode.created_from);
      if (codeNode.type !== "CodeNode") {
        throw new Error(`Expected checked node to be CodeNode, got ${codeNode.type}.`);
      }
      const checkResult = await store.readArtifactJson<CheckResult>(checkNode, "result.json");
      const files = await readCodeNodeFiles(store, codeNode);
      const context = buildRepairContext({ files, checkResult });
      console.log(
        JSON.stringify(
          {
            check_node: checkNode.id,
            code_node: codeNode.id,
            ok: checkResult.ok,
            context,
          },
          null,
          2,
        ),
      );
    });
}

async function checkWorkspaceFiles(
  root: string,
  files: Record<string, string>,
): Promise<CheckResult> {
  const check = checkSophiaFiles(files);
  if (!check.ok) return check;

  const config = await loadWorkspaceConfig(root);
  if (!config.check.require_strip_assist_equivalence) return check;

  const parsedFiles = parseSourceFilesForStripAssist(files);
  if ("diagnostics" in parsedFiles) {
    return {
      ok: false,
      diagnostics: [...check.diagnostics, ...parsedFiles.diagnostics],
    };
  }

  const stripAssist = checkStripAssistTypeScriptEquivalence(parsedFiles);
  if (stripAssist.ok) return check;
  return {
    ok: false,
    diagnostics: [...check.diagnostics, ...stripAssist.diagnostics],
  };
}

function parseSourceFilesForStripAssist(
  files: Record<string, string>,
): SophiaSourceFile[] | { diagnostics: CheckResult["diagnostics"] } {
  const parsedFiles: SophiaSourceFile[] = [];
  const diagnostics: CheckResult["diagnostics"] = [];
  for (const [filePath, content] of Object.entries(files).sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    if (!filePath.endsWith(".sophia")) continue;
    const parsed = parseSophiaSource(content, filePath);
    if (!parsed.ok || !parsed.ast) {
      diagnostics.push(
        ...parsed.diagnostics.map((diagnostic) =>
          error(diagnostic.code, diagnostic.location, diagnostic.problem),
        ),
      );
      continue;
    }
    parsedFiles.push({ path: filePath, ast: parsed.ast });
  }
  return diagnostics.length > 0 ? { diagnostics } : parsedFiles;
}
