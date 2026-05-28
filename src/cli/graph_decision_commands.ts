import type { Command } from "commander";
import { printJson, setFailedExitIf } from "./cli_utils.js";
import { applyDecisionNode, summarizeAppliedDecision } from "../graph/decision/apply.js";
import { decideNextActionWithOllama } from "../graph/decision/llm.js";
import {
  createArtifactDiffNode,
  createAuditNode,
  createCheckResultNode,
  createSelectionNode,
  inferBeforeCodeNodeForDiff,
  readCodeNodeFiles,
  summarizeResultNode,
} from "../graph/workflow/code.js";
import { buildGraphReport } from "../graph/core/report.js";
import { captureRawLlmFailure } from "../graph/workflow/llm_failure.js";
import { GraphStore } from "../graph/core/store.js";
import { assertNodeType } from "../graph/core/nodes.js";

export function registerGraphDecisionCommands(graph: Command): void {
  graph
    .command("decide")
    .argument("<node>")
    .requiredOption("--model <model>", "Ollama model name")
    .description("Create a DecisionNode with the next LLM heuristic workflow action")
    .action(async (nodeId: string, options: { model: string }) => {
      const store = new GraphStore(process.cwd());
      const currentNode = await store.readNode(nodeId);
      const result = await captureRawLlmFailure({
        store,
        createdFrom: currentNode,
        actionUsed: "llm_decide",
        edgeType: "decides",
        tags: ["llm", "decision", "failed"],
        model: options.model,
        goal: currentNode.goal,
        call: () =>
          decideNextActionWithOllama({
            store,
            currentNode,
            model: options.model,
          }),
      });
      const node = await store.createNode({
        type: "DecisionNode",
        createdFrom: currentNode.id,
        actionUsed: "llm_decide",
        goal: currentNode.goal,
        summary: `LLM decision for ${currentNode.id}: ${result.decision.selected_action}.`,
        artifacts: ["prompt.txt", "response.txt", "result.json", "baseline.json"],
        tags: ["decision", "llm", result.decision.selected_action],
        model: options.model,
        promptArtifact: "prompt.txt",
        responseArtifact: "response.txt",
      });
      await store.writeArtifact(node, "prompt.txt", result.prompt);
      await store.writeArtifact(node, "response.txt", result.rawResponse);
      await store.writeArtifactJson(node, "result.json", result.decision);
      await store.writeArtifactJson(node, "baseline.json", result.baseline);
      await store.appendEdge({ from: currentNode.id, to: node.id, type: "decides" });
      printJson({ node, decision: result.decision, baseline: result.baseline });
    });

  graph
    .command("apply")
    .argument("<decision-node>")
    .description("Apply a DecisionNode when the selected action has a deterministic executor")
    .action(async (decisionNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const decisionNode = await store.readNode(decisionNodeId);
      const result = await applyDecisionNode(store, decisionNode);
      printJson(summarizeAppliedDecision(result));
      setFailedExitIf(!result.ok);
    });

  graph
    .command("check")
    .argument("<code-node>")
    .description("Run deterministic checks for a CodeNode")
    .action(async (codeNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const codeNode = await store.readNode(codeNodeId);
      assertNodeType(codeNode, "CodeNode");

      const { result } = await createCheckResultNode({
        store,
        codeNode,
        files: await readCodeNodeFiles(store, codeNode),
      });
      printJson(result);
      setFailedExitIf(!result.ok);
    });

  graph
    .command("audit")
    .argument("<code-node>")
    .description("Audit a CodeNode against its ancestor .pseudo constraints")
    .action(async (codeNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const codeNode = await store.readNode(codeNodeId);
      assertNodeType(codeNode, "CodeNode");
      const { result } = await createAuditNode({
        store,
        codeNode,
        files: await readCodeNodeFiles(store, codeNode),
      });
      printJson(result);
      setFailedExitIf(!result.ok);
    });

  graph
    .command("diff")
    .argument("<after-code-node>")
    .argument("[before-code-node]")
    .description("Compare a CodeNode with its repaired ancestor or an explicit before CodeNode")
    .action(async (afterCodeNodeId: string, beforeCodeNodeId?: string) => {
      const store = new GraphStore(process.cwd());
      const afterCodeNode = await store.readNode(afterCodeNodeId);
      assertNodeType(afterCodeNode, "CodeNode");
      const beforeCodeNode = beforeCodeNodeId
        ? await store.readNode(beforeCodeNodeId)
        : await inferBeforeCodeNodeForDiff(store, afterCodeNode);
      assertNodeType(beforeCodeNode, "CodeNode");

      const diff = await createArtifactDiffNode({
        store,
        beforeNode: beforeCodeNode,
        afterNode: afterCodeNode,
        beforeFiles: await readCodeNodeFiles(store, beforeCodeNode),
        afterFiles: await readCodeNodeFiles(store, afterCodeNode),
      });
      printJson(diff.result);
    });

  graph
    .command("verify")
    .argument("<code-node>")
    .description("Run deterministic check, audit, and repair diff when applicable")
    .action(async (codeNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const codeNode = await store.readNode(codeNodeId);
      assertNodeType(codeNode, "CodeNode");
      const files = await readCodeNodeFiles(store, codeNode);
      const beforeNode =
        codeNode.action_used === "repair_code"
          ? await inferBeforeCodeNodeForDiff(store, codeNode)
          : null;
      const diff = beforeNode
        ? await createArtifactDiffNode({
            store,
            beforeNode,
            afterNode: codeNode,
            beforeFiles: await readCodeNodeFiles(store, beforeNode),
            afterFiles: files,
          })
        : null;
      const check = await createCheckResultNode({ store, codeNode, files });
      const audit = await createAuditNode({ store, codeNode, files });
      const ok = check.result.ok && audit.result.ok && (diff?.result.ok ?? true);
      console.log(
        JSON.stringify(
          {
            ok,
            code_node: codeNode.id,
            diff: diff ? summarizeResultNode(diff.node, diff.result) : null,
            check: summarizeResultNode(check.node, check.result),
            audit: summarizeResultNode(audit.node, audit.result),
          },
          null,
          2,
        ),
      );
      setFailedExitIf(!ok);
    });

  graph
    .command("select")
    .argument("<code-node>")
    .description("Select a CodeNode only after deterministic check and audit pass")
    .action(async (codeNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const codeNode = await store.readNode(codeNodeId);
      assertNodeType(codeNode, "CodeNode");
      const selectionNode = await createSelectionNode({ store, codeNode });
      printJson(selectionNode);
    });

  graph
    .command("report")
    .description("Summarize graph experiment status and diagnostics")
    .action(async () => {
      const store = new GraphStore(process.cwd());
      const nodes = await store.listNodes();
      const edges = await store.listEdges();
      const report = await buildGraphReport(store, nodes, edges);
      printJson(report);
    });
}
