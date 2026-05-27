import { checkPseudocode } from "../pseudo/check.js";
import type { StateAssessment } from "./decision_types.js";
import {
  countDescendantActions,
  hasRelatedNode,
  latestChildFor,
  type GraphSnapshot,
} from "./graph_snapshot.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";
import { readCheckResult } from "./decision_helpers.js";

export async function assessState(
  store: GraphStore,
  currentNode: GraphNode,
  snapshot: GraphSnapshot,
): Promise<StateAssessment> {
  const goalText = currentNode.goal ?? currentNode.summary;
  const latestCheck = latestChildFor(snapshot, currentNode.id, "CheckResultNode");
  const diagnosticNode = diagnosticNodeForAssessment(snapshot, currentNode);
  const compileStatus = await compileStatusFor(store, latestCheck);
  return {
    goal_size: classifyGoalSize(goalText),
    logic_clarity: await classifyLogicClarity(store, currentNode),
    has_pseudocode: hasRelatedNode(snapshot, currentNode.id, "PseudocodeNode"),
    has_code: hasRelatedNode(snapshot, currentNode.id, "CodeNode"),
    compile_status: compileStatus,
    error_type: await classifyErrorType(store, diagnosticNode),
    repair_attempts: countRepairAttempts(snapshot, currentNode.id),
    decomposition_needed: goalNeedsDecomposition(goalText),
  };
}

function diagnosticNodeForAssessment(
  snapshot: GraphSnapshot,
  currentNode: GraphNode,
): GraphNode | null {
  if (
    currentNode.type === "PseudocodeCheckNode" ||
    currentNode.type === "CheckResultNode" ||
    currentNode.type === "AuditNode"
  ) {
    return currentNode;
  }
  return (
    latestChildFor(snapshot, currentNode.id, "PseudocodeCheckNode") ??
    latestChildFor(snapshot, currentNode.id, "CheckResultNode") ??
    latestChildFor(snapshot, currentNode.id, "AuditNode")
  );
}

async function compileStatusFor(
  store: GraphStore,
  checkNode: GraphNode | null,
): Promise<StateAssessment["compile_status"]> {
  if (!checkNode) return "not_checked";
  const result = await readCheckResult(store, checkNode);
  return result.ok ? "pass" : "fail";
}

async function classifyErrorType(
  store: GraphStore,
  checkNode: GraphNode | null,
): Promise<StateAssessment["error_type"]> {
  if (!checkNode) return "none";
  const result = await readCheckResult(store, checkNode);
  if (result.ok) return "none";
  if (result.diagnostics.some((diagnostic) => diagnostic.code.startsWith("PSEUDO-"))) {
    return "conceptual";
  }
  return "local";
}

async function classifyLogicClarity(
  store: GraphStore,
  currentNode: GraphNode,
): Promise<StateAssessment["logic_clarity"]> {
  if (currentNode.type !== "PseudocodeNode") {
    return currentNode.type === "GoalNode" ? "low" : "medium";
  }
  const result = checkPseudocode(await store.readArtifact(currentNode, "content.pseudo"));
  if (result.ok) return "high";
  return result.diagnostics.some((diagnostic) => diagnostic.severity === "error")
    ? "low"
    : "medium";
}

function classifyGoalSize(goal: string): StateAssessment["goal_size"] {
  const words = goal.split(/\s+/).filter(Boolean).length;
  if (words <= 6) return "tiny";
  if (words <= 18) return "small";
  if (words <= 40) return "medium";
  return "large";
}

function goalNeedsDecomposition(goal: string): boolean {
  const implemented = goal.toLowerCase();
  return /\b(and then|workflow|multiple|several|crud|storage|database|authentication|integration)\b/.test(
    implemented,
  );
}

function countRepairAttempts(snapshot: GraphSnapshot, nodeId: string): number {
  return countDescendantActions(snapshot, nodeId, "repair_code");
}
