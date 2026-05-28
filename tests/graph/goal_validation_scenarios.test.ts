import { describe, expect, it } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import {
  loadGoalGraphScenarioSuite,
  materializeGoalGraphScenario,
} from "../../src/graph/goal/scenarios.js";
import { buildGraphReport } from "../../src/graph/core/report.js";
import { GraphStore } from "../../src/graph/core/store.js";

describe("goal graph validation scenarios", () => {
  it("loads the L4/L5 workflow benchmark scenarios from the main benchmark ladder", async () => {
    const scenarios = await loadGoalGraphScenarioSuite("benchmarks");

    expect(scenarios.map((scenario) => scenario.id)).toEqual([
      "mount_change",
      "multistage_todo",
      "wrong_decomposition_retry",
    ]);
    for (const scenario of scenarios) {
      expect(scenario.records.prompt).toContain("Manual graph run prompt");
      expect(scenario.records.final_verification).toMatchObject({
        check_ok: true,
        audit_ok: true,
        verify_ok: true,
      });
    }
  });

  it("materializes the multi-stage Todo scenario with priority regression context", async () => {
    const record = await materializeScenario("multistage_todo");

    expect(record.records.llm_response).toMatchObject({
      selected_path: expect.arrayContaining(["record_change_request", "analyze_change_impact"]),
    });
    expect(record.active_context.accepted_changes).toEqual([
      expect.objectContaining({
        request:
          "Add a priority field to todo items while keeping existing title behavior unchanged.",
      }),
    ]);
    expect(record.active_context.regression_constraints).toContain(
      "Existing title-only create/list checks must still pass.",
    );
    expect(record.active_context.out_of_scope).toEqual(
      expect.arrayContaining(["Priority sorting", "Priority editing"]),
    );
    expect(record.graph.nodes.length).toBeGreaterThan(0);
    expect(record.graph.edges.length).toBeGreaterThan(0);
  });

  it("materializes wrong decomposition retry while excluding the invalidated branch", async () => {
    const record = await materializeScenario("wrong_decomposition_retry");

    expect(record.active_context.excluded.objectives).toContain(record.refs.bad_platform_rewrite);
    expect(record.active_context.objectives).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ node_id: record.refs.select_export_rows }),
        expect.objectContaining({ node_id: record.refs.format_export_rows }),
      ]),
    );
    expect(record.active_context.objectives).not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ node_id: record.refs.bad_platform_rewrite }),
      ]),
    );
  });

  it("materializes mount change with cross-system impact analysis", async () => {
    const record = await materializeScenario("mount_change");
    const acceptedImpact = record.active_context.accepted_changes[0]?.impact_analysis;

    expect(acceptedImpact).toMatchObject({
      recommended_action: "plan_vertical_slice",
      risk: "high",
      affected_systems: expect.arrayContaining(["state", "input", "movement_rules", "save_data"]),
      affected_artifacts: expect.arrayContaining(["Player", "Mount", "Position", "SaveState"]),
    });
    expect(record.active_context.regression_constraints).toEqual(
      expect.arrayContaining([
        "Walking movement still updates Position when not mounted.",
        "Existing Position semantics do not change for unmounted players.",
      ]),
    );
  });

  it("reports scenario graph records needed before benchmark integration", async () => {
    const scenario = (await loadGoalGraphScenarioSuite("benchmarks")).find(
      (candidate) => candidate.id === "wrong_decomposition_retry",
    );
    if (!scenario) throw new Error("missing scenario");
    const root = await createTempDir("sophia-goal-scenario-");
    const store = new GraphStore(root);
    await materializeGoalGraphScenario({ store, scenario });

    const report = await buildGraphReport(store, await store.listNodes(), await store.listEdges());

    expect(report.goal_workflow.metrics).toMatchObject({
      invalidated_decompositions: 1,
      abandoned_branches: 1,
    });
    expect(report.goal_workflow.active_context.active_milestone).toMatchObject({
      name: "Export vertical slice",
    });
  });
});

async function materializeScenario(id: string) {
  const scenario = (await loadGoalGraphScenarioSuite("benchmarks")).find(
    (candidate) => candidate.id === id,
  );
  if (!scenario) throw new Error(`missing scenario ${id}`);
  const root = await createTempDir(`sophia-goal-scenario-${id}-`);
  return materializeGoalGraphScenario({
    store: new GraphStore(root),
    scenario,
  });
}
