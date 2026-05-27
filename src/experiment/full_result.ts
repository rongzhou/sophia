import path from "node:path";
import type { GraphNode } from "../graph/nodes.js";
import type { BenchmarkTask } from "./task.js";
import type { BenchmarkVerificationResult } from "./verify.js";

export interface FullExperimentOptions {
  task: BenchmarkTask;
  model: string;
  maxDesignRevisions: number;
  maxRepairs: number;
}

export interface FullExperimentResult {
  ok: boolean;
  mode: "full";
  task_id: string;
  model: string;
  workspace: string;
  graph_dir: string;
  goal_node: string | null;
  pseudocode_node: string | null;
  code_node: string | null;
  repairs_used: number;
  design_revisions_used: number;
  failure_type: string | null;
  steps: Array<Record<string, unknown>>;
  verification: BenchmarkVerificationResult | null;
}

export function failedFullExperimentResult(options: {
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
