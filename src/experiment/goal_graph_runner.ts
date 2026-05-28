import { writeFile } from "node:fs/promises";
import path from "node:path";
import { buildDecisionActionBaseline } from "../graph/decision/baseline.js";
import {
  type GoalGraphScenario,
  materializeGoalGraphScenario,
} from "../graph/goal/scenarios.js";
import { buildGraphReport } from "../graph/core/report.js";
import { GraphStore } from "../graph/core/store.js";
import { createRunDirectory, ensureRunStageDirectories } from "../workspace/fs_layout.js";
import { sophiaTomlTemplate } from "../workspace/workspace.js";
import { writeJsonFile } from "../util/fs.js";
import type { GoalGraphExperimentResult } from "./result.js";

export interface GoalGraphExperimentOptions {
  scenario: GoalGraphScenario;
  model: string;
}

export async function runGoalGraphExperiment(
  options: GoalGraphExperimentOptions,
): Promise<GoalGraphExperimentResult> {
  const workspace = await createRunDirectory(process.cwd(), options.scenario.id);
  await ensureRunStageDirectories(workspace);
  await writeFile(
    path.join(workspace, "sophia.toml"),
    `${sophiaTomlTemplate(`goal-graph-${options.scenario.id}`)}\n`,
    "utf8",
  );

  const store = new GraphStore(workspace, "graph");
  const materialized = await materializeGoalGraphScenario({ store, scenario: options.scenario });
  const nodes = await store.listNodes();
  const edges = await store.listEdges();
  const report = await buildGraphReport(store, nodes, edges);
  const baselineDecisions = await buildBaselineDecisionSummaries(store);
  const actionPath = extractActionPath(options.scenario.records.llm_response);
  const decompositionVersions = [
    ...new Set(
      materialized.active_context.objectives
        .map((objective) => objective.decomposition_id)
        .filter((value): value is string => value !== null),
    ),
  ].sort();
  const invalidatedBranches = [...materialized.active_context.excluded.objectives].sort();
  const acceptedChanges = materialized.active_context.accepted_changes
    .map((change) => change.node_id)
    .sort();

  await writeJsonFile(path.join(workspace, "goal/scenario.json"), options.scenario);
  await writeJsonFile(path.join(workspace, "goal/materialized_record.json"), materialized);
  await writeJsonFile(path.join(workspace, "graph/report.json"), report);
  await writeJsonFile(path.join(workspace, "graph/baseline_decisions.json"), baselineDecisions);

  const goalWorkflowMetrics = report.goal_workflow.metrics;
  return {
    ok: options.scenario.records.final_verification.verify_ok,
    mode: "goal-graph",
    task_id: options.scenario.id,
    model: options.model,
    workspace,
    graph_dir: path.join(workspace, "graph"),
    repairs_used: 0,
    design_revisions_used: 0,
    failure_type: options.scenario.records.final_verification.verify_ok
      ? null
      : "goal_graph_verification_failed",
    steps: [
      { step: "materialize_goal_graph_scenario", nodes: nodes.length, edges: edges.length },
      {
        step: "build_goal_context",
        active_objectives: materialized.active_context.objectives.length,
        accepted_changes: materialized.active_context.accepted_changes.length,
      },
      { step: "build_baseline_decisions", decisions: baselineDecisions.length },
      {
        step: "goal_graph_verify",
        ok: options.scenario.records.final_verification.verify_ok,
      },
    ],
    verification: null,
    scenario_record: options.scenario.records,
    action_path: actionPath,
    decomposition_versions: decompositionVersions,
    invalidated_branches: invalidatedBranches,
    accepted_changes: acceptedChanges,
    baseline_decisions: baselineDecisions,
    goal_graph_metrics: {
      active_objectives: materialized.active_context.objectives.length,
      excluded_objectives: materialized.active_context.excluded.objectives.length,
      active_milestone: materialized.active_context.active_milestone?.node_id ?? null,
      accepted_changes: materialized.active_context.accepted_changes.length,
      regression_constraints: materialized.active_context.regression_constraints.length,
      invalidated_decompositions: goalWorkflowMetrics.invalidated_decompositions,
      abandoned_branches: goalWorkflowMetrics.abandoned_branches,
    },
    comparison: {
      fixed_full_workflow: {
        mode: "full",
        applicable: false,
        reason:
          "Goal-graph scenarios validate staged objective state and change context, not a single hidden-case executable task.",
      },
      deterministic_decision_baseline: {
        mode: "deterministic-baseline",
        decisions: baselineDecisions.length,
      },
      llm_goal_graph_decision: {
        mode: "goal-graph",
        recorded_actions: actionPath.length,
      },
    },
  };
}

async function buildBaselineDecisionSummaries(store: GraphStore) {
  const nodes = await store.listNodes();
  const summaries = [];
  for (const node of nodes) {
    const decision = await buildDecisionActionBaseline(store, node);
    summaries.push({
      node: node.id,
      type: node.type,
      selected_action: decision.selected_action,
      candidate_actions: decision.candidate_actions.map((candidate) => candidate.action),
    });
  }
  return summaries;
}

function extractActionPath(llmResponse: unknown): string[] {
  if (
    typeof llmResponse === "object" &&
    llmResponse !== null &&
    "selected_path" in llmResponse &&
    Array.isArray((llmResponse as { selected_path?: unknown }).selected_path)
  ) {
    return (llmResponse as { selected_path: unknown[] }).selected_path.filter(
      (item): item is string => typeof item === "string",
    );
  }
  return [];
}
