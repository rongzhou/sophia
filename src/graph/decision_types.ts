import { z } from "zod";

export type DecisionAction =
  | "design_solution"
  | "decompose"
  | "pseudo_check"
  | "revise_design"
  | "implement_design"
  | "check_code"
  | "audit_code"
  | "repair_code"
  | "select"
  | "materialize_code"
  | "complete";

export const DecisionActionSchema = z.enum([
  "design_solution",
  "decompose",
  "pseudo_check",
  "revise_design",
  "implement_design",
  "check_code",
  "audit_code",
  "repair_code",
  "select",
  "materialize_code",
  "complete",
]);

export const GraphDecisionSchema = z
  .object({
    current_node: z.string().regex(/^N\d{4,}$/),
    state_assessment: z
      .object({
        goal_size: z.enum(["tiny", "small", "medium", "large"]),
        logic_clarity: z.enum(["low", "medium", "high"]),
        has_pseudocode: z.boolean(),
        has_code: z.boolean(),
        compile_status: z.enum(["not_checked", "pass", "fail"]),
        error_type: z.enum(["none", "local", "conceptual", "integration"]),
        repair_attempts: z.number().int().nonnegative(),
        decomposition_needed: z.boolean(),
      })
      .strict(),
    candidate_actions: z.array(
      z
        .object({
          action: DecisionActionSchema,
          score: z.number(),
          reason: z.string(),
        })
        .strict(),
    ),
    selected_action: DecisionActionSchema,
    confidence: z.number(),
  })
  .strict();

export interface StateAssessment {
  goal_size: "tiny" | "small" | "medium" | "large";
  logic_clarity: "low" | "medium" | "high";
  has_pseudocode: boolean;
  has_code: boolean;
  compile_status: "not_checked" | "pass" | "fail";
  error_type: "none" | "local" | "conceptual" | "integration";
  repair_attempts: number;
  decomposition_needed: boolean;
}

export interface CandidateAction {
  action: DecisionAction;
  score: number;
  reason: string;
}

export interface GraphDecision {
  current_node: string;
  state_assessment: StateAssessment;
  candidate_actions: CandidateAction[];
  selected_action: DecisionAction;
  confidence: number;
}
