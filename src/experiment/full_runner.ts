import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { sophiaTomlTemplate } from "../workspace/workspace.js";
import { createRunDirectory, ensureRunStageDirectories } from "../workspace/fs_layout.js";
import { runCheckRepairLoop } from "../graph/check_repair_loop.js";
import { GraphStore } from "../graph/store.js";
import {
  createImplementedCodeNode,
  createRevisedDesignNode,
  createDesignedPseudocodeNode,
} from "../graph/llm_node_workflow.js";
import type { GraphNode } from "../graph/nodes.js";
import { createPseudocodeCheckNode } from "../graph/pseudocode_workflow.js";
import { isLlmCallError } from "../llm/errors.js";
import { implementDesignWithOllama } from "../llm/tasks/implement_design.js";
import { reviseDesignWithOllama } from "../llm/tasks/revise_design.js";
import { designSolutionWithOllama } from "../llm/tasks/design_solution.js";
import { verifySophiaFilesAgainstTask } from "./verify.js";
import type { PseudocodeCheckResult } from "../pseudo/check.js";
import { buildImplementationStructurePlan } from "../pseudo/structure_plan.js";
import {
  failedFullExperimentResult as failedResult,
  type FullExperimentOptions,
  type FullExperimentResult,
} from "./full_result.js";
import { buildPublicGoalForTask } from "./public_goal.js";

export async function runFullExperiment(
  options: FullExperimentOptions,
): Promise<FullExperimentResult> {
  const workspace = await createRunDirectory(process.cwd(), options.task.id);
  await ensureRunStageDirectories(workspace);
  await writeFile(
    path.join(workspace, "sophia.toml"),
    `${sophiaTomlTemplate(`experiment-${options.task.id}`)}\n`,
    "utf8",
  );

  const store = new GraphStore(workspace, "graph");
  await store.init();
  const steps: Array<Record<string, unknown>> = [];
  const publicGoal = buildPublicGoalForTask(options.task);
  const goalNode = await store.createNode({
    type: "GoalNode",
    createdFrom: null,
    action_used: "start",
    goal: publicGoal,
    summary: options.task.title,
    artifacts: ["content.md"],
    tags: ["goal", "benchmark", options.task.id],
  });
  await store.writeArtifact(goalNode, "content.md", `${publicGoal}\n`);
  await writeFile(path.join(workspace, "goal", "content.md"), `${publicGoal}\n`, "utf8");
  steps.push({ step: "start", node: goalNode.id });

  let pseudoNode: GraphNode | null = null;
  let designRevisionsUsed = 0;
  try {
    const write = await designSolutionWithOllama({
      goal: publicGoal,
      model: options.model,
    });
    pseudoNode = await createDesignedPseudocodeNode({
      store,
      goalNode,
      result: write,
      model: options.model,
      ...(write.output.status === "needs_clarification" ? { status: "failed" as const } : {}),
    });
    await copyPseudocodeArtifact(store, pseudoNode, workspace);
    steps.push({ step: "design_solution", node: pseudoNode.id, status: write.output.status });
    if (write.output.status !== "designed") {
      return failedResult({
        options,
        workspace,
        goalNode,
        pseudoNode,
        codeNode: null,
        steps,
        failureType: "design_needs_clarification",
        designRevisionsUsed,
        repairsUsed: 0,
      });
    }

    while (true) {
      const pseudocode = await store.readArtifact(pseudoNode, "content.pseudo");
      const check = await createPseudocodeCheckNode({
        store,
        pseudoNode,
        pseudocode,
        tags: ["pseudo", "check", "benchmark"],
      });
      await writeJsonArtifact(workspace, "pseudo/check.json", check.result);
      steps.push({
        step: "pseudo_check",
        node: check.node.id,
        ok: check.result.ok,
        diagnostics: check.result.diagnostics.length,
      });
      if (check.result.ok) break;
      if (designRevisionsUsed >= options.maxDesignRevisions) {
        return failedResult({
          options,
          workspace,
          goalNode,
          pseudoNode,
          codeNode: null,
          steps,
          failureType: "design_revision_budget_exhausted",
          designRevisionsUsed,
          repairsUsed: 0,
        });
      }
      const revision = await reviseDesignWithOllama({
        pseudocode,
        checkResult: check.result as PseudocodeCheckResult,
        model: options.model,
      });
      designRevisionsUsed += 1;
      pseudoNode = await createRevisedDesignNode({
        store,
        sourcePseudoNode: pseudoNode,
        checkNode: check.node,
        result: revision,
        model: options.model,
        ...(revision.output.status === "needs_clarification" ? { status: "failed" as const } : {}),
      });
      await copyPseudocodeArtifact(store, pseudoNode, workspace);
      steps.push({
        step: "revise_design",
        node: pseudoNode.id,
        status: revision.output.status,
        revisions_used: designRevisionsUsed,
      });
      if (revision.output.status !== "revised") {
        return failedResult({
          options,
          workspace,
          goalNode,
          pseudoNode,
          codeNode: null,
          steps,
          failureType: "design_revision_needs_clarification",
          designRevisionsUsed,
          repairsUsed: 0,
        });
      }
    }
  } catch (error) {
    if (isLlmCallError(error)) {
      steps.push({ step: "llm_error", message: error.message });
      await writeLlmFailureArtifact(workspace, "design", error);
    }
    return failedResult({
      options,
      workspace,
      goalNode,
      pseudoNode,
      codeNode: null,
      steps,
      failureType: "design_or_revise_failed",
      designRevisionsUsed,
      repairsUsed: 0,
    });
  }

  let codeNode: GraphNode | null = null;
  let repairsUsed = 0;
  try {
    const pseudocode = await store.readArtifact(pseudoNode, "content.pseudo");
    const structureOverride = options.task.scaffold;
    const mainAction = buildImplementationStructurePlan(pseudocode, structureOverride).symbols
      .action;
    const implementation = await implementDesignWithOllama({
      pseudocode,
      model: options.model,
      structureOverride,
    });
    codeNode = await createImplementedCodeNode({
      store,
      pseudoNode,
      result: implementation,
      model: options.model,
    });
    steps.push({ step: "implement", node: codeNode.id });

    const loop = await runCheckRepairLoop({
      store,
      initialCodeNode: codeNode,
      pseudocode,
      model: options.model,
      maxRepairs: options.maxRepairs,
      structureOverride,
      repairFailureTags: ["llm", "repair", "failed", "experiment"],
    });
    steps.push(...loop.steps);
    repairsUsed = loop.repairsUsed;
    codeNode = loop.codeNode;

    if (loop.kind === "budget_exhausted") {
      return failedResult({
        options,
        workspace,
        goalNode,
        pseudoNode,
        codeNode,
        steps,
        failureType:
          loop.reason === "artifact_diff_failed"
            ? "artifact_diff_failed"
            : loop.reason === "repair_budget_exhausted"
              ? "code_repair_budget_exhausted"
              : "audit_repair_budget_exhausted",
        designRevisionsUsed,
        repairsUsed,
      });
    }

    await writeCandidateSophiaFiles(workspace, loop.files);
    const verification = await verifySophiaFilesAgainstTask(loop.files, options.task, {
      action: mainAction,
      scratchRoot: workspace,
    });
    await writeJsonArtifact(workspace, "executable/verify.json", verification);
    steps.push({
      step: "hidden_verify",
      ok: verification.ok,
      cases: verification.cases.length,
    });
    return {
      ok: verification.ok,
      mode: "full",
      task_id: options.task.id,
      model: options.model,
      workspace,
      graph_dir: path.join(workspace, "graph"),
      goal_node: goalNode.id,
      pseudocode_node: pseudoNode.id,
      code_node: codeNode.id,
      repairs_used: repairsUsed,
      design_revisions_used: designRevisionsUsed,
      failure_type: verification.ok ? null : "hidden_verification_failed",
      steps,
      verification,
    };
  } catch (error) {
    if (isLlmCallError(error)) {
      steps.push({ step: "llm_error", message: error.message });
      await writeLlmFailureArtifact(workspace, "implementation", error);
    }
    return failedResult({
      options,
      workspace,
      goalNode,
      pseudoNode,
      codeNode,
      steps,
      failureType: "implement_check_audit_or_repair_failed",
      designRevisionsUsed,
      repairsUsed,
    });
  }
}

async function copyPseudocodeArtifact(
  store: GraphStore,
  pseudoNode: GraphNode,
  workspace: string,
): Promise<void> {
  const pseudocode = await store.readArtifact(pseudoNode, "content.pseudo");
  await writeFile(path.join(workspace, "pseudo", "content.pseudo"), pseudocode, "utf8");
}

async function writeCandidateSophiaFiles(
  workspace: string,
  files: Record<string, string>,
): Promise<void> {
  for (const [filePath, content] of Object.entries(files)) {
    const relative = filePath.replace(/^domains\//, "");
    const target = path.join(workspace, "sophia", "candidate", "domains", relative);
    await mkdir(path.dirname(target), { recursive: true });
    await writeFile(target, content, "utf8");
  }
}

async function writeJsonArtifact(
  workspace: string,
  relativePath: string,
  value: unknown,
): Promise<void> {
  const target = path.join(workspace, relativePath);
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

async function writeLlmFailureArtifact(
  workspace: string,
  stage: "design" | "implementation",
  error: { message: string; prompt: string; rawResponse: string },
): Promise<void> {
  await writeJsonArtifact(workspace, `llm/${stage}_failure.json`, {
    message: error.message,
    prompt: error.prompt,
    raw_response: error.rawResponse,
  });
}
