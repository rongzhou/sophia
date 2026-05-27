import type { CheckResult } from "../lang/diagnostics.js";
import type { GraphEdge } from "./edges.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";
import { countBy } from "../util/strings.js";

export async function buildFocusedGraphContext(options: {
  store: GraphStore;
  currentNode: GraphNode;
  nodes: GraphNode[];
  edges: GraphEdge[];
}): Promise<{
  ancestry: Array<Record<string, unknown>>;
  adjacent_edges: GraphEdge[];
  child_results: Array<Record<string, unknown>>;
}> {
  const nodesById = new Map(options.nodes.map((node) => [node.id, node]));
  const ancestry: GraphNode[] = [];
  let cursor: GraphNode | undefined = options.currentNode;
  const visited = new Set<string>();
  while (cursor && !visited.has(cursor.id)) {
    visited.add(cursor.id);
    ancestry.push(cursor);
    cursor = cursor.created_from ? nodesById.get(cursor.created_from) : undefined;
  }
  const adjacentEdges = options.edges
    .filter((edge) => edge.from === options.currentNode.id || edge.to === options.currentNode.id)
    .sort((left, right) => `${left.from}:${left.to}`.localeCompare(`${right.from}:${right.to}`));
  const childResults = await Promise.all(
    adjacentEdges
      .filter((edge) => edge.from === options.currentNode.id)
      .map((edge) => nodesById.get(edge.to))
      .filter(isResultNode)
      .map(async (node) => ({
        node: summarizeNodeForPrompt(node),
        result: await readResultSummary(options.store, node),
      })),
  );
  return {
    ancestry: ancestry.map(summarizeNodeForPrompt),
    adjacent_edges: adjacentEdges,
    child_results: childResults,
  };
}

export async function summarizeCurrentNode(
  store: GraphStore,
  node: GraphNode,
): Promise<Record<string, unknown>> {
  const summary: Record<string, unknown> = summarizeNodeForPrompt(node);
  if (
    node.type === "PseudocodeCheckNode" ||
    node.type === "CheckResultNode" ||
    node.type === "AuditNode"
  ) {
    summary.result = await readResultSummary(store, node);
  }
  return summary;
}

async function readResultSummary(
  store: GraphStore,
  node: GraphNode,
): Promise<Record<string, unknown> | null> {
  if (!node.artifacts.includes("result.json")) return null;
  const result = await store.readArtifactJson<CheckResult>(node, "result.json");
  return {
    ok: result.ok,
    diagnostics: result.diagnostics.map((diagnostic) => ({
      code: diagnostic.code,
      severity: diagnostic.severity,
      problem: diagnostic.problem,
    })),
  };
}

function summarizeNodeForPrompt(node: GraphNode): Record<string, unknown> {
  return {
    id: node.id,
    type: node.type,
    status: node.status,
    action_used: node.action_used,
    created_from: node.created_from,
    goal: node.goal ?? null,
    summary: node.summary,
    tags: node.tags,
    model: node.model ?? null,
  };
}

function isResultNode(node: GraphNode | undefined): node is GraphNode {
  return (
    node?.type === "PseudocodeCheckNode" ||
    node?.type === "CheckResultNode" ||
    node?.type === "AuditNode" ||
    node?.type === "ArtifactDiffNode"
  );
}
