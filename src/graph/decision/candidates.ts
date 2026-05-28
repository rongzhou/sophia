import {
  DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT,
  countDesignRevisionAttempts,
  countCodeRepairAttemptsForCodeNode,
} from "../workflow/llm_node.js";
import { latestChildFor, type GraphSnapshot } from "../core/snapshot.js";
import type { CandidateAction, StateAssessment } from "./types.js";
import type { GraphNode, NodeAction } from "../core/nodes.js";
import type { GraphStore } from "../core/store.js";
import { action, readCheckResult, readCreatedFromCodeNode } from "./helpers.js";
import {
  readChangeRequestPayload,
  readImpactAnalysisPayload,
  readMilestonePayload,
  readObjectivePayload,
} from "../goal/workflow.js";

const DEFAULT_DECOMPOSITION_ATTEMPT_LIMIT = 2;
const DEFAULT_REDECOMPOSITION_ATTEMPT_LIMIT = 2;
const DEFAULT_DESIGN_REVISION_ATTEMPT_LIMIT = 2;

export async function buildCandidateActions(
  store: GraphStore,
  currentNode: GraphNode,
  snapshot: GraphSnapshot,
  assessment: StateAssessment,
): Promise<CandidateAction[]> {
  const candidates: CandidateAction[] = [];
  switch (currentNode.type) {
    case "GoalNode":
      if (assessment.decomposition_needed) {
        candidates.push(
          action("decompose", 0.82, "Goal appears to span multiple independent semantic parts."),
        );
      }
      candidates.push(
        action("design_solution", 0.78, "Goal has no accepted structured pseudocode yet."),
      );
      break;
    case "ObjectiveNode":
      await appendObjectiveNodeCandidates(store, snapshot, currentNode, candidates);
      break;
    case "MilestoneNode":
      await appendMilestoneNodeCandidates(store, currentNode, candidates);
      break;
    case "ChangeRequestNode":
      await appendChangeRequestNodeCandidates(store, snapshot, currentNode, candidates);
      break;
    case "ImpactAnalysisNode":
      await appendImpactAnalysisNodeCandidates(store, currentNode, candidates);
      break;
    case "AcceptanceNode":
      candidates.push(action("complete", 0.74, "Human acceptance event has been recorded."));
      break;
    case "PseudocodeNode":
      await appendPseudocodeNodeCandidates(store, snapshot, currentNode, candidates);
      break;
    case "PseudocodeCheckNode":
      await appendPseudocodeCheckCandidates(store, currentNode, candidates);
      break;
    case "CodeNode":
      await appendCodeNodeCandidates(store, snapshot, currentNode, candidates);
      break;
    case "CheckResultNode":
      await appendCheckResultCandidates(store, currentNode, candidates);
      break;
    case "AuditNode":
      await appendAuditNodeCandidates(store, currentNode, candidates);
      break;
    case "SelectionNode":
      candidates.push(
        action("materialize_code", 0.9, "Selected candidate is ready for materialization."),
      );
      break;
    case "MaterializeNode":
      candidates.push(action("complete", 0.95, "Candidate has already been materialized."));
      break;
    case "RawLlmNode":
      candidates.push(
        action(
          "revise_design",
          0.62,
          "LLM call failed; return to semantic input or retry outside this node.",
        ),
      );
      break;
    case "ArtifactDiffNode":
      candidates.push(
        action(
          "audit_code",
          0.7,
          "Diff is recorded; audit determines whether constraints still hold.",
        ),
      );
      break;
  }
  if (candidates.length === 0) {
    candidates.push(action("no_valid_action", 0.2, "No valid transition exists for this node."));
  }
  return candidates.sort((left, right) => right.score - left.score);
}

async function appendObjectiveNodeCandidates(
  store: GraphStore,
  snapshot: GraphSnapshot,
  currentNode: GraphNode,
  candidates: CandidateAction[],
): Promise<void> {
  const payload = await readObjectivePayload(store, currentNode);
  if (payload.decomposition_status === "invalidated") {
    candidates.push(
      action(
        "requires_redecomposition",
        0.86,
        "Current objective belongs to an invalidated split.",
      ),
    );
    return;
  }
  if (payload.status === "abandoned" || payload.status === "superseded") {
    candidates.push(action("no_valid_action", 0.2, "Objective is not active."));
    return;
  }
  if (payload.authority === "proposed") {
    candidates.push(
      action(
        "accept_objective_decomposition",
        0.76,
        "AI-derived objective is proposed and needs explicit decomposition acceptance.",
      ),
    );
    candidates.push(
      action(
        "invalidate_decomposition",
        0.55,
        "Proposed objective can be rejected if the decomposition is wrong.",
      ),
    );
    return;
  }

  const decompositionAttempts = countDirectChildActions(
    snapshot,
    currentNode.id,
    "decompose_objective",
  );
  const redecompositionAttempts = countDirectChildActions(
    snapshot,
    currentNode.id,
    "redecompose_objective",
  );
  const hasAcceptedChildren = await hasAcceptedObjectiveChildren(store, snapshot, currentNode.id);
  const hasInvalidatedChildren = await hasInvalidatedObjectiveChildren(
    store,
    snapshot,
    currentNode.id,
  );
  if (!hasAcceptedChildren) {
    if (hasInvalidatedChildren) {
      if (redecompositionAttempts >= DEFAULT_REDECOMPOSITION_ATTEMPT_LIMIT) {
        candidates.push(
          action(
            "budget_exhausted",
            0.68,
            "Redecomposition budget is exhausted after invalidated objective splits.",
          ),
        );
      } else {
        candidates.push(
          action(
            "redecompose_objective",
            0.82,
            "Previous decomposition was invalidated; create a replacement split while preserving history.",
          ),
        );
      }
      return;
    }
    if (decompositionAttempts >= DEFAULT_DECOMPOSITION_ATTEMPT_LIMIT) {
      candidates.push(
        action(
          "requires_human_scope_confirmation",
          0.66,
          "Objective decomposition budget is exhausted without accepted children.",
        ),
      );
    } else {
      candidates.push(
        action("decompose_objective", 0.84, "Objective has no accepted child objectives yet."),
      );
    }
    return;
  }
  if (redecompositionAttempts >= DEFAULT_REDECOMPOSITION_ATTEMPT_LIMIT) {
    candidates.push(
      action("budget_exhausted", 0.6, "Redecomposition budget is exhausted for this objective."),
    );
    return;
  }
  candidates.push(
    action(
      "create_milestone",
      0.66,
      "Objective has accepted children; define a milestone boundary before implementation.",
    ),
  );
  candidates.push(
    action(
      "design_solution",
      0.58,
      "Objective can proceed to design if milestone scope is already clear.",
    ),
  );
}

async function appendMilestoneNodeCandidates(
  store: GraphStore,
  currentNode: GraphNode,
  candidates: CandidateAction[],
): Promise<void> {
  const payload = await readMilestonePayload(store, currentNode);
  if (payload.status === "rejected" || payload.status === "superseded") {
    candidates.push(action("no_valid_action", 0.2, "Milestone is not active."));
    return;
  }
  if (payload.decomposition_status === "invalidated") {
    candidates.push(
      action("requires_redecomposition", 0.78, "Milestone belongs to an invalidated split."),
    );
    return;
  }
  if (payload.authority === "proposed") {
    candidates.push(
      action("accept_milestone", 0.82, "Milestone is proposed and needs acceptance."),
    );
    return;
  }
  if (payload.status !== "active") {
    candidates.push(
      action("activate_milestone", 0.84, "Accepted milestone can become the active stage."),
    );
    return;
  }
  candidates.push(
    action("design_solution", 0.76, "Active milestone is ready for pseudocode design."),
  );
}

async function appendChangeRequestNodeCandidates(
  store: GraphStore,
  snapshot: GraphSnapshot,
  currentNode: GraphNode,
  candidates: CandidateAction[],
): Promise<void> {
  const payload = await readChangeRequestPayload(store, currentNode);
  if (payload.status === "deferred" || payload.status === "rejected") {
    candidates.push(action("complete", 0.7, "Change request is not entering active context."));
    return;
  }
  if (payload.status === "accepted") {
    candidates.push(
      action("decompose_objective", 0.62, "Accepted change can drive objective work."),
    );
    return;
  }
  const latestImpact = latestChildFor(snapshot, currentNode.id, "ImpactAnalysisNode");
  if (!latestImpact) {
    candidates.push(
      action("analyze_change_impact", 0.88, "Change request needs impact analysis first."),
    );
    return;
  }
  const impactPayload = await readImpactAnalysisPayload(store, latestImpact);
  if (impactPayload.status === "accepted") {
    candidates.push(
      action("accept_change_request", 0.84, "Accepted impact analysis can admit the change."),
    );
    return;
  }
  candidates.push(
    action(
      "requires_human_scope_confirmation",
      0.62,
      "Impact analysis exists but is not accepted; human scope decision is required.",
    ),
  );
}

async function appendImpactAnalysisNodeCandidates(
  store: GraphStore,
  currentNode: GraphNode,
  candidates: CandidateAction[],
): Promise<void> {
  const payload = await readImpactAnalysisPayload(store, currentNode);
  if (payload.status === "superseded") {
    candidates.push(action("no_valid_action", 0.2, "Impact analysis has been superseded."));
    return;
  }
  if (payload.status === "proposed") {
    candidates.push(
      action(
        "accept_change_request",
        0.78,
        "Impact analysis is proposed; accept the matching change before implementation.",
      ),
    );
    return;
  }
  switch (payload.recommended_action) {
    case "decompose_objective":
      candidates.push(
        action("decompose_objective", 0.8, "Impact analysis recommends decomposition."),
      );
      return;
    case "branch_design":
      candidates.push(
        action("design_solution", 0.66, "Impact analysis recommends a design branch."),
      );
      return;
    case "revise_design":
      candidates.push(action("revise_design", 0.66, "Impact analysis recommends design revision."));
      return;
    case "defer_change":
      candidates.push(action("complete", 0.62, "Impact analysis recommends deferring the change."));
      return;
    case "plan_vertical_slice":
    case "run_spike":
    case "reject_as_too_large":
      candidates.push(
        action(
          "requires_human_scope_confirmation",
          0.72,
          `Impact analysis recommends ${payload.recommended_action}, which needs scope confirmation.`,
        ),
      );
      return;
  }
}

async function appendPseudocodeNodeCandidates(
  store: GraphStore,
  snapshot: GraphSnapshot,
  currentNode: GraphNode,
  candidates: CandidateAction[],
): Promise<void> {
  const latestPseudoCheck = latestChildFor(snapshot, currentNode.id, "PseudocodeCheckNode");
  if (!latestPseudoCheck) {
    candidates.push(
      action(
        "pseudo_check",
        0.88,
        "Pseudocode must have a recorded deterministic check before implementation.",
      ),
    );
    return;
  }
  const result = await readCheckResult(store, latestPseudoCheck);
  candidates.push(
    result.ok
      ? action("implement_design", 0.86, "Recorded pseudocode check passed.")
      : action("revise_design", 0.66, "Pseudocode diagnostics indicate it needs revision."),
  );
}

async function appendPseudocodeCheckCandidates(
  store: GraphStore,
  currentNode: GraphNode,
  candidates: CandidateAction[],
): Promise<void> {
  const result = await readCheckResult(store, currentNode);
  if (result.ok) {
    candidates.push(action("implement_design", 0.88, "Checked pseudocode passed."));
    return;
  }
  const checkedPseudoNode =
    currentNode.created_from !== null ? await store.readNode(currentNode.created_from) : null;
  const revisionsUsed =
    checkedPseudoNode?.type === "PseudocodeNode"
      ? await countDesignRevisionAttempts(store, checkedPseudoNode)
      : 0;
  candidates.push(
    revisionsUsed >= DEFAULT_DESIGN_REVISION_ATTEMPT_LIMIT
      ? action(
          "budget_exhausted",
          0.78,
          "Checked pseudocode still fails after the design revision budget.",
        )
      : action("revise_design", 0.82, "Checked pseudocode has diagnostics."),
  );
}

async function appendCheckResultCandidates(
  store: GraphStore,
  currentNode: GraphNode,
  candidates: CandidateAction[],
): Promise<void> {
  const result = await readCheckResult(store, currentNode);
  if (result.ok) {
    candidates.push(
      action("audit_code", 0.86, "Code passed deterministic checker and needs constraint audit."),
    );
    return;
  }
  const checkedCodeNode = await readCreatedFromCodeNode(store, currentNode);
  const repairAvailable =
    checkedCodeNode &&
    (await countCodeRepairAttemptsForCodeNode(store, checkedCodeNode)) <
      DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT;
  candidates.push(
    repairAvailable
      ? action(
          "repair_code",
          0.82,
          "Checker failure is recorded and can be used as repair context.",
        )
      : action(
          "revise_design",
          0.74,
          "Checker failure exists but code repair budget is exhausted; revise the semantic source.",
        ),
  );
}

async function appendAuditNodeCandidates(
  store: GraphStore,
  currentNode: GraphNode,
  candidates: CandidateAction[],
): Promise<void> {
  const result = await readCheckResult(store, currentNode);
  if (result.ok) {
    candidates.push(action("select", 0.86, "Audit passed; candidate can be selected."));
    return;
  }
  const auditedCodeNode = await readCreatedFromCodeNode(store, currentNode);
  const repairAvailable =
    auditedCodeNode &&
    (await countCodeRepairAttemptsForCodeNode(store, auditedCodeNode)) <
      DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT;
  candidates.push(
    repairAvailable
      ? action("repair_code", 0.8, "Audit failure is recorded and can be used as repair context.")
      : action(
          "revise_design",
          0.72,
          "Audit failed and code repair budget is exhausted; revise the semantic source.",
        ),
  );
}

async function appendCodeNodeCandidates(
  store: GraphStore,
  snapshot: GraphSnapshot,
  currentNode: GraphNode,
  candidates: CandidateAction[],
): Promise<void> {
  const latestCheck = latestChildFor(snapshot, currentNode.id, "CheckResultNode");
  if (!latestCheck) {
    candidates.push(action("check_code", 0.88, "CodeNode has no deterministic check result."));
    return;
  }
  const checkResult = await readCheckResult(store, latestCheck);
  if (!checkResult.ok) {
    const repairAttempts = await countCodeRepairAttemptsForCodeNode(store, currentNode);
    candidates.push(
      repairAttempts < DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT
        ? action("repair_code", 0.82, "Latest deterministic check failed.")
        : action(
            "revise_design",
            0.74,
            "Latest deterministic check failed and code repair budget is exhausted.",
          ),
    );
    return;
  }
  const latestAudit = latestChildFor(snapshot, currentNode.id, "AuditNode");
  if (!latestAudit) {
    candidates.push(
      action("audit_code", 0.84, "Code passed check and still needs constraint audit."),
    );
    return;
  }
  const auditResult = await readCheckResult(store, latestAudit);
  if (auditResult.ok) {
    candidates.push(action("select", 0.88, "Code passed check and audit."));
    return;
  }
  const repairAttempts = await countCodeRepairAttemptsForCodeNode(store, currentNode);
  candidates.push(
    repairAttempts < DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT
      ? action("repair_code", 0.8, "Latest audit failed and can be used as repair context.")
      : action("revise_design", 0.72, "Latest audit failed and code repair budget is exhausted."),
  );
}

function countDirectChildActions(
  snapshot: GraphSnapshot,
  nodeId: string,
  actionUsed: NodeAction,
): number {
  const childIds = new Set(
    snapshot.edges.filter((edge) => edge.from === nodeId).map((edge) => edge.to),
  );
  return snapshot.nodes.filter((node) => childIds.has(node.id) && node.action_used === actionUsed)
    .length;
}

async function hasAcceptedObjectiveChildren(
  store: GraphStore,
  snapshot: GraphSnapshot,
  parentObjectiveId: string,
): Promise<boolean> {
  for (const node of snapshot.nodes.filter((candidate) => candidate.type === "ObjectiveNode")) {
    const payload = await readObjectivePayload(store, node);
    if (
      payload.parent_objective === parentObjectiveId &&
      payload.authority !== "proposed" &&
      payload.status !== "abandoned" &&
      payload.status !== "superseded" &&
      payload.decomposition_status !== "invalidated" &&
      payload.decomposition_status !== "superseded"
    ) {
      return true;
    }
  }
  return false;
}

async function hasInvalidatedObjectiveChildren(
  store: GraphStore,
  snapshot: GraphSnapshot,
  parentObjectiveId: string,
): Promise<boolean> {
  for (const node of snapshot.nodes.filter((candidate) => candidate.type === "ObjectiveNode")) {
    const payload = await readObjectivePayload(store, node);
    if (
      payload.parent_objective === parentObjectiveId &&
      (payload.status === "superseded" || payload.decomposition_status === "invalidated")
    ) {
      return true;
    }
  }
  return false;
}
