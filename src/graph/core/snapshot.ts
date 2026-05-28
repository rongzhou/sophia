import type { GraphEdge } from "./nodes.js";
import type { GraphNode, NodeAction } from "./nodes.js";

export interface GraphSnapshot {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export function latestChildFor(
  snapshot: GraphSnapshot,
  nodeId: string,
  type: GraphNode["type"],
): GraphNode | null {
  const childIds = new Set(
    snapshot.edges.filter((edge) => edge.from === nodeId).map((edge) => edge.to),
  );
  return (
    snapshot.nodes
      .filter((node) => node.type === type && childIds.has(node.id))
      .sort((left, right) => right.id.localeCompare(left.id))[0] ?? null
  );
}

export function hasRelatedNode(
  snapshot: GraphSnapshot,
  nodeId: string,
  type: GraphNode["type"],
): boolean {
  const relatedIds = new Set(
    snapshot.edges
      .filter((edge) => edge.from === nodeId || edge.to === nodeId)
      .flatMap((edge) => [edge.from, edge.to]),
  );
  return snapshot.nodes.some((node) => node.type === type && relatedIds.has(node.id));
}

export function countDescendantActions(
  snapshot: GraphSnapshot,
  nodeId: string,
  action_used: NodeAction,
): number {
  const seen = new Set<string>();
  const stack = [nodeId];
  let attempts = 0;
  while (stack.length > 0) {
    const current = stack.pop();
    if (!current || seen.has(current)) continue;
    seen.add(current);
    const children = snapshot.edges.filter((edge) => edge.from === current).map((edge) => edge.to);
    for (const childId of children) {
      const child = snapshot.nodes.find((node) => node.id === childId);
      if (!child) continue;
      if (child.action_used === action_used) attempts += 1;
      stack.push(child.id);
    }
  }
  return attempts;
}
