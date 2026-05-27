import type { CheckResult } from "../lang/diagnostics.js";
import { isLlmCallError } from "../llm/errors.js";
import { repairCodeWithOllama } from "../llm/tasks/repair.js";
import type { ImplementationStructureOverride } from "../pseudo/structure_plan.js";
import {
  createArtifactDiffNode,
  createAuditNode,
  createCheckResultNode,
  readCodeNodeFiles,
} from "./code_workflow.js";
import { createRawLlmFailureNode, createRepairedCodeNode } from "./llm_node_workflow.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";

export type CheckRepairLoopResult =
  | {
      kind: "passed";
      codeNode: GraphNode;
      repairsUsed: number;
      diffOk: boolean;
      files: Record<string, string>;
      steps: Array<Record<string, unknown>>;
    }
  | {
      kind: "budget_exhausted";
      reason:
        | "repair_budget_exhausted"
        | "repair_budget_exhausted_after_audit"
        | "artifact_diff_failed";
      codeNode: GraphNode;
      repairsUsed: number;
      diffOk: boolean;
      steps: Array<Record<string, unknown>>;
    };

export async function runCheckRepairLoop(options: {
  store: GraphStore;
  initialCodeNode: GraphNode;
  pseudocode: string;
  model: string;
  maxRepairs: number;
  structureOverride?: ImplementationStructureOverride;
  repairFailureTags?: string[];
}): Promise<CheckRepairLoopResult> {
  let currentCodeNode = options.initialCodeNode;
  let repairsUsed = 0;
  let diffOk = true;
  const steps: Array<Record<string, unknown>> = [];

  while (true) {
    const files = await readCodeNodeFiles(options.store, currentCodeNode);
    const check = await createCheckResultNode({
      store: options.store,
      codeNode: currentCodeNode,
      files,
    });
    steps.push({
      step: "check",
      node: check.node.id,
      code_node: currentCodeNode.id,
      ok: check.result.ok,
      diagnostics: check.result.diagnostics.length,
    });

    if (!check.result.ok) {
      const repaired = await repairOrStop({
        ...options,
        currentCodeNode,
        resultNode: check.node,
        files,
        checkResult: check.result,
        repairsUsed,
        diffOk,
        reason: "repair_budget_exhausted",
        repairStep: "repair_after_check",
      });
      if (repaired.kind === "budget_exhausted") {
        return { ...repaired, steps };
      }
      currentCodeNode = repaired.codeNode;
      repairsUsed = repaired.repairsUsed;
      diffOk = repaired.diffOk;
      steps.push(repaired.step);
      continue;
    }

    const audit = await createAuditNode({ store: options.store, codeNode: currentCodeNode, files });
    steps.push({
      step: "audit",
      node: audit.node.id,
      code_node: currentCodeNode.id,
      ok: audit.result.ok,
      diagnostics: audit.result.diagnostics.length,
    });

    if (!audit.result.ok) {
      const repaired = await repairOrStop({
        ...options,
        currentCodeNode,
        resultNode: audit.node,
        files,
        checkResult: audit.result,
        repairsUsed,
        diffOk,
        reason: "repair_budget_exhausted_after_audit",
        repairStep: "repair_after_audit",
      });
      if (repaired.kind === "budget_exhausted") {
        return { ...repaired, steps };
      }
      currentCodeNode = repaired.codeNode;
      repairsUsed = repaired.repairsUsed;
      diffOk = repaired.diffOk;
      steps.push(repaired.step);
      continue;
    }

    return {
      kind: "passed",
      codeNode: currentCodeNode,
      repairsUsed,
      diffOk,
      files,
      steps,
    };
  }
}

async function repairOrStop(options: {
  store: GraphStore;
  currentCodeNode: GraphNode;
  resultNode: GraphNode;
  files: Record<string, string>;
  checkResult: CheckResult;
  pseudocode: string;
  model: string;
  repairsUsed: number;
  maxRepairs: number;
  structureOverride?: ImplementationStructureOverride;
  diffOk: boolean;
  reason: "repair_budget_exhausted" | "repair_budget_exhausted_after_audit";
  repairStep: "repair_after_check" | "repair_after_audit";
  repairFailureTags?: string[];
}): Promise<
  | {
      kind: "repaired";
      codeNode: GraphNode;
      repairsUsed: number;
      diffOk: boolean;
      step: Record<string, unknown>;
    }
  | {
      kind: "budget_exhausted";
      reason:
        | "repair_budget_exhausted"
        | "repair_budget_exhausted_after_audit"
        | "artifact_diff_failed";
      codeNode: GraphNode;
      repairsUsed: number;
      diffOk: boolean;
    }
> {
  if (options.repairsUsed >= options.maxRepairs) {
    return {
      kind: "budget_exhausted",
      reason: options.reason,
      codeNode: options.currentCodeNode,
      repairsUsed: options.repairsUsed,
      diffOk: options.diffOk,
    };
  }

  try {
    const repair = await repairCodeWithOllama({
      files: options.files,
      checkResult: options.checkResult,
      model: options.model,
      pseudocode: options.pseudocode,
      ...(options.structureOverride ? { structureOverride: options.structureOverride } : {}),
    });
    const repairedNode = await createRepairedCodeNode({
      store: options.store,
      sourceCodeNode: options.currentCodeNode,
      checkNode: options.resultNode,
      result: repair,
      model: options.model,
    });
    const diff = await createArtifactDiffNode({
      store: options.store,
      beforeNode: options.currentCodeNode,
      afterNode: repairedNode,
      beforeFiles: options.files,
      afterFiles: repair.output.files,
    });
    const repairsUsed = options.repairsUsed + 1;
    if (!diff.result.ok) {
      return {
        kind: "budget_exhausted",
        reason: "artifact_diff_failed",
        codeNode: repairedNode,
        repairsUsed,
        diffOk: false,
      };
    }
    return {
      kind: "repaired",
      codeNode: repairedNode,
      repairsUsed,
      diffOk: diff.result.ok,
      step: {
        step: options.repairStep,
        node: repairedNode.id,
        code_node: repairedNode.id,
        repairs_used: repairsUsed,
        diff_node: diff.node.id,
        diff_ok: diff.result.ok,
      },
    };
  } catch (error) {
    if (isLlmCallError(error)) {
      await createRawLlmFailureNode({
        store: options.store,
        createdFrom: options.resultNode,
        action_used: "repair_code",
        edgeType: "repairs",
        tags: options.repairFailureTags ?? ["llm", "repair", "failed"],
        model: options.model,
        error,
        ...(options.currentCodeNode.goal ? { goal: options.currentCodeNode.goal } : {}),
      });
    }
    throw error;
  }
}
