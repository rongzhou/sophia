import type { CheckResult } from "../lang/diagnostics.js";
import type { CandidateAction, DecisionAction } from "./decision_types.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";

export function action(actionName: DecisionAction, score: number, reason: string): CandidateAction {
  return { action: actionName, score, reason };
}

export async function readCheckResult(store: GraphStore, node: GraphNode): Promise<CheckResult> {
  return store.readArtifactJson<CheckResult>(node, "result.json");
}

export async function readCreatedFromCodeNode(
  store: GraphStore,
  node: GraphNode,
): Promise<GraphNode | null> {
  if (!node.created_from) return null;
  const parent = await store.readNode(node.created_from);
  return parent.type === "CodeNode" ? parent : null;
}
