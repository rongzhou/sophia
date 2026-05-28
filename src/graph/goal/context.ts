import type { GraphNode } from "../core/nodes.js";
import type { GraphStore } from "../core/store.js";
import {
  readChangeRequestPayload,
  readImpactAnalysisPayload,
  readMilestonePayload,
  readObjectivePayload,
  type ChangeRequestPayload,
  type ImpactAnalysisPayload,
  type MilestonePayload,
  type ObjectivePayload,
} from "./workflow.js";

export interface GoalContextObjective {
  node_id: string;
  title: string;
  description: string;
  origin: ObjectivePayload["origin"];
  authority: ObjectivePayload["authority"];
  status: ObjectivePayload["status"];
  constraints: string[];
  acceptance: string[];
  parent_objective: string | null;
  decomposition_id: string | null;
}

export interface GoalContextMilestone {
  node_id: string;
  name: string;
  origin: MilestonePayload["origin"];
  authority: MilestonePayload["authority"];
  status: MilestonePayload["status"];
  scope: string[];
  out_of_scope: string[];
  acceptance: string[];
  parent_objective: string | null;
  decomposition_id: string | null;
}

export interface GoalContextChange {
  node_id: string;
  kind: ChangeRequestPayload["kind"];
  request: string;
  priority: ChangeRequestPayload["priority"];
  applies_to: string[];
  impact_analysis: {
    node_id: string;
    affected_objectives: string[];
    affected_milestones: string[];
    affected_artifacts: string[];
    preserved_constraints: string[];
    recommended_action: ImpactAnalysisPayload["recommended_action"];
    risk: ImpactAnalysisPayload["risk"];
    blast_radius?: NonNullable<ImpactAnalysisPayload["blast_radius"]>;
    affected_systems: string[];
    unknowns: string[];
    recommended_strategy?: NonNullable<ImpactAnalysisPayload["recommended_strategy"]>;
    first_slice?: NonNullable<ImpactAnalysisPayload["first_slice"]>;
    regression_constraints: string[];
  } | null;
}

export interface GoalContext {
  objectives: GoalContextObjective[];
  active_milestone: GoalContextMilestone | null;
  accepted_changes: GoalContextChange[];
  out_of_scope: string[];
  regression_constraints: string[];
  excluded: {
    objectives: string[];
    milestones: string[];
    change_requests: string[];
    impact_analyses: string[];
  };
}

export async function buildGoalContext(store: GraphStore): Promise<GoalContext> {
  const nodes = await store.listNodes();
  const objectives: GoalContextObjective[] = [];
  const excludedObjectives: string[] = [];
  for (const node of nodes.filter((candidate) => candidate.type === "ObjectiveNode")) {
    const payload = await readObjectivePayload(store, node);
    if (isActiveObjectivePayload(payload, node)) {
      objectives.push(toContextObjective(node, payload));
    } else {
      excludedObjectives.push(node.id);
    }
  }

  const acceptedMilestones: GoalContextMilestone[] = [];
  const activeMilestones: GoalContextMilestone[] = [];
  const excludedMilestones: string[] = [];
  for (const node of nodes.filter((candidate) => candidate.type === "MilestoneNode")) {
    const payload = await readMilestonePayload(store, node);
    if (isActiveMilestonePayload(payload, node)) {
      const contextMilestone = toContextMilestone(node, payload);
      if (payload.status === "active") {
        activeMilestones.push(contextMilestone);
      } else {
        acceptedMilestones.push(contextMilestone);
      }
    } else {
      excludedMilestones.push(node.id);
    }
  }
  const activeMilestone = latestByNodeId(activeMilestones) ?? latestByNodeId(acceptedMilestones);

  const acceptedChanges: GoalContextChange[] = [];
  const excludedChangeRequests: string[] = [];
  const acceptedImpactByChange = await acceptedImpactAnalysesByChange(store, nodes);
  for (const node of nodes.filter((candidate) => candidate.type === "ChangeRequestNode")) {
    const payload = await readChangeRequestPayload(store, node);
    if (payload.status === "accepted") {
      acceptedChanges.push({
        node_id: node.id,
        kind: payload.kind,
        request: payload.request,
        priority: payload.priority,
        applies_to: payload.applies_to,
        impact_analysis: acceptedImpactByChange.get(node.id) ?? null,
      });
    } else {
      excludedChangeRequests.push(node.id);
    }
  }

  const acceptedImpactNodeIds = new Set(
    acceptedChanges
      .map((change) => change.impact_analysis?.node_id)
      .filter((id): id is string => Boolean(id)),
  );
  const excludedImpactAnalyses = nodes
    .filter((node) => node.type === "ImpactAnalysisNode" && !acceptedImpactNodeIds.has(node.id))
    .map((node) => node.id);

  return {
    objectives: objectives.sort(compareNodeId),
    active_milestone: activeMilestone,
    accepted_changes: acceptedChanges.sort(compareNodeId),
    out_of_scope: uniqueStrings([
      ...(activeMilestone?.out_of_scope ?? []),
      ...acceptedChanges.flatMap(
        (change) => change.impact_analysis?.first_slice?.out_of_scope ?? [],
      ),
    ]),
    regression_constraints: uniqueStrings(
      acceptedChanges.flatMap((change) => change.impact_analysis?.regression_constraints ?? []),
    ),
    excluded: {
      objectives: excludedObjectives.sort(),
      milestones: excludedMilestones.sort(),
      change_requests: excludedChangeRequests.sort(),
      impact_analyses: excludedImpactAnalyses.sort(),
    },
  };
}

function isActiveObjectivePayload(payload: ObjectivePayload, node: GraphNode): boolean {
  return (
    node.status !== "abandoned" &&
    node.status !== "superseded" &&
    payload.status !== "abandoned" &&
    payload.status !== "superseded" &&
    payload.authority !== "proposed" &&
    payload.decomposition_status !== "invalidated" &&
    payload.decomposition_status !== "superseded"
  );
}

function isActiveMilestonePayload(payload: MilestonePayload, node: GraphNode): boolean {
  return (
    node.status !== "abandoned" &&
    node.status !== "superseded" &&
    payload.status !== "rejected" &&
    payload.status !== "superseded" &&
    payload.authority !== "proposed" &&
    payload.decomposition_status !== "invalidated" &&
    payload.decomposition_status !== "superseded"
  );
}

async function acceptedImpactAnalysesByChange(
  store: GraphStore,
  nodes: GraphNode[],
): Promise<Map<string, GoalContextChange["impact_analysis"]>> {
  const accepted = new Map<string, Array<{ node: GraphNode; payload: ImpactAnalysisPayload }>>();
  for (const node of nodes.filter((candidate) => candidate.type === "ImpactAnalysisNode")) {
    const payload = await readImpactAnalysisPayload(store, node);
    if (payload.status !== "accepted") continue;
    const candidates = accepted.get(payload.change_request) ?? [];
    candidates.push({ node, payload });
    accepted.set(payload.change_request, candidates);
  }
  const result = new Map<string, GoalContextChange["impact_analysis"]>();
  for (const [changeRequestId, analyses] of accepted.entries()) {
    const latest = analyses.sort((left, right) => right.node.id.localeCompare(left.node.id))[0];
    if (!latest) continue;
    result.set(changeRequestId, toContextImpactAnalysis(latest.node, latest.payload));
  }
  return result;
}

function toContextObjective(node: GraphNode, payload: ObjectivePayload): GoalContextObjective {
  return {
    node_id: node.id,
    title: payload.title,
    description: payload.description,
    origin: payload.origin,
    authority: payload.authority,
    status: payload.status,
    constraints: payload.constraints,
    acceptance: payload.acceptance,
    parent_objective: payload.parent_objective,
    decomposition_id: payload.decomposition_id,
  };
}

function toContextMilestone(node: GraphNode, payload: MilestonePayload): GoalContextMilestone {
  return {
    node_id: node.id,
    name: payload.name,
    origin: payload.origin,
    authority: payload.authority,
    status: payload.status,
    scope: payload.scope,
    out_of_scope: payload.out_of_scope,
    acceptance: payload.acceptance,
    parent_objective: payload.parent_objective,
    decomposition_id: payload.decomposition_id,
  };
}

function toContextImpactAnalysis(
  node: GraphNode,
  payload: ImpactAnalysisPayload,
): NonNullable<GoalContextChange["impact_analysis"]> {
  return {
    node_id: node.id,
    affected_objectives: payload.affected_objectives,
    affected_milestones: payload.affected_milestones,
    affected_artifacts: payload.affected_artifacts,
    preserved_constraints: payload.preserved_constraints,
    recommended_action: payload.recommended_action,
    risk: payload.risk,
    ...(payload.blast_radius ? { blast_radius: payload.blast_radius } : {}),
    affected_systems: payload.affected_systems,
    unknowns: payload.unknowns,
    ...(payload.recommended_strategy ? { recommended_strategy: payload.recommended_strategy } : {}),
    ...(payload.first_slice ? { first_slice: payload.first_slice } : {}),
    regression_constraints: payload.regression_constraints,
  };
}

function latestByNodeId<T extends { node_id: string }>(items: T[]): T | null {
  return [...items].sort((left, right) => right.node_id.localeCompare(left.node_id))[0] ?? null;
}

function compareNodeId(left: { node_id: string }, right: { node_id: string }): number {
  return left.node_id.localeCompare(right.node_id);
}

function uniqueStrings(values: string[]): string[] {
  return [...new Set(values)].sort();
}
