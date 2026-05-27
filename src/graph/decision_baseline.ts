import { assessState } from "./decision_assessment.js";
import { buildCandidateActions } from "./decision_candidates.js";
import type { GraphDecision } from "./decision_types.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";

// Internal action-space baseline. It ranks valid executor actions from graph
// state so the LLM decision prompt can be constrained and reports can compare
// against a fixed reference. It is not a node selector; CLI node decisions must
// use the LLM decision path.
export async function buildDecisionActionBaseline(
  store: GraphStore,
  currentNode: GraphNode,
): Promise<GraphDecision> {
  const snapshot = {
    nodes: await store.listNodes(),
    edges: await store.listEdges(),
  };
  const assessment = await assessState(store, currentNode, snapshot);
  const candidateActions = await buildCandidateActions(store, currentNode, snapshot, assessment);
  const selectedAction = candidateActions[0]?.action ?? "complete";
  const confidence = candidateActions[0]?.score ?? 0.2;
  return {
    current_node: currentNode.id,
    state_assessment: assessment,
    candidate_actions: candidateActions,
    selected_action: selectedAction,
    confidence,
  };
}
