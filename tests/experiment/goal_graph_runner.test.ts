import { readFile } from "node:fs/promises";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import { loadGoalGraphScenario } from "../../src/graph/goal/scenarios.js";
import { runGoalGraphExperiment } from "../../src/experiment/goal_graph_runner.js";

describe("runGoalGraphExperiment", () => {
  it("runs a goal-graph scenario through the benchmark result path", async () => {
    const scenario = await loadGoalGraphScenario(
      path.resolve("benchmarks/L4/wrong_decomposition_retry/scenario.json"),
    );
    const originalCwd = process.cwd();
    const root = await createTempDir("sophia-goal-graph-runner-");
    try {
      process.chdir(root);
      const result = await runGoalGraphExperiment({ scenario, model: "test-model" });

      expect(result).toMatchObject({
        ok: true,
        mode: "goal-graph",
        task_id: "wrong_decomposition_retry",
        model: "test-model",
        repairs_used: 0,
        design_revisions_used: 0,
        failure_type: null,
      });
      expect(result.action_path).toEqual(
        expect.arrayContaining(["invalidate_decomposition", "redecompose_objective"]),
      );
      expect(result.invalidated_branches).toHaveLength(1);
      expect(result.goal_graph_metrics).toMatchObject({
        invalidated_decompositions: 1,
        abandoned_branches: 1,
      });
      expect(result.comparison).toMatchObject({
        fixed_full_workflow: { mode: "full", applicable: false },
        deterministic_decision_baseline: { mode: "deterministic-baseline" },
        llm_goal_graph_decision: { mode: "goal-graph" },
      });

      const report = JSON.parse(
        await readFile(path.join(result.workspace, "graph", "report.json"), "utf8"),
      ) as { goal_workflow?: unknown };
      expect(report.goal_workflow).toBeDefined();
    } finally {
      process.chdir(originalCwd);
    }
  });
});
