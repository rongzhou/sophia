import { describe, expect, it } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import type { CheckResult } from "../../src/lang/diagnostics.js";
import { applyDecisionNode } from "../../src/graph/apply_decision.js";
import type { DecisionAction, GraphDecision } from "../../src/graph/decision_types.js";
import type { GraphNode } from "../../src/graph/nodes.js";
import { GraphStore } from "../../src/graph/store.js";

describe("applyDecisionNode", () => {
  it("applies pseudo_check without generating pseudocode", async () => {
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
      `program Demo {
  purpose { "Return a label." }
  inputs { none }
  outputs { result := "label" }
  algorithm {
    return ready
  }
}
`,
    );
    const decision = await decisionNode(store, pseudo, "pseudo_check");

    const applied = await applyDecisionNode(store, decision);

    expect(applied.ok).toBe(true);
    expect(applied.created_node?.type).toBe("PseudocodeCheckNode");
    expect((await store.listEdges()).map((edge) => edge.type)).toEqual(
      expect.arrayContaining(["checks", "applies"]),
    );
  });

  it("applies check_code to a CodeNode artifact set", async () => {
    const store = await tempStore();
    const code = await codeNode(store);
    const decision = await decisionNode(store, code, "check_code");

    const applied = await applyDecisionNode(store, decision);

    expect(applied.ok).toBe(true);
    expect(applied.created_node?.type).toBe("CheckResultNode");
    expect(applied.result?.diagnostics).toEqual([]);
  });

  it("applies select only through deterministic gates", async () => {
    const store = await tempStore();
    const code = await codeNode(store);
    await resultNode(store, code, "CheckResultNode", "checks", { ok: true, diagnostics: [] });
    const audit = await resultNode(store, code, "AuditNode", "audits", {
      ok: true,
      diagnostics: [],
    });
    const decision = await decisionNode(store, audit, "select");

    const applied = await applyDecisionNode(store, decision);

    expect(applied.ok).toBe(true);
    expect(applied.created_node?.type).toBe("SelectionNode");
    expect(await store.readArtifact(applied.created_node!, "result.json")).toContain(code.id);
  });

  it("rejects actions that require explicit LLM or user input", async () => {
    const store = await tempStore();
    const pseudo = await store.createNode({
      type: "PseudocodeNode",
      createdFrom: null,
      action_used: "add_pseudo",
      summary: "Pseudo",
    });
    const decision = await decisionNode(store, pseudo, "implement_design");

    await expect(applyDecisionNode(store, decision)).rejects.toThrow(
      "requires explicit user or LLM input",
    );
  });
});

async function tempStore(): Promise<GraphStore> {
  return new GraphStore(await createTempDir("sophia-apply-"));
}

async function codeNode(store: GraphStore): Promise<GraphNode> {
  const code = await store.createNode({
    type: "CodeNode",
    createdFrom: null,
    action_used: "implement_design",
    summary: "Code",
    artifacts: [
      "files/domains/Demo/capabilities/PureCapability.sophia",
      "files/domains/Demo/actions/StaticLabel.sophia",
    ],
  });
  await store.writeArtifact(
    code,
    "files/domains/Demo/capabilities/PureCapability.sophia",
    `capability PureCapability {
  allow { }
}
`,
  );
  await store.writeArtifact(
    code,
    "files/domains/Demo/actions/StaticLabel.sophia",
    `action StaticLabel {
  capability: PureCapability
  output { result: Text }
  effects { }
  body {
    return "ready"
  }
}
`,
  );
  return code;
}

async function decisionNode(
  store: GraphStore,
  currentNode: GraphNode,
  selectedAction: DecisionAction,
): Promise<GraphNode> {
  const decision: GraphDecision = {
    current_node: currentNode.id,
    state_assessment: {
      goal_size: "tiny",
      logic_clarity: "medium",
      has_pseudocode: currentNode.type === "PseudocodeNode",
      has_code: currentNode.type === "CodeNode",
      compile_status: "not_checked",
      error_type: "none",
      repair_attempts: 0,
      decomposition_needed: false,
    },
    candidate_actions: [{ action: selectedAction, score: 0.8, reason: "test" }],
    selected_action: selectedAction,
    confidence: 0.8,
  };
  const node = await store.createNode({
    type: "DecisionNode",
    createdFrom: currentNode.id,
    action_used: "decide",
    summary: `Decision for ${currentNode.id}`,
    artifacts: ["result.json"],
  });
  await store.writeArtifact(node, "result.json", `${JSON.stringify(decision, null, 2)}\n`);
  await store.appendEdge({ from: currentNode.id, to: node.id, type: "decides" });
  return node;
}

async function resultNode(
  store: GraphStore,
  source: GraphNode,
  type: "CheckResultNode" | "AuditNode",
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
