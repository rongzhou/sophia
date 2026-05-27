import { z } from "zod";

export const NodeStatusSchema = z.enum(["active", "abandoned", "superseded", "failed", "merged"]);

export type NodeStatus = z.infer<typeof NodeStatusSchema>;

export const GraphNodeTypeSchema = z.enum([
  "GoalNode",
  "DecisionNode",
  "PseudocodeNode",
  "PseudocodeCheckNode",
  "RawLlmNode",
  "CodeNode",
  "CheckResultNode",
  "AuditNode",
  "ArtifactDiffNode",
  "SelectionNode",
  "MaterializeNode",
]);

export type GraphNodeType = z.infer<typeof GraphNodeTypeSchema>;

export const GraphNodeSchema = z.object({
  id: z.string().regex(/^N\d{4,}$/),
  type: GraphNodeTypeSchema,
  status: NodeStatusSchema,
  created_from: z
    .string()
    .regex(/^N\d{4,}$/)
    .nullable(),
  action_used: z.string(),
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
