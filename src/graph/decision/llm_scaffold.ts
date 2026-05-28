import type { DecisionAction, GraphDecisionPayload } from "./types.js";
import { PROMPT_PATHS, loadPromptData } from "../../llm/prompt_templates.js";

export interface DecisionScaffoldAction {
  action: DecisionAction;
  score: number;
  baseline_reason: string;
  preconditions: string[];
  executor: "graph apply" | "explicit command";
  executor_command: string;
  produces: string;
  llm_must_not: string[];
}

type DecisionScaffoldTemplate = Omit<
  DecisionScaffoldAction,
  "action" | "score" | "baseline_reason"
>;

const ACTION_SCAFFOLD = loadPromptData<Record<DecisionAction, DecisionScaffoldTemplate>>(
  PROMPT_PATHS.decision.decisionScaffold,
);

export function buildDecisionScaffold(baseline: GraphDecisionPayload): DecisionScaffoldAction[] {
  return baseline.candidate_actions.map((candidate) => ({
    action: candidate.action,
    score: candidate.score,
    baseline_reason: candidate.reason,
    ...ACTION_SCAFFOLD[candidate.action],
  }));
}
