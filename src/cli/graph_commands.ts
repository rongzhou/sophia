import type { Command } from "commander";
import { registerGraphDecisionCommands } from "./graph_decision_commands.js";
import { registerGraphImplementationCommands } from "./graph_implementation_commands.js";
import { registerGraphGoalCommands } from "./graph_goal_commands.js";
import { registerGraphPseudocodeCommands } from "./graph_pseudocode_commands.js";
import { registerGraphMaterializeCommands } from "./graph_materialize_commands.js";
import { GraphStore } from "../graph/core/store.js";

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

  registerGraphPseudocodeCommands(graph);
  registerGraphImplementationCommands(graph);
  registerGraphGoalCommands(graph);
  registerGraphDecisionCommands(graph);
  registerGraphMaterializeCommands(graph);
}
