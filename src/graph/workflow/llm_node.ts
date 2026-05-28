import type { implementDesignWithOllama } from "../../llm/tasks/implement_design.js";
import type { LlmCallExecutionError, LlmCallParseError } from "../../llm/errors.js";
import type { GraphEdgeType } from "../core/nodes.js";
import type { reviseDesignWithOllama } from "../../llm/tasks/revise_design.js";
import type { designSolutionWithOllama } from "../../llm/tasks/design_solution.js";
import type { repairCodeWithOllama } from "../../llm/tasks/repair.js";
import { assertNodeType, type GraphNode, type NodeAction } from "../core/nodes.js";
import type { GraphStore } from "../core/store.js";

export const DEFAULT_CODE_REPAIR_ATTEMPT_LIMIT = 2;

export async function createDesignedPseudocodeNode(options: {
  store: GraphStore;
  goalNode: GraphNode;
  result: Awaited<ReturnType<typeof designSolutionWithOllama>>;
  model: string;
  status?: GraphNode["status"];
}): Promise<GraphNode> {
  const pseudoNode = await options.store.createNode({
    type: "PseudocodeNode",
    ...(options.status ? { status: options.status } : {}),
    createdFrom: options.goalNode.id,
    actionUsed: "design_solution",
    goal: options.goalNode.goal,
    summary:
      options.result.output.status === "designed"
        ? `Designed pseudocode for ${options.goalNode.id} with ${options.model}.`
        : `Solution design needs clarification for ${options.goalNode.id}.`,
    artifacts: ["prompt.txt", "response.txt", "design_solution.json", "content.pseudo"],
    tags:
      options.result.output.status === "designed"
        ? ["pseudo", "design"]
        : ["pseudo", "design", "failed", "needs_clarification"],
    model: options.model,
    promptArtifact: "prompt.txt",
    responseArtifact: "response.txt",
  });
  await options.store.writeArtifact(pseudoNode, "prompt.txt", options.result.prompt);
  await options.store.writeArtifact(pseudoNode, "response.txt", options.result.rawResponse);
  await options.store.writeArtifactJson(pseudoNode, "design_solution.json", options.result.output);
  await options.store.writeArtifact(pseudoNode, "content.pseudo", options.result.output.pseudocode);
  await options.store.appendEdge({
    from: options.goalNode.id,
    to: pseudoNode.id,
    type: "designs_solution",
  });
  return pseudoNode;
}

export async function createImplementedCodeNode(options: {
  store: GraphStore;
  pseudoNode: GraphNode;
  result: Awaited<ReturnType<typeof implementDesignWithOllama>>;
  model: string;
}): Promise<GraphNode> {
  const artifacts = ["prompt.txt", "response.txt", "implementation.json"];
  for (const filePath of Object.keys(options.result.output.files).sort()) {
    artifacts.push(`files/${filePath}`);
  }
  const codeNode = await options.store.createNode({
    type: "CodeNode",
    createdFrom: options.pseudoNode.id,
    actionUsed: "implement_design",
    goal: options.pseudoNode.goal,
    summary: `Implemented ${options.pseudoNode.id} with ${options.model}.`,
    artifacts,
    tags: ["code", "implementation"],
    model: options.model,
    promptArtifact: "prompt.txt",
    responseArtifact: "response.txt",
  });
  await options.store.writeArtifact(codeNode, "prompt.txt", options.result.prompt);
  await options.store.writeArtifact(codeNode, "response.txt", options.result.rawResponse);
  await options.store.writeArtifactJson(codeNode, "implementation.json", options.result.output);
  for (const [filePath, content] of Object.entries(options.result.output.files)) {
    await options.store.writeArtifact(codeNode, `files/${filePath}`, content);
  }
  await options.store.appendEdge({
    from: options.pseudoNode.id,
    to: codeNode.id,
    type: "implements_design",
  });
  return codeNode;
}

export async function createRepairedCodeNode(options: {
  store: GraphStore;
  sourceCodeNode: GraphNode;
  checkNode: GraphNode;
  result: Awaited<ReturnType<typeof repairCodeWithOllama>>;
  model: string;
}): Promise<GraphNode> {
  const artifacts = ["prompt.txt", "response.txt", "repair.json"];
  for (const filePath of Object.keys(options.result.output.files).sort()) {
    artifacts.push(`files/${filePath}`);
  }
  const repairedNode = await options.store.createNode({
    type: "CodeNode",
    createdFrom: options.checkNode.id,
    actionUsed: "repair_code",
    goal: options.sourceCodeNode.goal,
    summary: `Repaired ${options.sourceCodeNode.id} from ${options.checkNode.id} with ${options.model}.`,
    artifacts,
    tags: ["code", "repair"],
    model: options.model,
    promptArtifact: "prompt.txt",
    responseArtifact: "response.txt",
  });
  await options.store.writeArtifact(repairedNode, "prompt.txt", options.result.prompt);
  await options.store.writeArtifact(repairedNode, "response.txt", options.result.rawResponse);
  await options.store.writeArtifactJson(repairedNode, "repair.json", options.result.output);
  for (const [filePath, content] of Object.entries(options.result.output.files)) {
    await options.store.writeArtifact(repairedNode, `files/${filePath}`, content);
  }
  await options.store.appendEdge({
    from: options.checkNode.id,
    to: repairedNode.id,
    type: "repairs",
  });
  return repairedNode;
}

export async function createRevisedDesignNode(options: {
  store: GraphStore;
  sourcePseudoNode: GraphNode;
  checkNode: GraphNode;
  result: Awaited<ReturnType<typeof reviseDesignWithOllama>>;
  model: string;
  status?: GraphNode["status"];
  tags?: string[];
  summary?: string;
}): Promise<GraphNode> {
  const revisedNode = await options.store.createNode({
    type: "PseudocodeNode",
    ...(options.status ? { status: options.status } : {}),
    createdFrom: options.checkNode.id,
    actionUsed: "revise_design",
    goal: options.sourcePseudoNode.goal,
    summary:
      options.summary ??
      `Revised ${options.sourcePseudoNode.id} from ${options.checkNode.id} with ${options.model}.`,
    artifacts: ["prompt.txt", "response.txt", "revision.json", "content.pseudo"],
    tags: options.tags ?? ["pseudo", "revise"],
    model: options.model,
    promptArtifact: "prompt.txt",
    responseArtifact: "response.txt",
  });
  await options.store.writeArtifact(revisedNode, "prompt.txt", options.result.prompt);
  await options.store.writeArtifact(revisedNode, "response.txt", options.result.rawResponse);
  await options.store.writeArtifactJson(revisedNode, "revision.json", options.result.output);
  await options.store.writeArtifact(
    revisedNode,
    "content.pseudo",
    options.result.output.pseudocode,
  );
  await options.store.appendEdge({
    from: options.checkNode.id,
    to: revisedNode.id,
    type: "revises",
  });
  return revisedNode;
}

export async function createDesignBudgetExhaustedNode(options: {
  store: GraphStore;
  sourcePseudoNode: GraphNode;
  checkNode: GraphNode;
  revisionsUsed: number;
  model: string;
}): Promise<GraphNode> {
  const failedNode = await options.store.createNode({
    type: "PseudocodeNode",
    status: "failed",
    createdFrom: options.checkNode.id,
    actionUsed: "revise_design",
    goal: options.sourcePseudoNode.goal,
    summary: `Pseudocode revision budget exhausted after ${options.revisionsUsed} attempt(s).`,
    artifacts: ["result.json"],
    tags: ["pseudo", "revise", "failed"],
    model: options.model,
  });
  await options.store.writeArtifactJson(failedNode, "result.json", {
    ok: false,
    reason: "pseudo_revision_budget_exhausted",
    revisions_used: options.revisionsUsed,
  });
  await options.store.appendEdge({
    from: options.checkNode.id,
    to: failedNode.id,
    type: "revises",
  });
  return failedNode;
}

export async function createRawLlmFailureNode(options: {
  store: GraphStore;
  createdFrom: GraphNode;
  actionUsed: NodeAction;
  edgeType: GraphEdgeType;
  tags: string[];
  model: string;
  error: LlmCallParseError | LlmCallExecutionError;
  goal?: string | undefined;
}): Promise<GraphNode> {
  const goal = options.goal ?? options.createdFrom.goal;
  const failedNode = await options.store.createNode({
    type: "RawLlmNode",
    status: "failed",
    createdFrom: options.createdFrom.id,
    actionUsed: options.actionUsed,
    goal,
    summary: options.error.message,
    artifacts: ["prompt.txt", "response.txt", "error.txt"],
    tags: options.tags,
    model: options.model,
    promptArtifact: "prompt.txt",
    responseArtifact: "response.txt",
  });
  await options.store.writeArtifact(failedNode, "prompt.txt", options.error.prompt);
  await options.store.writeArtifact(failedNode, "response.txt", options.error.rawResponse);
  await options.store.writeArtifact(failedNode, "error.txt", `${options.error.message}\n`);
  await options.store.appendEdge({
    from: options.createdFrom.id,
    to: failedNode.id,
    type: options.edgeType,
  });
  return failedNode;
}

export async function countDesignRevisionAttempts(
  store: GraphStore,
  pseudoNode: GraphNode,
): Promise<number> {
  let current: GraphNode | null = pseudoNode;
  let count = 0;
  const visited = new Set<string>();
  while (current) {
    if (visited.has(current.id)) {
      throw new Error(`Cycle detected while counting pseudocode revisions from ${pseudoNode.id}.`);
    }
    visited.add(current.id);
    if (current.type === "PseudocodeNode" && current.action_used === "revise_design") {
      count += 1;
    }
    if (!current.created_from) break;
    current = await store.readNode(current.created_from);
  }
  return count;
}

export async function assertCodeRepairBudgetAvailable(options: {
  store: GraphStore;
  codeNode: GraphNode;
  maxRepairs: number;
}): Promise<number> {
  assertNodeType(options.codeNode, "CodeNode");
  if (!Number.isInteger(options.maxRepairs) || options.maxRepairs < 0) {
    throw new Error(`maxRepairs must be a non-negative integer.`);
  }
  const attempts = await countCodeRepairAttemptsForCodeNode(options.store, options.codeNode);
  if (attempts >= options.maxRepairs) {
    throw new Error(
      `CodeNode ${options.codeNode.id} repair budget exhausted after ${attempts} attempt(s).`,
    );
  }
  return attempts;
}

export async function countCodeRepairAttemptsForCodeNode(
  store: GraphStore,
  codeNode: GraphNode,
): Promise<number> {
  assertNodeType(codeNode, "CodeNode");
  const edges = await store.listEdges();
  const resultNodeIds = new Set(
    edges
      .filter(
        (edge) => edge.from === codeNode.id && (edge.type === "checks" || edge.type === "audits"),
      )
      .map((edge) => edge.to),
  );
  const repairNodeIds = edges
    .filter((edge) => resultNodeIds.has(edge.from) && edge.type === "repairs")
    .map((edge) => edge.to);
  const repairNodes = await Promise.all(repairNodeIds.map(async (id) => store.readNode(id)));
  return repairNodes.filter(
    (node) => node.type === "CodeNode" && node.action_used === "repair_code",
  ).length;
}
