import { z } from "zod";

export const NodeStatusSchema = z.enum(["active", "abandoned", "superseded", "failed", "merged"]);

export type NodeStatus = z.infer<typeof NodeStatusSchema>;

export const GraphNodeTypeSchema = z.enum([
  "GoalNode",
  "DecisionNode",
  "PseudocodeNode",
  "PseudocodeCheckNode",
  "RawLlmNode",
  "ObjectiveNode",
  "MilestoneNode",
  "ChangeRequestNode",
  "ImpactAnalysisNode",
  "AcceptanceNode",
  "CodeNode",
  "CheckResultNode",
  "AuditNode",
  "ArtifactDiffNode",
  "SelectionNode",
  "MaterializeNode",
]);

export type GraphNodeType = z.infer<typeof GraphNodeTypeSchema>;

export const NodeIdSchema = z.string().regex(/^N\d{4,}$/);

export type NodeId = z.infer<typeof NodeIdSchema>;

export const NodeActionSchema = z.enum([
  "start",
  "llm_decide",
  "add_pseudo",
  "design_solution",
  "pseudo_check",
  "revise_design",
  "implement_design",
  "check_code",
  "constraint_audit",
  "artifact_diff",
  "repair_code",
  "select_code",
  "materialize_code",
  "create_objective",
  "decompose_objective",
  "accept_objective_decomposition",
  "invalidate_decomposition",
  "redecompose_objective",
  "create_milestone",
  "record_change_request",
  "analyze_change_impact",
  "record_acceptance",
]);

export type NodeAction = z.infer<typeof NodeActionSchema>;

export const GraphEdgeTypeSchema = z.enum([
  "designs_solution",
  "decides",
  "implements_design",
  "repairs",
  "revises",
  "checks",
  "audits",
  "diffs",
  "selects",
  "materializes",
  "applies",
  "defines_objective",
  "decomposes_to",
  "decomposes_to_milestone",
  "accepts_decomposition",
  "invalidates_decomposition",
  "defines_milestone",
  "requests_change",
  "analyzes_change",
  "records_acceptance",
]);

export type GraphEdgeType = z.infer<typeof GraphEdgeTypeSchema>;

export const GraphEdgeSchema = z.object({
  from: NodeIdSchema,
  to: NodeIdSchema,
  type: GraphEdgeTypeSchema,
});

export type GraphEdge = z.infer<typeof GraphEdgeSchema>;

export const GraphNodeSchema = z.object({
  id: NodeIdSchema,
  type: GraphNodeTypeSchema,
  status: NodeStatusSchema,
  created_from: NodeIdSchema.nullable(),
  action_used: NodeActionSchema,
  goal: z.string().optional(),
  version: z.number().int().nonnegative(),
  artifacts: z.array(z.string()),
  summary: z.string(),
  score: z.record(z.string(), z.number()),
  tags: z.array(z.string()),
  model: z.string().optional(),
  prompt_artifact: z.string().optional(),
  response_artifact: z.string().optional(),
});

export type GraphNode = z.infer<typeof GraphNodeSchema>;

export function nodeDirectoryName(node: Pick<GraphNode, "id" | "type">): string {
  const suffix = node.type
    .replace(/Node$/, "")
    .replace(/([a-z])([A-Z])/g, "$1_$2")
    .toLowerCase();
  return `${node.id}.${suffix}`;
}

export function assertNodeType<TType extends GraphNodeType>(
  node: GraphNode,
  type: TType,
): asserts node is GraphNode & { type: TType } {
  if (node.type !== type) {
    throw new Error(`Expected ${type}, got ${node.type}.`);
  }
}

export function assertNodeTypeIn<TTypes extends readonly GraphNodeType[]>(
  node: GraphNode,
  types: TTypes,
): asserts node is GraphNode & { type: TTypes[number] } {
  if (!types.includes(node.type)) {
    throw new Error(`Expected ${types.join(" or ")}, got ${node.type}.`);
  }
}
