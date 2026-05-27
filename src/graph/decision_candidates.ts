import {
  DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT,
  countCodeRepairAttemptsForCodeNode,
} from "./llm_node_workflow.js";
import { latestChildFor, type GraphSnapshot } from "./graph_snapshot.js";
import type { CandidateAction, StateAssessment } from "./decision_types.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";
import { action, readCheckResult, readCreatedFromCodeNode } from "./decision_helpers.js";

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
  return candidates.sort((left, right) => right.score - left.score);
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
  candidates.push(
    result.ok
      ? action("implement_design", 0.88, "Checked pseudocode passed.")
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
