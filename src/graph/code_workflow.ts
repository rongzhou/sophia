import { diffSophiaArtifacts } from "../analysis/artifact_diff.js";
import { checkSophiaFiles } from "../lang/checker.js";
import { auditConstraints } from "../analysis/constraint_audit.js";
import type { CheckResult } from "../lang/diagnostics.js";
import type { GraphEdge } from "./edges.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";

export async function readCodeNodeFiles(
  store: GraphStore,
  codeNode: GraphNode,
): Promise<Record<string, string>> {
  const files: Record<string, string> = {};
  for (const artifact of codeNode.artifacts) {
    if (!artifact.startsWith("files/") || !artifact.endsWith(".sophia")) continue;
    files[artifact.slice("files/".length)] = await store.readArtifact(codeNode, artifact);
  }
  return files;
}

export async function inferBeforeCodeNodeForDiff(
  store: GraphStore,
  afterCodeNode: GraphNode,
): Promise<GraphNode> {
  if (!afterCodeNode.created_from) {
    throw new Error(
      `CodeNode ${afterCodeNode.id} has no parent; pass before-code-node explicitly.`,
    );
  }
  const parent = await store.readNode(afterCodeNode.created_from);
  if (parent.type === "CodeNode") return parent;
  if (parent.type === "CheckResultNode" && parent.created_from) {
    const checkedNode = await store.readNode(parent.created_from);
    if (checkedNode.type === "CodeNode") return checkedNode;
  }
  throw new Error(
    `Cannot infer before CodeNode for ${afterCodeNode.id}; pass before-code-node explicitly.`,
  );
}

export async function createCheckResultNode(options: {
  store: GraphStore;
  codeNode: GraphNode;
  files: Record<string, string>;
}): Promise<{ node: GraphNode; result: CheckResult }> {
  const result = checkSophiaFiles(options.files);
  return createResultNode({
    store: options.store,
    sourceNode: options.codeNode,
    nodeType: "CheckResultNode",
    edgeType: "checks",
    action_used: "check_code",
    tags: ["check"],
    result,
    summary: result.ok
      ? "Sophia check passed."
      : `Sophia check failed with ${result.diagnostics.length} diagnostic(s).`,
  });
}

export async function createAuditNode(options: {
  store: GraphStore;
  codeNode: GraphNode;
  files: Record<string, string>;
}): Promise<{ node: GraphNode; result: CheckResult }> {
  const pseudoNode = await findAncestorPseudocodeNode(options.store, options.codeNode.id);
  const pseudocode = await options.store.readArtifact(pseudoNode, "content.pseudo");
  const result = auditConstraints({ pseudocode, files: options.files });
  return createResultNode({
    store: options.store,
    sourceNode: options.codeNode,
    nodeType: "AuditNode",
    edgeType: "audits",
    action_used: "constraint_audit",
    tags: ["audit"],
    result,
    summary: result.ok
      ? "Constraint audit passed."
      : `Constraint audit failed with ${result.diagnostics.length} diagnostic(s).`,
  });
}

export async function createArtifactDiffNode(options: {
  store: GraphStore;
  beforeNode: GraphNode;
  afterNode: GraphNode;
  beforeFiles: Record<string, string>;
  afterFiles: Record<string, string>;
}): Promise<{ node: GraphNode; result: CheckResult }> {
  const result = diffSophiaArtifacts({ before: options.beforeFiles, after: options.afterFiles });
  return createResultNode({
    store: options.store,
    sourceNode: options.afterNode,
    nodeType: "ArtifactDiffNode",
    edgeType: "diffs",
    action_used: "artifact_diff",
    tags: ["diff"],
    result,
    summary: `Compared ${options.beforeNode.id} -> ${options.afterNode.id}.`,
  });
}

export async function createSelectionNode(options: {
  store: GraphStore;
  codeNode: GraphNode;
}): Promise<GraphNode> {
  await assertCodeNodeCanMaterialize(options.store, options.codeNode);
  const { node } = await createResultNode({
    store: options.store,
    sourceNode: options.codeNode,
    nodeType: "SelectionNode",
    edgeType: "selects",
    action_used: "select_code",
    tags: ["selection"],
    result: { ok: true, diagnostics: [] },
    resultArtifact: { ok: true, selected_code_node: options.codeNode.id },
    summary: `Selected ${options.codeNode.id} after deterministic verification passed.`,
  });
  return node;
}

export function summarizeResultNode(node: GraphNode, result: CheckResult) {
  return {
    id: node.id,
    ok: result.ok,
    diagnostics: result.diagnostics.length,
    errors: result.diagnostics.filter((diagnostic) => diagnostic.severity === "error").length,
    warnings: result.diagnostics.filter((diagnostic) => diagnostic.severity === "warning").length,
  };
}

export async function assertCodeNodeCanMaterialize(
  store: GraphStore,
  codeNode: GraphNode,
): Promise<void> {
  const edges = await store.listEdges();
  const children = edges.filter((edge) => edge.from === codeNode.id);
  const checkNode = await latestResultNode(store, children, "checks", "CheckResultNode");
  const auditNode = await latestResultNode(store, children, "audits", "AuditNode");
  const diffNode = await latestResultNode(store, children, "diffs", "ArtifactDiffNode");

  if (!checkNode) {
    throw new Error(`CodeNode ${codeNode.id} has no CheckResultNode; run graph check first.`);
  }
  const checkResult = await store.readArtifactJson<CheckResult>(checkNode, "result.json");
  if (!checkResult.ok) {
    throw new Error(
      `CodeNode ${codeNode.id} cannot materialize because latest check failed: ${checkNode.id}.`,
    );
  }
  if (!auditNode) {
    throw new Error(`CodeNode ${codeNode.id} has no AuditNode; run graph audit first.`);
  }
  const auditResult = await store.readArtifactJson<CheckResult>(auditNode, "result.json");
  if (!auditResult.ok) {
    throw new Error(
      `CodeNode ${codeNode.id} cannot materialize because latest audit failed: ${auditNode.id}.`,
    );
  }
  if (codeNode.action_used === "repair_code") {
    if (!diffNode) {
      throw new Error(
        `Repaired CodeNode ${codeNode.id} has no ArtifactDiffNode; run graph diff first.`,
      );
    }
    const diffResult = await store.readArtifactJson<CheckResult>(diffNode, "result.json");
    if (!diffResult.ok) {
      throw new Error(
        `Repaired CodeNode ${codeNode.id} cannot materialize because latest diff failed: ${diffNode.id}.`,
      );
    }
  }
}

export async function assertCodeNodeSelectedForMaterialize(
  store: GraphStore,
  codeNode: GraphNode,
): Promise<void> {
  const edges = await store.listEdges();
  const selectionEdges = edges.filter(
    (edge) => edge.from === codeNode.id && edge.type === "selects",
  );
  const selectionNodes = await Promise.all(
    selectionEdges.map(async (edge) => store.readNode(edge.to)),
  );
  if (selectionNodes.some((node) => node.type === "SelectionNode")) {
    return;
  }
  throw new Error(
    `CodeNode ${codeNode.id} has no SelectionNode; run graph select before materialize.`,
  );
}

export async function findAncestorPseudocodeNode(store: GraphStore, startNodeId: string) {
  let current = await store.readNode(startNodeId);
  const visited = new Set<string>();
  while (current.created_from) {
    if (visited.has(current.id)) {
      throw new Error(`Cycle detected while tracing ancestors from ${startNodeId}.`);
    }
    visited.add(current.id);
    current = await store.readNode(current.created_from);
    if (current.type === "PseudocodeNode") {
      return current;
    }
  }
  throw new Error(`No ancestor PseudocodeNode found for ${startNodeId}.`);
}

async function latestResultNode(
  store: GraphStore,
  edges: GraphEdge[],
  edgeType: string,
  nodeType: GraphNode["type"],
): Promise<GraphNode | null> {
  const candidates = await Promise.all(
    edges.filter((edge) => edge.type === edgeType).map(async (edge) => store.readNode(edge.to)),
  );
  return (
    candidates
      .filter((node) => node.type === nodeType)
      .sort((left, right) => right.id.localeCompare(left.id))[0] ?? null
  );
}

async function createResultNode(options: {
  store: GraphStore;
  sourceNode: GraphNode;
  nodeType: GraphNode["type"];
  edgeType: GraphEdge["type"];
  action_used: string;
  tags: string[];
  result: CheckResult;
  summary: string;
  resultArtifact?: unknown;
}): Promise<{ node: GraphNode; result: CheckResult }> {
  const node = await options.store.createNode({
    type: options.nodeType,
    status: options.result.ok ? "active" : "failed",
    createdFrom: options.sourceNode.id,
    action_used: options.action_used,
    ...(options.sourceNode.goal ? { goal: options.sourceNode.goal } : {}),
    summary: options.summary,
    artifacts: ["result.json"],
    tags: options.tags,
  });
  await options.store.writeArtifact(
    node,
    "result.json",
    `${JSON.stringify(options.resultArtifact ?? options.result, null, 2)}\n`,
  );
  await options.store.appendEdge({
    from: options.sourceNode.id,
    to: node.id,
    type: options.edgeType,
  });
  return { node, result: options.result };
}
