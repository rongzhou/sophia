import { designSolutionWithOllama } from "../../llm/tasks/design_solution.js";
import { reviseDesignWithOllama } from "../../llm/tasks/revise_design.js";
import type { PseudocodeCheckResult } from "../../pseudo/check.js";
import { createDesignedPseudocodeNode, createRevisedDesignNode } from "./llm_node.js";
import { captureRawLlmFailure } from "./llm_failure.js";
import type { GraphNode } from "../core/nodes.js";
import { createPseudocodeCheckNode } from "./pseudocode.js";
import type { GraphStore } from "../core/store.js";

export type DesignReviseLoopResult =
  | {
      kind: "designed";
      pseudoNode: GraphNode;
      revisionsUsed: number;
      steps: Array<Record<string, unknown>>;
    }
  | {
      kind:
        | "design_needs_clarification"
        | "design_revision_budget_exhausted"
        | "design_revision_needs_clarification";
      pseudoNode: GraphNode;
      revisionsUsed: number;
      steps: Array<Record<string, unknown>>;
    };

export async function runDesignReviseLoop(options: {
  store: GraphStore;
  goalNode: GraphNode;
  goal: string;
  model: string;
  maxRevisions: number;
  checkTags?: string[];
  onPseudocodeNode?: (node: GraphNode) => Promise<void>;
  onCheckResult?: (node: GraphNode, result: PseudocodeCheckResult) => Promise<void>;
}): Promise<DesignReviseLoopResult> {
  const steps: Array<Record<string, unknown>> = [];
  let revisionsUsed = 0;
  const design = await captureRawLlmFailure({
    store: options.store,
    createdFrom: options.goalNode,
    actionUsed: "design_solution",
    edgeType: "designs_solution",
    tags: ["llm", "design", "failed"],
    model: options.model,
    goal: options.goal,
    call: () =>
      designSolutionWithOllama({
        goal: options.goal,
        model: options.model,
      }),
  });
  let pseudoNode = await createDesignedPseudocodeNode({
    store: options.store,
    goalNode: options.goalNode,
    result: design,
    model: options.model,
    ...(design.output.status === "needs_clarification" ? { status: "failed" as const } : {}),
  });
  await options.onPseudocodeNode?.(pseudoNode);
  steps.push({ step: "design_solution", node: pseudoNode.id, status: design.output.status });
  if (design.output.status !== "designed") {
    return {
      kind: "design_needs_clarification",
      pseudoNode,
      revisionsUsed,
      steps,
    };
  }

  while (true) {
    const pseudocode = await options.store.readArtifact(pseudoNode, "content.pseudo");
    const check = await createPseudocodeCheckNode({
      store: options.store,
      pseudoNode,
      pseudocode,
      ...(options.checkTags ? { tags: options.checkTags } : {}),
    });
    await options.onCheckResult?.(check.node, check.result as PseudocodeCheckResult);
    steps.push({
      step: "pseudo_check",
      node: check.node.id,
      ok: check.result.ok,
      diagnostics: check.result.diagnostics.length,
    });
    if (check.result.ok) {
      return {
        kind: "designed",
        pseudoNode,
        revisionsUsed,
        steps,
      };
    }
    if (revisionsUsed >= options.maxRevisions) {
      return {
        kind: "design_revision_budget_exhausted",
        pseudoNode,
        revisionsUsed,
        steps,
      };
    }

    const revision = await captureRawLlmFailure({
      store: options.store,
      createdFrom: check.node,
      actionUsed: "revise_design",
      edgeType: "revises",
      tags: ["llm", "revise", "failed"],
      model: options.model,
      goal: pseudoNode.goal,
      call: () =>
        reviseDesignWithOllama({
          pseudocode,
          checkResult: check.result as PseudocodeCheckResult,
          model: options.model,
        }),
    });
    revisionsUsed += 1;
    pseudoNode = await createRevisedDesignNode({
      store: options.store,
      sourcePseudoNode: pseudoNode,
      checkNode: check.node,
      result: revision,
      model: options.model,
      ...(revision.output.status === "needs_clarification" ? { status: "failed" as const } : {}),
    });
    await options.onPseudocodeNode?.(pseudoNode);
    steps.push({
      step: "revise_design",
      node: pseudoNode.id,
      status: revision.output.status,
      revisions_used: revisionsUsed,
    });
    if (revision.output.status !== "revised") {
      return {
        kind: "design_revision_needs_clarification",
        pseudoNode,
        revisionsUsed,
        steps,
      };
    }
  }
}
