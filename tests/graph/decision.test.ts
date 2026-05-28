import { describe, expect, it } from "vitest";
import { samplePseudocodeJson } from "../helpers/sophia_workspace.js";
import { createTempDir } from "../helpers/sophia_workspace.js";
import type { CheckResult } from "../../src/lang/ast/diagnostics.js";
import { buildDecisionActionBaseline } from "../../src/graph/decision/baseline.js";
import {
  acceptChangeRequest,
  acceptObjectiveDecomposition,
  createChangeRequestNode,
  createImpactAnalysisNode,
  createObjectiveNode,
  decomposeObjective,
  invalidateDecomposition,
} from "../../src/graph/goal/workflow.js";
import type { GraphNode } from "../../src/graph/core/nodes.js";
import { GraphStore } from "../../src/graph/core/store.js";

describe("buildDecisionActionBaseline", () => {
  it("is an internal action-space baseline for LLM node-decision experiments", async () => {
    const store = await tempStore();
    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      actionUsed: "start",
      goal: "Compute and print a short sequence",
      summary: "Compute and print a short sequence",
    });

    const first = await buildDecisionActionBaseline(store, goal);
    const second = await buildDecisionActionBaseline(store, goal);

    expect(first).toEqual(second);
    expect(first.selected_action).toBe("design_solution");
  });

  it("suggests designing pseudocode for a new goal", async () => {
    const store = await tempStore();
    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      actionUsed: "start",
      goal: "Compute and print a short sequence",
      summary: "Compute and print a short sequence",
    });

    const decision = await buildDecisionActionBaseline(store, goal);

    expect(decision.selected_action).toBe("design_solution");
    expect(decision.state_assessment.has_pseudocode).toBe(false);
    expect(decision.state_assessment.compile_status).toBe("not_checked");
  });

  it("suggests recording a check before implementation explicit pseudocode", async () => {
    const store = await tempStore();
    const pseudo = await store.createNode({
      type: "PseudocodeNode",
      createdFrom: null,
      actionUsed: "add_pseudo",
      summary: "Pseudo",
      artifacts: ["content.pseudo"],
    });
    await store.writeArtifact(
      pseudo,
      "content.pseudo",
      samplePseudocodeJson({ expected: { result: "ready" } }),
    );

    const decision = await buildDecisionActionBaseline(store, pseudo);

    expect(decision.selected_action).toBe("pseudo_check");
    expect(decision.state_assessment.logic_clarity).toBe("high");
  });

  it("suggests implementation after explicit pseudocode has a passing check node", async () => {
    const store = await tempStore();
    const pseudo = await store.createNode({
      type: "PseudocodeNode",
      createdFrom: null,
      actionUsed: "add_pseudo",
      summary: "Pseudo",
      artifacts: ["content.pseudo"],
    });
    await store.writeArtifact(
      pseudo,
      "content.pseudo",
      samplePseudocodeJson(),
    );
    await resultNode(store, pseudo, "PseudocodeCheckNode", "checks", {
      ok: true,
      diagnostics: [],
    });

    const decision = await buildDecisionActionBaseline(store, pseudo);

    expect(decision.selected_action).toBe("implement_design");
  });

  it("classifies failed pseudocode checks as conceptual errors", async () => {
    const store = await tempStore();
    const pseudo = await store.createNode({
      type: "PseudocodeNode",
      createdFrom: null,
      actionUsed: "add_pseudo",
      summary: "Pseudo",
      artifacts: ["content.pseudo"],
    });
    const check = await resultNode(store, pseudo, "PseudocodeCheckNode", "checks", {
      ok: false,
      diagnostics: [
        {
          code: "PSEUDO-BRANCH-002",
          severity: "error",
          problem: "Independent inputs are incorrectly nested in an else chain.",
        },
      ],
    });

    const decision = await buildDecisionActionBaseline(store, check);

    expect(decision.selected_action).toBe("revise_design");
    expect(decision.state_assessment.error_type).toBe("conceptual");
  });

  it("suggests selection for code that passed check and audit", async () => {
    const store = await tempStore();
    const code = await store.createNode({
      type: "CodeNode",
      createdFrom: null,
      actionUsed: "implement_design",
      summary: "Code",
    });
    await resultNode(store, code, "CheckResultNode", "checks", { ok: true, diagnostics: [] });
    await resultNode(store, code, "AuditNode", "audits", { ok: true, diagnostics: [] });

    const decision = await buildDecisionActionBaseline(store, code);

    expect(decision.selected_action).toBe("select");
    expect(decision.state_assessment.compile_status).toBe("pass");
  });

  it("suggests code repair for audit failures while repair budget remains", async () => {
    const store = await tempStore();
    const code = await store.createNode({
      type: "CodeNode",
      createdFrom: null,
      actionUsed: "implement_design",
      summary: "Code",
    });
    await resultNode(store, code, "CheckResultNode", "checks", { ok: true, diagnostics: [] });
    const audit = await resultNode(store, code, "AuditNode", "audits", {
      ok: false,
      diagnostics: [
        {
          code: "AUDIT-HARDCODE-001",
          severity: "error",
          problem: "Generated .sophia appears to hardcode a full expected result list.",
        },
      ],
    });

    const codeDecision = await buildDecisionActionBaseline(store, code);
    const auditDecision = await buildDecisionActionBaseline(store, audit);

    expect(codeDecision.selected_action).toBe("repair_code");
    expect(auditDecision.selected_action).toBe("repair_code");
  });

  it("does not suggest code repair after the default repair budget is exhausted", async () => {
    const store = await tempStore();
    const code = await store.createNode({
      type: "CodeNode",
      createdFrom: null,
      actionUsed: "implement_design",
      summary: "Code",
    });
    const check = await resultNode(store, code, "CheckResultNode", "checks", {
      ok: false,
      diagnostics: [
        { code: "CHECK-BODY-004", severity: "error", problem: "Unsupported statement." },
      ],
    });
    await repairNode(store, check);
    await repairNode(store, check);

    const codeDecision = await buildDecisionActionBaseline(store, code);
    const checkDecision = await buildDecisionActionBaseline(store, check);

    expect(codeDecision.selected_action).toBe("revise_design");
    expect(checkDecision.selected_action).toBe("revise_design");
    expect(codeDecision.candidate_actions.map((candidate) => candidate.action)).not.toContain(
      "repair_code",
    );
    expect(checkDecision.candidate_actions.map((candidate) => candidate.action)).not.toContain(
      "repair_code",
    );
  });

  it("suggests decomposition for authoritative objectives without accepted children", async () => {
    const store = await tempStore();
    const objective = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Build todo workflow",
        description: "Create and list todo items with priority.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });

    const decision = await buildDecisionActionBaseline(store, objective);

    expect(decision.selected_action).toBe("decompose_objective");
    expect(decision.candidate_actions.map((candidate) => candidate.action)).toContain(
      "decompose_objective",
    );
  });

  it("requires acceptance for proposed AI objective decompositions", async () => {
    const store = await tempStore();
    const parent = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Parent",
        description: "Parent objective.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });
    const decomposition = await decomposeObjective({
      store,
      parentObjective: parent,
      decompositionId: "D0001",
      objectives: [
        {
          status: "open",
          title: "Child",
          description: "Child objective.",
          constraints: [],
          acceptance: [],
        },
      ],
    });

    const decision = await buildDecisionActionBaseline(store, decomposition.objective_nodes[0]!);

    expect(decision.selected_action).toBe("accept_objective_decomposition");
    expect(decision.candidate_actions.map((candidate) => candidate.action)).toContain(
      "invalidate_decomposition",
    );
  });

  it("requires redecomposition for invalidated objective decompositions", async () => {
    const store = await tempStore();
    const parent = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Parent",
        description: "Parent objective.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });
    const decomposition = await decomposeObjective({
      store,
      parentObjective: parent,
      decompositionId: "D0001",
      objectives: [
        {
          status: "open",
          title: "Wrong child",
          description: "Wrong child objective.",
          constraints: [],
          acceptance: [],
        },
      ],
    });
    await acceptObjectiveDecomposition({
      store,
      parentObjective: parent,
      decompositionId: "D0001",
    });
    await invalidateDecomposition({
      store,
      parentObjective: parent,
      decompositionId: "D0001",
      reason: "Wrong split.",
    });

    const decision = await buildDecisionActionBaseline(store, decomposition.objective_nodes[0]!);
    const parentDecision = await buildDecisionActionBaseline(store, parent);

    expect(decision.selected_action).toBe("requires_redecomposition");
    expect(parentDecision.selected_action).toBe("redecompose_objective");
  });

  it("stops repeated pseudocode revision loops with a budget exhausted decision", async () => {
    const store = await tempStore();
    const original = await store.createNode({
      type: "PseudocodeNode",
      createdFrom: null,
      actionUsed: "design_solution",
      summary: "Original pseudo",
      artifacts: ["content.pseudo"],
    });
    const firstCheck = await resultNode(store, original, "PseudocodeCheckNode", "checks", {
      ok: false,
      diagnostics: [
        {
          code: "missing-output",
          severity: "error",
          problem: "Missing output.",
        },
      ],
    });
    const firstRevision = await revisedPseudoNode(store, firstCheck, "First revision");
    const secondCheck = await resultNode(store, firstRevision, "PseudocodeCheckNode", "checks", {
      ok: false,
      diagnostics: [
        {
          code: "missing-output",
          severity: "error",
          problem: "Still missing output.",
        },
      ],
    });
    const secondRevision = await revisedPseudoNode(store, secondCheck, "Second revision");
    const thirdCheck = await resultNode(store, secondRevision, "PseudocodeCheckNode", "checks", {
      ok: false,
      diagnostics: [
        {
          code: "still-invalid",
          severity: "error",
          problem: "Still invalid after two revisions.",
        },
      ],
    });

    const decision = await buildDecisionActionBaseline(store, thirdCheck);

    expect(decision.selected_action).toBe("budget_exhausted");
  });

  it("suggests impact analysis before accepting change requests", async () => {
    const store = await tempStore();
    const objective = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Mount",
        description: "Model movement.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });
    const change = await createChangeRequestNode({
      store,
      createdFrom: objective,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "proposed",
        kind: "new_requirement",
        request: "Player can mount.",
        applies_to: ["player"],
        priority: "must",
      },
    });

    const decision = await buildDecisionActionBaseline(store, change);

    expect(decision.selected_action).toBe("analyze_change_impact");
  });

  it("suggests accepting change requests once matching impact analysis is accepted", async () => {
    const store = await tempStore();
    const objective = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Mount",
        description: "Model movement.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });
    const change = await createChangeRequestNode({
      store,
      createdFrom: objective,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "proposed",
        kind: "new_requirement",
        request: "Player can mount.",
        applies_to: ["player"],
        priority: "must",
      },
    });
    const impact = await createImpactAnalysisNode({
      store,
      createdFrom: change,
      payload: {
        origin: "ai",
        authority: "proposed",
        status: "proposed",
        change_request: change.id,
        affected_objectives: [objective.id],
        affected_milestones: [],
        affected_artifacts: ["Player"],
        preserved_constraints: [],
        possibly_invalidated_acceptance: [],
        recommended_action: "decompose_objective",
        risk: "medium",
        affected_systems: ["movement"],
        unknowns: [],
        regression_constraints: [],
      },
    });
    await acceptChangeRequest({ store, changeRequestNode: change, impactAnalysisNode: impact });

    const proposedChange = await createChangeRequestNode({
      store,
      createdFrom: objective,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "proposed",
        kind: "new_requirement",
        request: "Player can dismount.",
        applies_to: ["player"],
        priority: "must",
      },
    });
    const proposedImpact = await createImpactAnalysisNode({
      store,
      createdFrom: proposedChange,
      payload: {
        origin: "ai",
        authority: "proposed",
        status: "accepted",
        change_request: proposedChange.id,
        affected_objectives: [objective.id],
        affected_milestones: [],
        affected_artifacts: ["Player"],
        preserved_constraints: [],
        possibly_invalidated_acceptance: [],
        recommended_action: "decompose_objective",
        risk: "medium",
        affected_systems: ["movement"],
        unknowns: [],
        regression_constraints: [],
      },
    });

    const decision = await buildDecisionActionBaseline(store, proposedChange);
    const impactDecision = await buildDecisionActionBaseline(store, proposedImpact);

    expect(decision.selected_action).toBe("accept_change_request");
    expect(impactDecision.selected_action).toBe("decompose_objective");
  });
});

async function tempStore(): Promise<GraphStore> {
  return new GraphStore(await createTempDir("sophia-decision-"));
}

async function resultNode(
  store: GraphStore,
  source: GraphNode,
  type: "PseudocodeCheckNode" | "CheckResultNode" | "AuditNode",
  edge: "checks" | "audits",
  result: CheckResult,
): Promise<GraphNode> {
  const node = await store.createNode({
    type,
    status: result.ok ? "active" : "failed",
    createdFrom: source.id,
    actionUsed:
      type === "PseudocodeCheckNode"
        ? "pseudo_check"
        : edge === "checks"
          ? "check_code"
          : "constraint_audit",
    summary: result.ok ? "passed" : "failed",
    artifacts: ["result.json"],
  });
  await store.writeArtifactJson(node, "result.json", result);
  await store.appendEdge({ from: source.id, to: node.id, type: edge });
  return node;
}

async function repairNode(store: GraphStore, check: GraphNode): Promise<GraphNode> {
  const node = await store.createNode({
    type: "CodeNode",
    createdFrom: check.id,
    actionUsed: "repair_code",
    summary: "repaired",
  });
  await store.appendEdge({ from: check.id, to: node.id, type: "repairs" });
  return node;
}

async function revisedPseudoNode(
  store: GraphStore,
  check: GraphNode,
  summary: string,
): Promise<GraphNode> {
  const node = await store.createNode({
    type: "PseudocodeNode",
    createdFrom: check.id,
    actionUsed: "revise_design",
    summary,
    artifacts: ["content.pseudo"],
  });
  await store.appendEdge({ from: check.id, to: node.id, type: "revises" });
  return node;
}
