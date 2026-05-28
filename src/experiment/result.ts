import type { BenchmarkVerificationResult } from "./verify.js";
import type { DirectTsVerificationResult } from "./direct_ts_runner.js";
import type { GoalGraphScenario } from "../graph/goal/scenarios.js";

export interface BaseExperimentResult<TMode extends string, TVerification> {
  ok: boolean;
  mode: TMode;
  task_id: string;
  model: string;
  workspace: string;
  graph_dir: string | null;
  repairs_used: number;
  design_revisions_used: number;
  failure_type: string | null;
  steps: Array<Record<string, unknown>>;
  verification: TVerification;
}

export interface FullExperimentResult
  extends BaseExperimentResult<"full", BenchmarkVerificationResult | null> {
  graph_dir: string;
  goal_node: string | null;
  pseudocode_node: string | null;
  code_node: string | null;
}

export interface DirectTsExperimentResult
  extends BaseExperimentResult<"direct-ts", DirectTsVerificationResult | null> {
  graph_dir: null;
  goal_node: null;
  pseudocode_node: null;
  code_node: null;
  repairs_used: 0;
  design_revisions_used: 0;
}

export interface GoalGraphExperimentResult extends BaseExperimentResult<"goal-graph", null> {
  graph_dir: string;
  repairs_used: 0;
  design_revisions_used: 0;
  scenario_record: {
    prompt: string;
    llm_response: unknown;
    final_verification: GoalGraphScenario["records"]["final_verification"];
  };
  action_path: string[];
  decomposition_versions: string[];
  invalidated_branches: string[];
  accepted_changes: string[];
  baseline_decisions: Array<{
    node: string;
    type: string;
    selected_action: string;
    candidate_actions: string[];
  }>;
  goal_graph_metrics: {
    active_objectives: number;
    excluded_objectives: number;
    active_milestone: string | null;
    accepted_changes: number;
    regression_constraints: number;
    invalidated_decompositions: number;
    abandoned_branches: number;
  };
  comparison: {
    fixed_full_workflow: {
      mode: "full";
      applicable: false;
      reason: string;
    };
    deterministic_decision_baseline: {
      mode: "deterministic-baseline";
      decisions: number;
    };
    llm_goal_graph_decision: {
      mode: "goal-graph";
      recorded_actions: number;
    };
  };
}

export type ExperimentResult =
  | FullExperimentResult
  | DirectTsExperimentResult
  | GoalGraphExperimentResult;
