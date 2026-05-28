import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { sophiaTomlTemplate } from "../workspace/workspace.js";
import { createRunDirectory, ensureRunStageDirectories } from "../workspace/fs_layout.js";
import { runCheckRepairLoop } from "../graph/workflow/check_repair_loop.js";
import { runDesignReviseLoop } from "../graph/workflow/design_revise_loop.js";
import { GraphStore } from "../graph/core/store.js";
import { createImplementedCodeNode } from "../graph/workflow/llm_node.js";
import type { GraphNode } from "../graph/core/nodes.js";
import { isLlmCallError } from "../llm/errors.js";
import { implementDesignWithOllama } from "../llm/tasks/implement_design.js";
import { captureRawLlmFailure } from "../graph/workflow/llm_failure.js";
import { verifySophiaFilesAgainstTask } from "./verify.js";
import type { FullExperimentResult } from "./result.js";
import { buildImplementationStructurePlan } from "../pseudo/structure_plan.js";
import { writeJsonFile } from "../util/fs.js";
import type { BenchmarkTask } from "./task.js";
import { buildPublicGoalForTask } from "./public_goal.js";

export interface FullExperimentOptions {
  task: BenchmarkTask;
  model: string;
  maxDesignRevisions: number;
  maxRepairs: number;
}

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
    actionUsed: "start",
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
    const designLoop = await runDesignReviseLoop({
      store,
      goalNode,
      goal: publicGoal,
      model: options.model,
      maxRevisions: options.maxDesignRevisions,
      checkTags: ["pseudo", "check", "benchmark"],
      onPseudocodeNode: async (node) => {
        await copyPseudocodeArtifact(store, node, workspace);
      },
      onCheckResult: async (_node, result) => {
        await writeJsonFile(path.join(workspace, "pseudo/check.json"), result);
      },
    });
    pseudoNode = designLoop.pseudoNode;
    designRevisionsUsed = designLoop.revisionsUsed;
    steps.push(...designLoop.steps);
    if (designLoop.kind !== "designed") {
      return failedFullExperimentResult({
        options,
        workspace,
        goalNode,
        pseudoNode,
        codeNode: null,
        steps,
        failureType: designLoop.kind,
        designRevisionsUsed,
        repairsUsed: 0,
      });
    }
  } catch (error) {
    if (isLlmCallError(error)) {
      steps.push({ step: "llm_error", message: error.message });
      await writeLlmFailureArtifact(workspace, "design", error);
    }
    return failedFullExperimentResult({
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
    const implementation = await captureRawLlmFailure({
      store,
      createdFrom: pseudoNode,
      actionUsed: "implement_design",
      edgeType: "implements_design",
      tags: ["llm", "implementation", "failed", "experiment"],
      model: options.model,
      goal: publicGoal,
      call: () =>
        implementDesignWithOllama({
          pseudocode,
          model: options.model,
          structureOverride,
        }),
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
      return failedFullExperimentResult({
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
    await writeJsonFile(path.join(workspace, "executable/verify.json"), verification);
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
    return failedFullExperimentResult({
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

async function writeLlmFailureArtifact(
  workspace: string,
  stage: "design" | "implementation",
  error: { message: string; prompt: string; rawResponse: string },
): Promise<void> {
  await writeJsonFile(path.join(workspace, `llm/${stage}_failure.json`), {
    message: error.message,
    prompt: error.prompt,
    raw_response: error.rawResponse,
  });
}

function failedFullExperimentResult(options: {
  options: FullExperimentOptions;
  workspace: string;
  goalNode: GraphNode | null;
  pseudoNode: GraphNode | null;
  codeNode: GraphNode | null;
  steps: Array<Record<string, unknown>>;
  failureType: string;
  designRevisionsUsed: number;
  repairsUsed: number;
}): FullExperimentResult {
  return {
    ok: false,
    mode: "full",
    task_id: options.options.task.id,
    model: options.options.model,
    workspace: options.workspace,
    graph_dir: path.join(options.workspace, "graph"),
    goal_node: options.goalNode?.id ?? null,
    pseudocode_node: options.pseudoNode?.id ?? null,
    code_node: options.codeNode?.id ?? null,
    repairs_used: options.repairsUsed,
    design_revisions_used: options.designRevisionsUsed,
    failure_type: options.failureType,
    steps: options.steps,
    verification: null,
  };
}
