import type { CheckResult } from "../../lang/ast/diagnostics.js";
import {
  createAuditNode,
  createCheckResultNode,
  createSelectionNode,
  readCodeNodeFiles,
  summarizeResultNode,
} from "../workflow/code.js";
import { GraphDecisionSchema, type DecisionAction, type GraphDecisionPayload } from "./types.js";
import { assertNodeType, type GraphNode } from "../core/nodes.js";
import { createPseudocodeCheckNode } from "../workflow/pseudocode.js";
import type { GraphStore } from "../core/store.js";

export interface AppliedDecision {
  ok: boolean;
  decision_node: string;
  current_node: string;
  selected_action: DecisionAction;
  created_node: GraphNode | null;
  result?: CheckResult;
  message: string;
}

export async function applyDecisionNode(
  store: GraphStore,
  decisionNode: GraphNode,
): Promise<AppliedDecision> {
  assertNodeType(decisionNode, "DecisionNode");
  const decision = GraphDecisionSchema.parse(
    await store.readArtifactJson<unknown>(decisionNode, "result.json"),
  );
  if (decisionNode.created_from !== decision.current_node) {
    throw new Error(
      `DecisionNode ${decisionNode.id} does not match decision current_node ${decision.current_node}.`,
    );
  }
  const currentNode = await store.readNode(decision.current_node);
  switch (decision.selected_action) {
    case "pseudo_check":
      return applyPseudoCheck(store, decisionNode, currentNode, decision);
    case "check_code":
      return applyCheckCode(
        store,
        decisionNode,
        await requireCodeTarget(store, currentNode),
        decision,
      );
    case "audit_code":
      return applyAuditCode(
        store,
        decisionNode,
        await requireCodeTarget(store, currentNode),
        decision,
      );
    case "select":
      return applySelect(
        store,
        decisionNode,
        await requireCodeTarget(store, currentNode),
        decision,
      );
    case "complete":
      return {
        ok: true,
        decision_node: decisionNode.id,
        current_node: currentNode.id,
        selected_action: decision.selected_action,
        created_node: null,
        message: "Decision selected complete; no graph node was created.",
      };
    case "no_valid_action":
    case "requires_redecomposition":
    case "requires_human_scope_confirmation":
    case "budget_exhausted":
      return {
        ok: false,
        decision_node: decisionNode.id,
        current_node: currentNode.id,
        selected_action: decision.selected_action,
        created_node: null,
        message: `Decision selected ${decision.selected_action}; no graph node was created.`,
      };
    default:
      throw new Error(
        `Decision action ${decision.selected_action} requires explicit user or LLM input and cannot be applied deterministically.`,
      );
  }
}

async function applyPseudoCheck(
  store: GraphStore,
  decisionNode: GraphNode,
  currentNode: GraphNode,
  decision: GraphDecisionPayload,
): Promise<AppliedDecision> {
  assertNodeType(currentNode, "PseudocodeNode");
  const created = await createPseudocodeCheckNode({
    store,
    pseudoNode: currentNode,
    pseudocode: await store.readArtifact(currentNode, "content.pseudo"),
    tags: ["pseudo", "check", "apply"],
  });
  await store.appendEdge({ from: decisionNode.id, to: created.node.id, type: "applies" });
  return applied(decisionNode, currentNode, decision, created.node, created.result);
}

async function applyCheckCode(
  store: GraphStore,
  decisionNode: GraphNode,
  codeNode: GraphNode,
  decision: GraphDecisionPayload,
): Promise<AppliedDecision> {
  const created = await createCheckResultNode({
    store,
    codeNode,
    files: await readCodeNodeFiles(store, codeNode),
  });
  await store.appendEdge({ from: decisionNode.id, to: created.node.id, type: "applies" });
  return applied(decisionNode, codeNode, decision, created.node, created.result);
}

async function applyAuditCode(
  store: GraphStore,
  decisionNode: GraphNode,
  codeNode: GraphNode,
  decision: GraphDecisionPayload,
): Promise<AppliedDecision> {
  const created = await createAuditNode({
    store,
    codeNode,
    files: await readCodeNodeFiles(store, codeNode),
  });
  await store.appendEdge({ from: decisionNode.id, to: created.node.id, type: "applies" });
  return applied(decisionNode, codeNode, decision, created.node, created.result);
}

async function applySelect(
  store: GraphStore,
  decisionNode: GraphNode,
  codeNode: GraphNode,
  decision: GraphDecisionPayload,
): Promise<AppliedDecision> {
  const selectionNode = await createSelectionNode({ store, codeNode });
  await store.appendEdge({ from: decisionNode.id, to: selectionNode.id, type: "applies" });
  return {
    ok: true,
    decision_node: decisionNode.id,
    current_node: codeNode.id,
    selected_action: decision.selected_action,
    created_node: selectionNode,
    message: `Applied ${decision.selected_action} and created ${selectionNode.id}.`,
  };
}

async function requireCodeTarget(store: GraphStore, node: GraphNode): Promise<GraphNode> {
  if (node.type === "CodeNode") return node;
  if (
    (node.type === "CheckResultNode" ||
      node.type === "AuditNode" ||
      node.type === "ArtifactDiffNode") &&
    node.created_from
  ) {
    const parent = await store.readNode(node.created_from);
    if (parent.type === "CodeNode") return parent;
  }
  throw new Error(`Decision action requires CodeNode context, got ${node.type}.`);
}

function applied(
  decisionNode: GraphNode,
  currentNode: GraphNode,
  decision: GraphDecisionPayload,
  createdNode: GraphNode,
  result: CheckResult,
): AppliedDecision {
  return {
    ok: result.ok,
    decision_node: decisionNode.id,
    current_node: currentNode.id,
    selected_action: decision.selected_action,
    created_node: createdNode,
    result,
    message: `Applied ${decision.selected_action} and created ${createdNode.id}.`,
  };
}

export function summarizeAppliedDecision(
  appliedDecision: AppliedDecision,
): Record<string, unknown> {
  return {
    ok: appliedDecision.ok,
    decision_node: appliedDecision.decision_node,
    current_node: appliedDecision.current_node,
    selected_action: appliedDecision.selected_action,
    created_node: appliedDecision.created_node
      ? summarizeCreatedNode(appliedDecision.created_node, appliedDecision.result)
      : null,
    message: appliedDecision.message,
  };
}

function summarizeCreatedNode(
  node: GraphNode,
  result: CheckResult | undefined,
): Record<string, unknown> {
  if (!result) {
    return {
      id: node.id,
      type: node.type,
      status: node.status,
    };
  }
  return {
    type: node.type,
    ...summarizeResultNode(node, result),
  };
}
