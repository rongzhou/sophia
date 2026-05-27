import { describe, expect, it } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import type { CheckResult } from "../../src/lang/diagnostics.js";
import { buildDecisionActionBaseline } from "../../src/graph/decision_baseline.js";
import type { GraphNode } from "../../src/graph/nodes.js";
import { GraphStore } from "../../src/graph/store.js";

describe("buildDecisionActionBaseline", () => {
  it("is an internal action-space baseline for LLM node-decision experiments", async () => {
    const store = await tempStore();
    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      action_used: "start",
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
      action_used: "start",
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
      action_used: "add_pseudo",
      summary: "Pseudo",
      artifacts: ["content.pseudo"],
    });
    await store.writeArtifact(
      pseudo,
      "content.pseudo",
      `program StaticLabel {
  purpose { "Return a fixed label." }
  inputs { none }
  outputs { result := "text label" }
  algorithm {
    return ready
  }
  expected { result := "ready" }
}
`,
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
      action_used: "add_pseudo",
      summary: "Pseudo",
      artifacts: ["content.pseudo"],
    });
    await store.writeArtifact(
      pseudo,
      "content.pseudo",
      `program StaticLabel {
  purpose { "Return a fixed label." }
  inputs { none }
  outputs { result := "text label" }
  algorithm {
    return ready
  }
}
`,
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
      action_used: "add_pseudo",
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
      action_used: "implement_design",
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
      action_used: "implement_design",
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
      action_used: "implement_design",
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
    action_used: edge,
    summary: result.ok ? "passed" : "failed",
    artifacts: ["result.json"],
  });
  await store.writeArtifact(node, "result.json", `${JSON.stringify(result, null, 2)}\n`);
  await store.appendEdge({ from: source.id, to: node.id, type: edge });
  return node;
}

async function repairNode(store: GraphStore, check: GraphNode): Promise<GraphNode> {
  const node = await store.createNode({
    type: "CodeNode",
    createdFrom: check.id,
    action_used: "repair_code",
    summary: "repaired",
  });
  await store.appendEdge({ from: check.id, to: node.id, type: "repairs" });
  return node;
}
