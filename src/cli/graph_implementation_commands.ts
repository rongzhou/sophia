import type { Command } from "commander";
import { parseNonNegativeIntegerOption, setFailedExitIf } from "./cli_utils.js";
import { checkSophiaFiles } from "../lang/checker.js";
import {
  createArtifactDiffNode,
  findAncestorPseudocodeNode,
  readCodeNodeFiles,
} from "../graph/code_workflow.js";
import { runCheckRepairLoop } from "../graph/check_repair_loop.js";
import { captureRawLlmFailure } from "../graph/llm_failure_capture.js";
import {
  DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT,
  assertCodeRepairBudgetAvailable,
  createImplementedCodeNode,
  createRepairedCodeNode,
} from "../graph/llm_node_workflow.js";
import type { GraphNode } from "../graph/nodes.js";
import { assertPseudocodeNodeCanImplement } from "../graph/pseudocode_workflow.js";
import { GraphStore } from "../graph/store.js";
import { implementDesignWithOllama } from "../llm/tasks/implement_design.js";
import { repairCodeWithOllama } from "../llm/tasks/repair.js";

export function registerGraphImplementationCommands(graph: Command): void {
  graph
    .command("implement")
    .argument("<pseudo-node>")
    .requiredOption("--model <model>", "Ollama model name")
    .description("Implement a PseudocodeNode into a candidate CodeNode using Ollama")
    .action(async (pseudoNodeId: string, options: { model: string }) => {
      const store = new GraphStore(process.cwd());
      const pseudoNode = await store.readNode(pseudoNodeId);
      if (pseudoNode.type !== "PseudocodeNode") {
        throw new Error(`Expected PseudocodeNode, got ${pseudoNode.type}.`);
      }
      await assertPseudocodeNodeCanImplement(store, pseudoNode);
      const pseudocode = await store.readArtifact(pseudoNode, "content.pseudo");

      const result = await captureRawLlmFailure({
        store,
        createdFrom: pseudoNode,
        action_used: "implement_design",
        edgeType: "implements_design",
        tags: ["llm", "failed"],
        model: options.model,
        call: () => implementDesignWithOllama({ pseudocode, model: options.model }),
      });
      const codeNode = await createImplementedCodeNode({
        store,
        pseudoNode,
        result,
        model: options.model,
      });
      console.log(JSON.stringify(codeNode, null, 2));
    });

  graph
    .command("implement-loop")
    .argument("<pseudo-node>")
    .requiredOption("--model <model>", "Ollama model name")
    .option(
      "--max-repairs <count>",
      "Maximum repair attempts after failed checks",
      String(DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT),
    )
    .description("Run implementation plus bounded deterministic check/repair/audit loop")
    .action(async (pseudoNodeId: string, options: { model: string; maxRepairs: string }) => {
      const store = new GraphStore(process.cwd());
      const pseudoNode = await store.readNode(pseudoNodeId);
      if (pseudoNode.type !== "PseudocodeNode") {
        throw new Error(`Expected PseudocodeNode, got ${pseudoNode.type}.`);
      }
      await assertPseudocodeNodeCanImplement(store, pseudoNode);
      const maxRepairs = parseNonNegativeIntegerOption(options.maxRepairs, "--max-repairs");
      const pseudocode = await store.readArtifact(pseudoNode, "content.pseudo");
      const summary: Array<Record<string, unknown>> = [];

      let currentCodeNode: GraphNode;
      const implementation = await captureRawLlmFailure({
        store,
        createdFrom: pseudoNode,
        action_used: "implement_design",
        edgeType: "implements_design",
        tags: ["llm", "failed", "implement_loop"],
        model: options.model,
        call: () => implementDesignWithOllama({ pseudocode, model: options.model }),
      });
      currentCodeNode = await createImplementedCodeNode({
        store,
        pseudoNode,
        result: implementation,
        model: options.model,
      });
      summary.push({ step: "implement", code_node: currentCodeNode.id });

      const loop = await runCheckRepairLoop({
        store,
        initialCodeNode: currentCodeNode,
        pseudocode,
        model: options.model,
        maxRepairs,
        repairFailureTags: ["llm", "repair", "failed", "implement_loop"],
      });
      summary.push(...loop.steps);

      if (loop.kind === "budget_exhausted") {
        console.log(
          JSON.stringify(
            {
              ok: false,
              reason: loop.reason,
              final_code_node: loop.codeNode.id,
              repairs_used: loop.repairsUsed,
              diff_ok: loop.diffOk,
              steps: summary,
            },
            null,
            2,
          ),
        );
        setFailedExitIf(true);
        return;
      }

      const ok = loop.diffOk;
      console.log(
        JSON.stringify(
          {
            ok,
            final_code_node: loop.codeNode.id,
            repairs_used: loop.repairsUsed,
            diff_ok: loop.diffOk,
            steps: summary,
          },
          null,
          2,
        ),
      );
      setFailedExitIf(!ok);
      return;
    });

  graph
    .command("repair")
    .argument("<result-node>")
    .requiredOption("--model <model>", "Ollama model name")
    .option(
      "--max-repairs <count>",
      "Maximum repair attempts for the checked CodeNode",
      String(DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT),
    )
    .description("Repair a failed CheckResultNode or AuditNode using Ollama")
    .action(async (checkNodeId: string, options: { model: string; maxRepairs: string }) => {
      const store = new GraphStore(process.cwd());
      const checkNode = await store.readNode(checkNodeId);
      if (checkNode.type !== "CheckResultNode" && checkNode.type !== "AuditNode") {
        throw new Error(`Expected CheckResultNode or AuditNode, got ${checkNode.type}.`);
      }
      if (!checkNode.created_from) {
        throw new Error(`CheckResultNode ${checkNode.id} does not reference a CodeNode.`);
      }
      const codeNode = await store.readNode(checkNode.created_from);
      if (codeNode.type !== "CodeNode") {
        throw new Error(`Expected checked node to be CodeNode, got ${codeNode.type}.`);
      }
      const checkResult = await store.readArtifactJson<ReturnType<typeof checkSophiaFiles>>(
        checkNode,
        "result.json",
      );
      if (checkResult.ok) {
        throw new Error(`CheckResultNode ${checkNode.id} already passed; refusing to repair.`);
      }
      const maxRepairs = parseNonNegativeIntegerOption(options.maxRepairs, "--max-repairs");
      await assertCodeRepairBudgetAvailable({ store, codeNode, maxRepairs });

      const files = await readCodeNodeFiles(store, codeNode);
      const pseudoNode = await findAncestorPseudocodeNode(store, codeNode.id);
      const pseudocode = await store.readArtifact(pseudoNode, "content.pseudo");

      const result = await captureRawLlmFailure({
        store,
        createdFrom: checkNode,
        action_used: "repair_code",
        edgeType: "repairs",
        tags: ["llm", "repair", "failed"],
        model: options.model,
        ...(codeNode.goal ? { goal: codeNode.goal } : {}),
        call: () =>
          repairCodeWithOllama({
            files,
            checkResult,
            model: options.model,
            pseudocode,
          }),
      });
      const repairedNode = await createRepairedCodeNode({
        store,
        sourceCodeNode: codeNode,
        checkNode,
        result,
        model: options.model,
      });
      await createArtifactDiffNode({
        store,
        beforeNode: codeNode,
        afterNode: repairedNode,
        beforeFiles: files,
        afterFiles: result.output.files,
      });
      console.log(JSON.stringify(repairedNode, null, 2));
    });
}
