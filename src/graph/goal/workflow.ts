import { z } from "zod";
import { assertNodeType, NodeIdSchema, type GraphNode, type NodeAction } from "../core/nodes.js";
import type { GraphStore } from "../core/store.js";
import type { GraphEdgeType } from "../core/nodes.js";

export const GoalNodeOriginSchema = z.enum(["human", "ai"]);
export const GoalNodeAuthoritySchema = z.enum(["authoritative", "proposed", "derived"]);

export type GoalNodeOrigin = z.infer<typeof GoalNodeOriginSchema>;
export type GoalNodeAuthority = z.infer<typeof GoalNodeAuthoritySchema>;

const CommonGoalPayloadSchema = z.object({
  origin: GoalNodeOriginSchema,
  authority: GoalNodeAuthoritySchema,
  status: z.string().min(1),
});

const DecompositionStatusSchema = z.enum(["proposed", "accepted", "invalidated", "superseded"]);

export const ObjectivePayloadSchema = CommonGoalPayloadSchema.extend({
  title: z.string().min(1),
  description: z.string().min(1),
  constraints: z.array(z.string()).default([]),
  acceptance: z.array(z.string()).default([]),
  parent_objective: NodeIdSchema.nullable().default(null),
  decomposition_id: z.string().min(1).nullable().default(null),
  decomposition_status: DecompositionStatusSchema.nullable().default(null),
  status: z.enum(["open", "active", "satisfied", "superseded", "abandoned"]),
}).superRefine((payload, ctx) => {
  rejectAiAuthoritative(payload, ctx);
});

export type ObjectivePayload = z.infer<typeof ObjectivePayloadSchema>;
export type ObjectivePayloadInput = z.input<typeof ObjectivePayloadSchema>;

export const MilestonePayloadSchema = CommonGoalPayloadSchema.extend({
  name: z.string().min(1),
  scope: z.array(z.string()).default([]),
  out_of_scope: z.array(z.string()).default([]),
  acceptance: z.array(z.string()).default([]),
  parent_objective: NodeIdSchema.nullable().default(null),
  decomposition_id: z.string().min(1).nullable().default(null),
  decomposition_status: DecompositionStatusSchema.nullable().default(null),
  status: z.enum(["planned", "active", "accepted", "rejected", "superseded"]),
}).superRefine((payload, ctx) => {
  rejectAiAuthoritative(payload, ctx);
});

export type MilestonePayload = z.infer<typeof MilestonePayloadSchema>;
export type MilestonePayloadInput = z.input<typeof MilestonePayloadSchema>;

export const ChangeRequestPayloadSchema = z.object({
  origin: z.literal("human"),
  authority: z.literal("authoritative"),
  status: z.enum(["proposed", "accepted", "deferred", "rejected"]),
  kind: z.enum(["new_requirement", "correction", "preference", "rejection", "constraint_change"]),
  request: z.string().min(1),
  applies_to: z.array(z.string()).default([]),
  priority: z.enum(["must", "should", "could"]),
});

export type ChangeRequestPayload = z.infer<typeof ChangeRequestPayloadSchema>;
export type ChangeRequestPayloadInput = z.input<typeof ChangeRequestPayloadSchema>;

export const ImpactAnalysisPayloadSchema = z.object({
  origin: z.literal("ai"),
  authority: z.literal("proposed"),
  status: z.enum(["proposed", "accepted", "superseded"]),
  change_request: NodeIdSchema,
  affected_objectives: z.array(NodeIdSchema).default([]),
  affected_milestones: z.array(NodeIdSchema).default([]),
  affected_artifacts: z.array(z.string()).default([]),
  preserved_constraints: z.array(z.string()).default([]),
  possibly_invalidated_acceptance: z.array(z.string()).default([]),
  recommended_action: z.enum([
    "revise_design",
    "branch_design",
    "decompose_objective",
    "defer_change",
    "plan_vertical_slice",
    "run_spike",
    "reject_as_too_large",
  ]),
  risk: z.enum(["low", "medium", "high"]),
  blast_radius: z
    .enum(["local", "module", "subsystem", "cross_system", "product_scale"])
    .optional(),
  affected_systems: z.array(z.string()).default([]),
  unknowns: z.array(z.string()).default([]),
  recommended_strategy: z
    .enum(["direct_change", "vertical_slice", "staged_rollout", "spike", "reject_as_too_large"])
    .optional(),
  first_slice: z
    .object({
      scope: z.array(z.string()).default([]),
      out_of_scope: z.array(z.string()).default([]),
      acceptance: z.array(z.string()).default([]),
    })
    .optional(),
  regression_constraints: z.array(z.string()).default([]),
});

export type ImpactAnalysisPayload = z.infer<typeof ImpactAnalysisPayloadSchema>;
export type ImpactAnalysisPayloadInput = z.input<typeof ImpactAnalysisPayloadSchema>;

export const AcceptancePayloadSchema = z.object({
  origin: z.literal("human"),
  authority: z.literal("authoritative"),
  status: z.enum(["recorded", "superseded"]),
  target: NodeIdSchema,
  decision: z.enum(["accepted", "rejected", "accepted_with_changes"]),
  notes: z.string().default(""),
  creates_change_request: NodeIdSchema.nullable().default(null),
});

export type AcceptancePayload = z.infer<typeof AcceptancePayloadSchema>;
export type AcceptancePayloadInput = z.input<typeof AcceptancePayloadSchema>;

export interface DecomposeObjectiveResult {
  decomposition_id: string;
  objective_nodes: GraphNode[];
  milestone_nodes: GraphNode[];
}

export async function decomposeObjective(options: {
  store: GraphStore;
  parentObjective: GraphNode;
  decompositionId: string;
  objectives: Array<
    Omit<
      ObjectivePayloadInput,
      "origin" | "authority" | "parent_objective" | "decomposition_id" | "decomposition_status"
    > & {
      authority?: Exclude<GoalNodeAuthority, "authoritative">;
    }
  >;
  milestones?: Array<
    Omit<
      MilestonePayloadInput,
      "origin" | "authority" | "parent_objective" | "decomposition_id" | "decomposition_status"
    > & {
      authority?: Exclude<GoalNodeAuthority, "authoritative">;
    }
  >;
  actionUsed?: NodeAction;
}): Promise<DecomposeObjectiveResult> {
  assertNodeType(options.parentObjective, "ObjectiveNode");
  assertNonEmptyDecompositionId(options.decompositionId);
  const objectiveNodes: GraphNode[] = [];
  for (const objective of options.objectives) {
    objectiveNodes.push(
      await createObjectiveNode({
        store: options.store,
        createdFrom: options.parentObjective,
        actionUsed: options.actionUsed ?? "decompose_objective",
        edgeType: "decomposes_to",
        payload: {
          ...objective,
          origin: "ai",
          authority: objective.authority ?? "proposed",
          parent_objective: options.parentObjective.id,
          decomposition_id: options.decompositionId,
          decomposition_status: "proposed",
        },
      }),
    );
  }
  const milestoneNodes: GraphNode[] = [];
  for (const milestone of options.milestones ?? []) {
    milestoneNodes.push(
      await createMilestoneNode({
        store: options.store,
        createdFrom: options.parentObjective,
        actionUsed: options.actionUsed ?? "decompose_objective",
        edgeType: "decomposes_to_milestone",
        payload: {
          ...milestone,
          origin: "ai",
          authority: milestone.authority ?? "proposed",
          parent_objective: options.parentObjective.id,
          decomposition_id: options.decompositionId,
          decomposition_status: "proposed",
        },
      }),
    );
  }
  return {
    decomposition_id: options.decompositionId,
    objective_nodes: objectiveNodes,
    milestone_nodes: milestoneNodes,
  };
}

export async function acceptObjectiveDecomposition(options: {
  store: GraphStore;
  parentObjective: GraphNode;
  decompositionId: string;
}): Promise<DecomposeObjectiveResult> {
  return updateDecomposition({
    ...options,
    actionUsed: "accept_objective_decomposition",
    objectiveStatus: "open",
    milestoneStatus: "planned",
    decompositionStatus: "accepted",
    authority: "derived",
    edgeType: "accepts_decomposition",
  });
}

export async function invalidateDecomposition(options: {
  store: GraphStore;
  parentObjective: GraphNode;
  decompositionId: string;
  reason: string;
}): Promise<DecomposeObjectiveResult> {
  const result = await updateDecomposition({
    ...options,
    actionUsed: "invalidate_decomposition",
    objectiveStatus: "superseded",
    milestoneStatus: "superseded",
    decompositionStatus: "invalidated",
    authority: "proposed",
    edgeType: "invalidates_decomposition",
  });
  await options.store.writeArtifact(
    options.parentObjective,
    `decompositions/${safeArtifactSegment(options.decompositionId)}.invalidated.json`,
    `${JSON.stringify(
      {
        decomposition_id: options.decompositionId,
        reason: options.reason,
        invalidated_nodes: [
          ...result.objective_nodes.map((node) => node.id),
          ...result.milestone_nodes.map((node) => node.id),
        ],
      },
      null,
      2,
    )}\n`,
  );
  return result;
}

export async function redecomposeObjective(options: {
  store: GraphStore;
  parentObjective: GraphNode;
  previousDecompositionId: string;
  decompositionId: string;
  reason: string;
  objectives: Parameters<typeof decomposeObjective>[0]["objectives"];
  milestones?: Parameters<typeof decomposeObjective>[0]["milestones"];
}): Promise<DecomposeObjectiveResult> {
  await invalidateDecomposition({
    store: options.store,
    parentObjective: options.parentObjective,
    decompositionId: options.previousDecompositionId,
    reason: options.reason,
  });
  const result = await decomposeObjective({
    store: options.store,
    parentObjective: options.parentObjective,
    decompositionId: options.decompositionId,
    objectives: options.objectives,
    actionUsed: "redecompose_objective",
    ...(options.milestones ? { milestones: options.milestones } : {}),
  });
  await options.store.writeArtifact(
    options.parentObjective,
    `decompositions/${safeArtifactSegment(options.decompositionId)}.json`,
    `${JSON.stringify(
      {
        decomposition_id: options.decompositionId,
        invalidates: options.previousDecompositionId,
        reason: options.reason,
        children: [
          ...result.objective_nodes.map((node) => node.id),
          ...result.milestone_nodes.map((node) => node.id),
        ],
      },
      null,
      2,
    )}\n`,
  );
  return result;
}

export async function acceptMilestone(options: {
  store: GraphStore;
  milestoneNode: GraphNode;
}): Promise<GraphNode> {
  assertNodeType(options.milestoneNode, "MilestoneNode");
  const payload = await readMilestonePayload(options.store, options.milestoneNode);
  const updatedPayload = MilestonePayloadSchema.parse({
    ...payload,
    authority: payload.origin === "ai" ? "derived" : payload.authority,
    status: "accepted",
    decomposition_status:
      payload.decomposition_status === "invalidated" ? "invalidated" : "accepted",
  });
  const updatedNode = await updateGoalNodePayload({
    store: options.store,
    node: options.milestoneNode,
    payload: updatedPayload,
    status: "active",
    tags: goalTags("milestone", updatedPayload),
  });
  await writeEventArtifact(options.store, updatedNode, "accept_milestone", {
    milestone_node: updatedNode.id,
  });
  return updatedNode;
}

export async function activateMilestone(options: {
  store: GraphStore;
  milestoneNode: GraphNode;
}): Promise<GraphNode> {
  assertNodeType(options.milestoneNode, "MilestoneNode");
  const payload = await readMilestonePayload(options.store, options.milestoneNode);
  if (payload.authority === "proposed") {
    throw new Error(
      `MilestoneNode ${options.milestoneNode.id} must be accepted before activation.`,
    );
  }
  if (payload.status !== "accepted" && payload.status !== "planned") {
    throw new Error(
      `MilestoneNode ${options.milestoneNode.id} cannot activate from status ${payload.status}.`,
    );
  }
  const updatedPayload = MilestonePayloadSchema.parse({ ...payload, status: "active" });
  return updateGoalNodePayload({
    store: options.store,
    node: options.milestoneNode,
    payload: updatedPayload,
    status: "active",
    tags: goalTags("milestone", updatedPayload),
  });
}

export async function acceptChangeRequest(options: {
  store: GraphStore;
  changeRequestNode: GraphNode;
  impactAnalysisNode: GraphNode;
}): Promise<{ change_request: GraphNode; impact_analysis: GraphNode }> {
  assertNodeType(options.changeRequestNode, "ChangeRequestNode");
  assertNodeType(options.impactAnalysisNode, "ImpactAnalysisNode");
  const changePayload = await readChangeRequestPayload(options.store, options.changeRequestNode);
  const impactPayload = await readImpactAnalysisPayload(options.store, options.impactAnalysisNode);
  if (impactPayload.change_request !== options.changeRequestNode.id) {
    throw new Error(
      `ImpactAnalysisNode ${options.impactAnalysisNode.id} does not analyze ${options.changeRequestNode.id}.`,
    );
  }
  const updatedChange = await updateGoalNodePayload({
    store: options.store,
    node: options.changeRequestNode,
    payload: ChangeRequestPayloadSchema.parse({ ...changePayload, status: "accepted" }),
    status: "active",
    tags: [
      "goal",
      "change",
      changePayload.origin,
      changePayload.authority,
      "accepted",
      changePayload.kind,
    ],
  });
  const updatedImpact = await updateGoalNodePayload({
    store: options.store,
    node: options.impactAnalysisNode,
    payload: ImpactAnalysisPayloadSchema.parse({ ...impactPayload, status: "accepted" }),
    status: "active",
    tags: [
      "goal",
      "impact",
      impactPayload.origin,
      impactPayload.authority,
      "accepted",
      impactPayload.risk,
    ],
  });
  await writeEventArtifact(options.store, updatedChange, "accept_change_request", {
    change_request: updatedChange.id,
    impact_analysis: updatedImpact.id,
  });
  return {
    change_request: updatedChange,
    impact_analysis: updatedImpact,
  };
}

export async function createObjectiveNode(options: {
  store: GraphStore;
  payload: ObjectivePayloadInput;
  createdFrom?: GraphNode | null;
  actionUsed?: NodeAction;
  edgeType?: GraphEdgeType;
}): Promise<GraphNode> {
  const payload = ObjectivePayloadSchema.parse(options.payload);
  const node = await options.store.createNode({
    type: "ObjectiveNode",
    createdFrom: options.createdFrom?.id ?? null,
    actionUsed: options.actionUsed ?? "create_objective",
    summary: payload.title,
    artifacts: ["payload.json"],
    tags: goalTags("objective", payload),
  });
  await writePayload(options.store, node, payload);
  await appendOptionalEdge(
    options.store,
    options.createdFrom,
    node,
    options.edgeType ?? "defines_objective",
  );
  return node;
}

export async function createMilestoneNode(options: {
  store: GraphStore;
  payload: MilestonePayloadInput;
  createdFrom?: GraphNode | null;
  actionUsed?: NodeAction;
  edgeType?: GraphEdgeType;
}): Promise<GraphNode> {
  const payload = MilestonePayloadSchema.parse(options.payload);
  const node = await options.store.createNode({
    type: "MilestoneNode",
    createdFrom: options.createdFrom?.id ?? null,
    actionUsed: options.actionUsed ?? "create_milestone",
    summary: payload.name,
    artifacts: ["payload.json"],
    tags: goalTags("milestone", payload),
  });
  await writePayload(options.store, node, payload);
  await appendOptionalEdge(
    options.store,
    options.createdFrom,
    node,
    options.edgeType ?? "defines_milestone",
  );
  return node;
}

export async function createChangeRequestNode(options: {
  store: GraphStore;
  payload: ChangeRequestPayloadInput;
  createdFrom?: GraphNode | null;
  actionUsed?: NodeAction;
  edgeType?: GraphEdgeType;
}): Promise<GraphNode> {
  const payload = ChangeRequestPayloadSchema.parse(options.payload);
  const node = await options.store.createNode({
    type: "ChangeRequestNode",
    createdFrom: options.createdFrom?.id ?? null,
    actionUsed: options.actionUsed ?? "record_change_request",
    summary: payload.request,
    artifacts: ["payload.json"],
    tags: ["goal", "change", payload.origin, payload.authority, payload.status, payload.kind],
  });
  await writePayload(options.store, node, payload);
  await appendOptionalEdge(
    options.store,
    options.createdFrom,
    node,
    options.edgeType ?? "requests_change",
  );
  return node;
}

export async function createImpactAnalysisNode(options: {
  store: GraphStore;
  payload: ImpactAnalysisPayloadInput;
  createdFrom: GraphNode;
  actionUsed?: NodeAction;
  edgeType?: GraphEdgeType;
}): Promise<GraphNode> {
  assertNodeType(options.createdFrom, "ChangeRequestNode");
  const payload = ImpactAnalysisPayloadSchema.parse(options.payload);
  const node = await options.store.createNode({
    type: "ImpactAnalysisNode",
    createdFrom: options.createdFrom.id,
    actionUsed: options.actionUsed ?? "analyze_change_impact",
    summary: `Impact analysis for ${payload.change_request}: ${payload.recommended_action}.`,
    artifacts: ["payload.json"],
    tags: ["goal", "impact", payload.origin, payload.authority, payload.status, payload.risk],
  });
  await writePayload(options.store, node, payload);
  await options.store.appendEdge({
    from: options.createdFrom.id,
    to: node.id,
    type: options.edgeType ?? "analyzes_change",
  });
  return node;
}

export async function createAcceptanceNode(options: {
  store: GraphStore;
  payload: AcceptancePayloadInput;
  createdFrom: GraphNode;
  actionUsed?: NodeAction;
  edgeType?: GraphEdgeType;
}): Promise<GraphNode> {
  const payload = AcceptancePayloadSchema.parse(options.payload);
  const node = await options.store.createNode({
    type: "AcceptanceNode",
    createdFrom: options.createdFrom.id,
    actionUsed: options.actionUsed ?? "record_acceptance",
    summary: `Human acceptance for ${payload.target}: ${payload.decision}.`,
    artifacts: ["payload.json"],
    tags: [
      "goal",
      "acceptance",
      payload.origin,
      payload.authority,
      payload.status,
      payload.decision,
    ],
  });
  await writePayload(options.store, node, payload);
  await options.store.appendEdge({
    from: options.createdFrom.id,
    to: node.id,
    type: options.edgeType ?? "records_acceptance",
  });
  return node;
}

function rejectAiAuthoritative(
  payload: z.infer<typeof CommonGoalPayloadSchema>,
  ctx: z.RefinementCtx,
): void {
  if (payload.origin === "ai" && payload.authority === "authoritative") {
    ctx.addIssue({
      code: "custom",
      message: "AI-derived goal nodes cannot be authoritative.",
      path: ["authority"],
    });
  }
}

function goalTags(
  kind: string,
  payload: { origin: GoalNodeOrigin; authority: GoalNodeAuthority; status: string },
): string[] {
  return ["goal", kind, payload.origin, payload.authority, payload.status];
}

async function writePayload(store: GraphStore, node: GraphNode, payload: unknown): Promise<void> {
  await store.writeArtifactJson(node, "payload.json", payload);
}

async function appendOptionalEdge(
  store: GraphStore,
  source: GraphNode | null | undefined,
  target: GraphNode,
  edgeType: GraphEdgeType,
): Promise<void> {
  if (!source) return;
  await store.appendEdge({ from: source.id, to: target.id, type: edgeType });
}

async function updateDecomposition(options: {
  store: GraphStore;
  parentObjective: GraphNode;
  decompositionId: string;
  actionUsed: NodeAction;
  objectiveStatus: ObjectivePayload["status"];
  milestoneStatus: MilestonePayload["status"];
  decompositionStatus: NonNullable<ObjectivePayload["decomposition_status"]>;
  authority: Exclude<GoalNodeAuthority, "authoritative">;
  edgeType: GraphEdgeType;
}): Promise<DecomposeObjectiveResult> {
  assertNodeType(options.parentObjective, "ObjectiveNode");
  assertNonEmptyDecompositionId(options.decompositionId);
  const nodes = await options.store.listNodes();
  const objectiveNodes: GraphNode[] = [];
  const milestoneNodes: GraphNode[] = [];
  for (const node of nodes) {
    if (node.type === "ObjectiveNode") {
      const payload = await readObjectivePayload(options.store, node);
      if (
        payload.parent_objective === options.parentObjective.id &&
        payload.decomposition_id === options.decompositionId
      ) {
        objectiveNodes.push(
          await updateGoalNodePayload({
            store: options.store,
            node,
            payload: ObjectivePayloadSchema.parse({
              ...payload,
              authority: options.authority,
              status: options.objectiveStatus,
              decomposition_status: options.decompositionStatus,
            }),
            status: options.objectiveStatus === "superseded" ? "superseded" : "active",
            tags: goalTags("objective", {
              ...payload,
              authority: options.authority,
              status: options.objectiveStatus,
            }),
          }),
        );
      }
    }
    if (node.type === "MilestoneNode") {
      const payload = await readMilestonePayload(options.store, node);
      if (
        payload.parent_objective === options.parentObjective.id &&
        payload.decomposition_id === options.decompositionId
      ) {
        milestoneNodes.push(
          await updateGoalNodePayload({
            store: options.store,
            node,
            payload: MilestonePayloadSchema.parse({
              ...payload,
              authority: options.authority,
              status: options.milestoneStatus,
              decomposition_status: options.decompositionStatus,
            }),
            status: options.milestoneStatus === "superseded" ? "superseded" : "active",
            tags: goalTags("milestone", {
              ...payload,
              authority: options.authority,
              status: options.milestoneStatus,
            }),
          }),
        );
      }
    }
  }
  if (objectiveNodes.length === 0 && milestoneNodes.length === 0) {
    throw new Error(
      `No decomposition nodes found for ${options.parentObjective.id} / ${options.decompositionId}.`,
    );
  }
  await options.store.writeArtifactJson(
    options.parentObjective,
    `decompositions/${safeArtifactSegment(options.decompositionId)}.${options.actionUsed}.json`,
    {
      action: options.actionUsed,
      edge_type: options.edgeType,
      decomposition_id: options.decompositionId,
      objective_nodes: objectiveNodes.map((node) => node.id),
      milestone_nodes: milestoneNodes.map((node) => node.id),
    },
  );
  return {
    decomposition_id: options.decompositionId,
    objective_nodes: objectiveNodes,
    milestone_nodes: milestoneNodes,
  };
}

async function updateGoalNodePayload<TPayload>(options: {
  store: GraphStore;
  node: GraphNode;
  payload: TPayload;
  status?: GraphNode["status"];
  tags: string[];
}): Promise<GraphNode> {
  await writePayload(options.store, options.node, options.payload);
  return options.store.updateNode({
    ...options.node,
    ...(options.status ? { status: options.status } : {}),
    tags: options.tags,
  });
}

export async function readObjectivePayload(
  store: GraphStore,
  node: GraphNode,
): Promise<ObjectivePayload> {
  assertNodeType(node, "ObjectiveNode");
  return ObjectivePayloadSchema.parse(await store.readArtifactJson<unknown>(node, "payload.json"));
}

export async function readMilestonePayload(
  store: GraphStore,
  node: GraphNode,
): Promise<MilestonePayload> {
  assertNodeType(node, "MilestoneNode");
  return MilestonePayloadSchema.parse(await store.readArtifactJson<unknown>(node, "payload.json"));
}

export async function readChangeRequestPayload(
  store: GraphStore,
  node: GraphNode,
): Promise<ChangeRequestPayload> {
  assertNodeType(node, "ChangeRequestNode");
  return ChangeRequestPayloadSchema.parse(
    await store.readArtifactJson<unknown>(node, "payload.json"),
  );
}

export async function readImpactAnalysisPayload(
  store: GraphStore,
  node: GraphNode,
): Promise<ImpactAnalysisPayload> {
  assertNodeType(node, "ImpactAnalysisNode");
  return ImpactAnalysisPayloadSchema.parse(
    await store.readArtifactJson<unknown>(node, "payload.json"),
  );
}

function assertNonEmptyDecompositionId(decompositionId: string): void {
  if (decompositionId.trim().length === 0) {
    throw new Error("decompositionId must not be empty.");
  }
}

function safeArtifactSegment(value: string): string {
  const segment = value.replace(/[^A-Za-z0-9_-]/g, "_");
  if (segment.length === 0) {
    throw new Error(`Invalid artifact segment: ${value}`);
  }
  return segment;
}

async function writeEventArtifact(
  store: GraphStore,
  node: GraphNode,
  action: string,
  payload: Record<string, unknown>,
): Promise<void> {
  await store.writeArtifactJson(node, `events/${safeArtifactSegment(action)}.json`, {
    action,
    ...payload,
  });
}
