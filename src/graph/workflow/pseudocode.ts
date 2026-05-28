import type { CheckResult } from "../../lang/ast/diagnostics.js";
import { checkPseudocode } from "../../pseudo/check.js";
import { assertNodeType, type GraphNode } from "../core/nodes.js";
import type { GraphStore } from "../core/store.js";

export async function createPseudocodeCheckNode(options: {
  store: GraphStore;
  pseudoNode: GraphNode;
  pseudocode: string;
  tags?: string[];
  summary?: string;
}): Promise<{ node: GraphNode; result: CheckResult }> {
  assertNodeType(options.pseudoNode, "PseudocodeNode");
  const result = checkPseudocode(options.pseudocode);
  const node = await options.store.createNode({
    type: "PseudocodeCheckNode",
    status: result.ok ? "active" : "failed",
    createdFrom: options.pseudoNode.id,
    actionUsed: "pseudo_check",
    goal: options.pseudoNode.goal,
    summary:
      options.summary ??
      (result.ok
        ? "Pseudocode check passed."
        : `Pseudocode check failed with ${result.diagnostics.length} diagnostic(s).`),
    artifacts: ["result.json"],
    tags: options.tags ?? ["pseudo", "check"],
  });
  await options.store.writeArtifactJson(node, "result.json", result);
  await options.store.appendEdge({ from: options.pseudoNode.id, to: node.id, type: "checks" });
  return { node, result };
}

export async function assertPseudocodeNodeCanImplement(
  store: GraphStore,
  pseudoNode: GraphNode,
): Promise<{ node: GraphNode; result: CheckResult }> {
  assertNodeType(pseudoNode, "PseudocodeNode");
  const latestCheck = await latestPseudocodeCheckNode(store, pseudoNode);
  if (!latestCheck) {
    throw new Error(
      `PseudocodeNode ${pseudoNode.id} has no PseudocodeCheckNode; run graph pseudo-check ${pseudoNode.id} first.`,
    );
  }
  const result = await store.readArtifactJson<CheckResult>(latestCheck, "result.json");
  if (!result.ok) {
    const errorCount = result.diagnostics.filter(
      (diagnostic) => diagnostic.severity === "error",
    ).length;
    const warningCount = result.diagnostics.filter(
      (diagnostic) => diagnostic.severity === "warning",
    ).length;
    throw new Error(
      `PseudocodeNode ${pseudoNode.id} cannot implement because latest pseudocode check failed: ${latestCheck.id} (${errorCount} error(s), ${warningCount} warning(s)); run graph revise-design ${latestCheck.id} or update the pseudocode and re-run graph pseudo-check ${pseudoNode.id}.`,
    );
  }
  return { node: latestCheck, result };
}

async function latestPseudocodeCheckNode(
  store: GraphStore,
  pseudoNode: GraphNode,
): Promise<GraphNode | null> {
  const edges = await store.listEdges();
  const checks = await Promise.all(
    edges
      .filter((edge) => edge.from === pseudoNode.id && edge.type === "checks")
      .map(async (edge) => store.readNode(edge.to)),
  );
  return (
    checks
      .filter((node) => node.type === "PseudocodeCheckNode")
      .sort((left, right) => right.id.localeCompare(left.id))[0] ?? null
  );
}
