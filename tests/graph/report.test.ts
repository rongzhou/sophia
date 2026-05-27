import { describe, expect, it } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import type { CheckResult } from "../../src/lang/diagnostics.js";
import { buildGraphReport } from "../../src/graph/report.js";
import { GraphStore } from "../../src/graph/store.js";

describe("buildGraphReport", () => {
  it("summarizes experiments by pseudocode node without mutating code nodes", async () => {
    const root = await createTempDir("sophia-report-");
    const store = new GraphStore(root);

    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      action_used: "start",
      goal: "Build a tiny list",
      summary: "Build a tiny list",
    });
    const pseudo = await store.createNode({
      type: "PseudocodeNode",
      createdFrom: goal.id,
      action_used: "add_pseudo",
      goal: goal.goal,
      summary: "Pseudocode from fixtures/list/build_three_numbers.pseudo.",
    });
    await store.appendEdge({ from: goal.id, to: pseudo.id, type: "designs_solution" });

    const implemented = await store.createNode({
      type: "CodeNode",
      createdFrom: pseudo.id,
      action_used: "implement_design",
      goal: goal.goal,
      summary: "Implemented",
    });
    await store.appendEdge({ from: pseudo.id, to: implemented.id, type: "implements_design" });
    const failedCheck = await resultNode(store, {
      type: "CheckResultNode",
      createdFrom: implemented.id,
      edge: "checks",
      status: "failed",
      result: {
        ok: false,
        diagnostics: [{ code: "CHECK-BODY-003", severity: "error", problem: "bad append" }],
      },
    });

    const repaired = await store.createNode({
      type: "CodeNode",
      createdFrom: failedCheck.id,
      action_used: "repair_code",
      goal: goal.goal,
      summary: "Repaired",
    });
    await store.appendEdge({ from: failedCheck.id, to: repaired.id, type: "repairs" });
    await resultNode(store, {
      type: "CheckResultNode",
      createdFrom: repaired.id,
      edge: "checks",
      result: { ok: true, diagnostics: [] },
    });
    await resultNode(store, {
      type: "AuditNode",
      createdFrom: repaired.id,
      edge: "audits",
      result: { ok: true, diagnostics: [] },
    });
    await resultNode(store, {
      type: "ArtifactDiffNode",
      createdFrom: repaired.id,
      edge: "diffs",
      result: { ok: true, diagnostics: [] },
    });
    const selection = await store.createNode({
      type: "SelectionNode",
      createdFrom: repaired.id,
      action_used: "select_code",
      summary: "Selected",
    });
    await store.appendEdge({ from: repaired.id, to: selection.id, type: "selects" });
    const decision = await store.createNode({
      type: "DecisionNode",
      createdFrom: repaired.id,
      action_used: "llm_decide",
      summary: "Selected by LLM decision",
      artifacts: ["result.json"],
    });
    await store.writeArtifact(
      decision,
      "result.json",
      `${JSON.stringify(
        {
          current_node: repaired.id,
          state_assessment: {
            goal_size: "small",
            logic_clarity: "high",
            has_pseudocode: true,
            has_code: true,
            compile_status: "pass",
            error_type: "none",
            repair_attempts: 1,
            decomposition_needed: false,
          },
          candidate_actions: [{ action: "select", score: 0.9, reason: "passed" }],
          selected_action: "select",
          confidence: 0.9,
        },
        null,
        2,
      )}\n`,
    );
    await store.appendEdge({ from: decision.id, to: selection.id, type: "applies" });

    const report = await buildGraphReport(store, await store.listNodes(), await store.listEdges());

    expect(report.metrics.code_nodes_total).toBe(2);
    expect(report.metrics.llm_decision_nodes).toBe(1);
    expect(report.metrics.repaired_code_nodes).toBe(1);
    expect(report.metrics.selected_code_nodes).toBe(1);
    expect(report.metrics.latest_check_passed).toBe(1);
    expect(report.metrics.latest_check_failed).toBe(1);
    expect(report.diagnostic_counts).toEqual({ "CHECK-BODY-003": 1 });
    expect(report.experiments).toHaveLength(1);
    expect(report.experiments[0]).toMatchObject({
      pseudocode_node: pseudo.id,
      fixture: "fixtures/list/build_three_numbers.pseudo",
      implementation_attempts: 1,
      implementation_passed: 0,
      implementation_success_rate: 0,
      repair_attempts: 1,
      repaired_passed: 1,
      materialize_ready: 1,
      checker_error_types: { "CHECK-BODY-003": 1 },
      latest_code_node: repaired.id,
      latest_check_ok: true,
      latest_audit_ok: true,
    });
    await expect(store.readNode(repaired.id)).resolves.toMatchObject({
      status: "active",
    });
  });

  it("attributes implemented code to the nearest revised pseudocode node", async () => {
    const root = await createTempDir("sophia-report-");
    const store = new GraphStore(root);

    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      action_used: "start",
      goal: "Classify a count",
      summary: "Classify a count",
    });
    const originalPseudo = await store.createNode({
      type: "PseudocodeNode",
      createdFrom: goal.id,
      action_used: "design_solution",
      goal: goal.goal,
      summary: "Original pseudocode",
    });
    await store.appendEdge({ from: goal.id, to: originalPseudo.id, type: "designs_solution" });
    const failedPseudoCheck = await resultNode(store, {
      type: "PseudocodeCheckNode",
      createdFrom: originalPseudo.id,
      edge: "checks",
      status: "failed",
      result: {
        ok: false,
        diagnostics: [{ code: "PSEUDO-BRANCH-001", severity: "error", problem: "bad branch" }],
      },
    });
    const revisedPseudo = await store.createNode({
      type: "PseudocodeNode",
      createdFrom: failedPseudoCheck.id,
      action_used: "revise_design",
      goal: goal.goal,
      summary: "Revised pseudocode",
    });
    await store.appendEdge({ from: failedPseudoCheck.id, to: revisedPseudo.id, type: "revises" });

    const implemented = await store.createNode({
      type: "CodeNode",
      createdFrom: revisedPseudo.id,
      action_used: "implement_design",
      goal: goal.goal,
      summary: "Implemented",
    });
    await store.appendEdge({
      from: revisedPseudo.id,
      to: implemented.id,
      type: "implements_design",
    });
    await resultNode(store, {
      type: "CheckResultNode",
      createdFrom: implemented.id,
      edge: "checks",
      result: { ok: true, diagnostics: [] },
    });
    await resultNode(store, {
      type: "AuditNode",
      createdFrom: implemented.id,
      edge: "audits",
      result: { ok: true, diagnostics: [] },
    });

    const report = await buildGraphReport(store, await store.listNodes(), await store.listEdges());
    const byPseudo = new Map(
      report.experiments.map((experiment) => [experiment.pseudocode_node, experiment]),
    );

    expect(byPseudo.get(originalPseudo.id)).toMatchObject({
      pseudocode_checks: 1,
      latest_pseudocode_check_ok: false,
      pseudocode_diagnostic_types: { "PSEUDO-BRANCH-001": 1 },
      implementation_attempts: 0,
      latest_code_node: null,
    });
    expect(byPseudo.get(revisedPseudo.id)).toMatchObject({
      implementation_attempts: 1,
      implementation_passed: 1,
      latest_code_node: implemented.id,
    });
  });
});

async function resultNode(
  store: GraphStore,
  options: {
    type: "PseudocodeCheckNode" | "CheckResultNode" | "AuditNode" | "ArtifactDiffNode";
    createdFrom: string;
    edge: "checks" | "audits" | "diffs";
    status?: "active" | "failed";
    result: CheckResult;
  },
) {
  const node = await store.createNode({
    type: options.type,
    status: options.status ?? (options.result.ok ? "active" : "failed"),
    createdFrom: options.createdFrom,
    action_used: options.edge,
    summary: options.result.ok ? "passed" : "failed",
    artifacts: ["result.json"],
  });
  await store.writeArtifact(node, "result.json", `${JSON.stringify(options.result, null, 2)}\n`);
  await store.appendEdge({ from: options.createdFrom, to: node.id, type: options.edge });
  return node;
}
