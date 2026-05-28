import { readFile } from "node:fs/promises";
import type { Command } from "commander";
import { isNodeId, parseNonNegativeIntegerOption, printJson, setFailedExitIf } from "./cli_utils.js";
import {
  countDesignRevisionAttempts,
  createDesignBudgetExhaustedNode,
  createRevisedDesignNode,
  createDesignedPseudocodeNode,
} from "../graph/workflow/llm_node.js";
import { captureRawLlmFailure } from "../graph/workflow/llm_failure.js";
import { createPseudocodeCheckNode } from "../graph/workflow/pseudocode.js";
import { GraphStore } from "../graph/core/store.js";
import { assertNodeType } from "../graph/core/nodes.js";
import { reviseDesignWithOllama } from "../llm/tasks/revise_design.js";
import { designSolutionWithOllama } from "../llm/tasks/design_solution.js";
import { checkPseudocode } from "../pseudo/check.js";
import { outlinePseudocode } from "../pseudo/outline.js";
import { buildSophiaScaffold } from "../pseudo/scaffold.js";
import { buildImplementationStructurePlan } from "../pseudo/structure_plan.js";

export function registerGraphPseudocodeCommands(graph: Command): void {
  graph
    .command("design")
    .argument("<goal-node>")
    .requiredOption("--model <model>", "Ollama model name")
    .description("Design a PseudocodeNode from a GoalNode using Ollama, then run pseudo-check")
    .action(async (goalNodeId: string, options: { model: string }) => {
      const store = new GraphStore(process.cwd());
      const goalNode = await store.readNode(goalNodeId);
      assertNodeType(goalNode, "GoalNode");
      if (!goalNode.goal) {
        throw new Error(`GoalNode ${goalNode.id} does not contain goal text.`);
      }
      const goalText = goalNode.goal;

      const result = await captureRawLlmFailure({
        store,
        createdFrom: goalNode,
        actionUsed: "design_solution",
        edgeType: "designs_solution",
        tags: ["llm", "pseudo", "design", "failed"],
        model: options.model,
        goal: goalText,
        call: () =>
          designSolutionWithOllama({
            goal: goalText,
            model: options.model,
          }),
      });
      const pseudoNode = await createDesignedPseudocodeNode({
        store,
        goalNode,
        result,
        model: options.model,
        ...(result.output.status === "needs_clarification" ? { status: "failed" } : {}),
      });
      const checkResult = checkPseudocode(result.output.pseudocode);
      const checkNode = await createPseudocodeCheckNode({
        store,
        pseudoNode,
        pseudocode: result.output.pseudocode,
        summary: checkResult.ok
          ? "Designed pseudocode check passed."
          : `Designed pseudocode check failed with ${checkResult.diagnostics.length} diagnostic(s).`,
        tags: ["pseudo", "check", "design"],
      });
      console.log(
        JSON.stringify(
          {
            node: pseudoNode,
            check: checkNode.node,
            result: checkResult,
            questions: result.output.questions,
          },
          null,
          2,
        ),
      );
      setFailedExitIf(result.output.status !== "designed" || !checkResult.ok);
    });

  graph
    .command("pseudo-check")
    .argument("<pseudo-node-or-file>")
    .description("Check a .pseudo node or file")
    .action(async (target: string) => {
      const store = new GraphStore(process.cwd());
      const sourceNode = isNodeId(target) ? await store.readNode(target) : null;
      const content = sourceNode
        ? await store.readArtifact(sourceNode, "content.pseudo")
        : await readFile(target, "utf8");
      const result = checkPseudocode(content);
      if (sourceNode) {
        await createPseudocodeCheckNode({
          store,
          pseudoNode: sourceNode,
          pseudocode: content,
          summary: result.ok ? "Pseudocode check passed." : "Pseudocode check failed.",
        });
      }
      printJson(result);
      setFailedExitIf(!result.ok);
    });

  graph
    .command("pseudo-outline")
    .argument("<file>")
    .description("Extract an algorithm outline from a .pseudo file")
    .action(async (file: string) => {
      const content = await readFile(file, "utf8");
      printJson(outlinePseudocode(content));
    });

  graph
    .command("pseudo-structure")
    .argument("<file>")
    .description("Extract deterministic implementation structure hints from a .pseudo file")
    .action(async (file: string) => {
      const content = await readFile(file, "utf8");
      printJson(buildImplementationStructurePlan(content));
    });

  graph
    .command("pseudo-scaffold")
    .argument("<pseudo-node-or-file>")
    .description("Generate deterministic .sophia scaffold files from .pseudo structure")
    .action(async (target: string) => {
      const store = new GraphStore(process.cwd());
      const sourceNode = isNodeId(target) ? await store.readNode(target) : null;
      const content = sourceNode
        ? await store.readArtifact(sourceNode, "content.pseudo")
        : await readFile(target, "utf8");
      printJson(buildSophiaScaffold(content));
    });

  graph
    .command("revise-design")
    .argument("<pseudo-check-node>")
    .requiredOption("--model <model>", "Ollama model name")
    .option(
      "--max-design-revisions <count>",
      "Maximum design revision attempts in an ancestry chain",
      "2",
    )
    .description("Revise a failed or warning PseudocodeCheckNode using Ollama")
    .action(async (checkNodeId: string, options: { model: string; maxDesignRevisions: string }) => {
      const store = new GraphStore(process.cwd());
      const checkNode = await store.readNode(checkNodeId);
      assertNodeType(checkNode, "PseudocodeCheckNode");
      if (!checkNode.created_from) {
        throw new Error(`PseudocodeCheckNode ${checkNode.id} does not reference a PseudocodeNode.`);
      }
      const pseudoNode = await store.readNode(checkNode.created_from);
      assertNodeType(pseudoNode, "PseudocodeNode");
      const maxRevisions = parseNonNegativeIntegerOption(
        options.maxDesignRevisions,
        "--max-design-revisions",
      );
      const revisionsUsed = await countDesignRevisionAttempts(store, pseudoNode);
      if (revisionsUsed >= maxRevisions) {
        await createDesignBudgetExhaustedNode({
          store,
          sourcePseudoNode: pseudoNode,
          checkNode,
          revisionsUsed,
          model: options.model,
        });
        throw new Error(`Pseudocode revision budget exhausted for ${pseudoNode.id}.`);
      }

      const pseudocode = await store.readArtifact(pseudoNode, "content.pseudo");
      const checkResult = await store.readArtifactJson<ReturnType<typeof checkPseudocode>>(
        checkNode,
        "result.json",
      );
      if (checkResult.ok && checkResult.diagnostics.length === 0) {
        throw new Error(
          `PseudocodeCheckNode ${checkNode.id} has no diagnostics; refusing to revise.`,
        );
      }

      const revision = await captureRawLlmFailure({
        store,
        createdFrom: checkNode,
        actionUsed: "revise_design",
        edgeType: "revises",
        tags: ["llm", "pseudo", "revise", "failed"],
        model: options.model,
        goal: pseudoNode.goal,
        call: () =>
          reviseDesignWithOllama({
            pseudocode,
            checkResult,
            model: options.model,
          }),
      });
      if (revision.output.status === "needs_clarification") {
        const failedNode = await createRevisedDesignNode({
          store,
          sourcePseudoNode: pseudoNode,
          checkNode,
          result: revision,
          model: options.model,
          status: "failed",
          tags: ["pseudo", "revise", "failed", "needs_clarification"],
          summary: `Pseudocode revision needs clarification for ${pseudoNode.id}.`,
        });
        console.log(
          JSON.stringify(
            {
              ok: false,
              node: failedNode,
              reason: "needs_clarification",
              questions: revision.output.questions,
            },
            null,
            2,
          ),
        );
        setFailedExitIf(true);
        return;
      }
      const revisedNode = await createRevisedDesignNode({
        store,
        sourcePseudoNode: pseudoNode,
        checkNode,
        result: revision,
        model: options.model,
      });
      const revisedCheck = checkPseudocode(revision.output.pseudocode);
      const revisedCheckNode = await createPseudocodeCheckNode({
        store,
        pseudoNode: revisedNode,
        pseudocode: revision.output.pseudocode,
        summary: revisedCheck.ok
          ? "Revised pseudocode check passed."
          : `Revised pseudocode check failed with ${revisedCheck.diagnostics.length} diagnostic(s).`,
        tags: ["pseudo", "check", "revise"],
      });
      console.log(
        JSON.stringify(
          { node: revisedNode, check: revisedCheckNode.node, result: revisedCheck },
          null,
          2,
        ),
      );
      setFailedExitIf(!revisedCheck.ok);
    });

  graph
    .command("add-pseudo")
    .argument("<goal-node>")
    .argument("<file>")
    .description("Create a PseudocodeNode from a .pseudo file")
    .action(async (goalNodeId: string, file: string) => {
      const store = new GraphStore(process.cwd());
      const goalNode = await store.readNode(goalNodeId);
      const content = await readFile(file, "utf8");
      const node = await store.createNode({
        type: "PseudocodeNode",
        createdFrom: goalNode.id,
        actionUsed: "add_pseudo",
        goal: goalNode.goal,
        summary: `Pseudocode from ${file}.`,
        artifacts: ["content.pseudo"],
        tags: ["pseudo"],
      });
      await store.writeArtifact(node, "content.pseudo", content);
      await store.appendEdge({ from: goalNode.id, to: node.id, type: "designs_solution" });
      printJson(node);
    });
}
