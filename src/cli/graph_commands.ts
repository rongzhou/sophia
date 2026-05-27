import type { Command } from "commander";
import { setFailedExitIf } from "./cli_utils.js";
import { registerGraphDeterministicCommands } from "./graph_deterministic_commands.js";
import { registerGraphImplementationCommands } from "./graph_implementation_commands.js";
import { registerGraphPseudocodeCommands } from "./graph_pseudocode_commands.js";
import { registerGraphMaterializeCommands } from "./graph_materialize_commands.js";
import { applyDecisionNode, summarizeAppliedDecision } from "../graph/apply_decision.js";
import { decideNextActionWithOllama } from "../graph/llm_decision.js";
import { createRawLlmFailureNode } from "../graph/llm_node_workflow.js";
import { GraphStore } from "../graph/store.js";
import { isLlmCallError } from "../llm/errors.js";

export function registerGraphCommands(program: Command): void {
  const graph = program.command("graph").description("Manage the Sophia exploration graph");

  graph
    .command("init")
    .description("Initialize sophia-runs/graph")
    .action(async () => {
      const store = new GraphStore(process.cwd());
      await store.init();
      console.log("Initialized sophia-runs/graph.");
    });

  graph
    .command("start")
    .argument("<goal>")
    .description("Create a GoalNode")
    .action(async (goal: string) => {
      const store = new GraphStore(process.cwd());
      const node = await store.createNode({
        type: "GoalNode",
        createdFrom: null,
        action_used: "start",
        goal,
        summary: goal,
        artifacts: ["content.md"],
        tags: ["goal"],
      });
      await store.writeArtifact(node, "content.md", `${goal}\n`);
      console.log(JSON.stringify(node, null, 2));
    });

  graph
    .command("decide")
    .argument("<node>")
    .requiredOption("--model <model>", "Ollama model name")
    .description("Create a DecisionNode with the next LLM heuristic workflow action")
    .action(async (nodeId: string, options: { model: string }) => {
      const store = new GraphStore(process.cwd());
      const currentNode = await store.readNode(nodeId);
      try {
        const result = await decideNextActionWithOllama({
          store,
          currentNode,
          model: options.model,
        });
        const node = await store.createNode({
          type: "DecisionNode",
          createdFrom: currentNode.id,
          action_used: "llm_decide",
          ...(currentNode.goal ? { goal: currentNode.goal } : {}),
          summary: `LLM decision for ${currentNode.id}: ${result.decision.selected_action}.`,
          artifacts: ["prompt.txt", "response.txt", "result.json", "baseline.json"],
          tags: ["decision", "llm", result.decision.selected_action],
          model: options.model,
          promptArtifact: "prompt.txt",
          responseArtifact: "response.txt",
        });
        await store.writeArtifact(node, "prompt.txt", result.prompt);
        await store.writeArtifact(node, "response.txt", result.rawResponse);
        await store.writeArtifact(
          node,
          "result.json",
          `${JSON.stringify(result.decision, null, 2)}\n`,
        );
        await store.writeArtifact(
          node,
          "baseline.json",
          `${JSON.stringify(result.baseline, null, 2)}\n`,
        );
        await store.appendEdge({ from: currentNode.id, to: node.id, type: "decides" });
        console.log(
          JSON.stringify({ node, decision: result.decision, baseline: result.baseline }, null, 2),
        );
      } catch (error) {
        if (isLlmCallError(error)) {
          await createRawLlmFailureNode({
            store,
            createdFrom: currentNode,
            action_used: "llm_decide",
            edgeType: "decides",
            tags: ["llm", "decision", "failed"],
            model: options.model,
            error,
            ...(currentNode.goal ? { goal: currentNode.goal } : {}),
          });
        }
        throw error;
      }
    });

  graph
    .command("apply")
    .argument("<decision-node>")
    .description("Apply a DecisionNode when the selected action has a deterministic executor")
    .action(async (decisionNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const decisionNode = await store.readNode(decisionNodeId);
      const result = await applyDecisionNode(store, decisionNode);
      console.log(JSON.stringify(summarizeAppliedDecision(result), null, 2));
      setFailedExitIf(!result.ok);
    });

  registerGraphPseudocodeCommands(graph);
  registerGraphImplementationCommands(graph);
  registerGraphDeterministicCommands(graph);
  registerGraphMaterializeCommands(graph);
}
