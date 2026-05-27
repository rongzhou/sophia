import { afterEach, describe, expect, it, vi } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import { buildDecisionActionBaseline } from "../../src/graph/decision_baseline.js";
import {
  buildLlmDecisionPrompt,
  decideNextActionWithOllama,
} from "../../src/graph/llm_decision.js";
import { GraphStore } from "../../src/graph/store.js";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("LLM node decision", () => {
  it("builds a generic decision prompt without writing artifacts for the LLM", async () => {
    const store = await tempStore();
    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      action_used: "start",
      goal: "Given an input count, return count doubled.",
      summary: "Given an input count, return count doubled.",
    });
    const baseline = await buildDecisionActionBaseline(store, goal);

    const prompt = await buildLlmDecisionPrompt({
      store,
      currentNode: goal,
      nodes: await store.listNodes(),
      edges: await store.listEdges(),
      baseline,
    });

    expect(prompt).toContain("Your only job is heuristic node decision");
    expect(prompt).toContain("Choose exactly one next action from the allowed action list");
    expect(prompt).toContain("Decision scaffold");
    expect(prompt).toContain("Focused graph context");
    expect(prompt).toContain("graph design <goal-node> --model <model>");
    expect(prompt).toContain("Do not write pseudocode");
    expect(prompt).toContain("do not write Sophia code");
    expect(prompt).toContain("Do not infer missing task logic");
    expect(prompt).toContain("Candidate action reasons should describe workflow state");
    expect(prompt).toContain("do not reveal or assume hidden fixture answers");
    expect(prompt).not.toContain("action Demo");
    expect(prompt).not.toContain("expected_outputs.json");
  });

  it("parses and validates an Ollama decision against allowed actions", async () => {
    const store = await tempStore();
    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      action_used: "start",
      goal: "Return a label for an input count.",
      summary: "Return a label for an input count.",
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => ({
          model: "test-model",
          response: JSON.stringify({
            current_node: goal.id,
            state_assessment: {
              goal_size: "small",
              logic_clarity: "low",
              has_pseudocode: false,
              has_code: false,
              compile_status: "not_checked",
              error_type: "none",
              repair_attempts: 0,
              decomposition_needed: false,
            },
            candidate_actions: [
              {
                action: "design_solution",
                score: 0.86,
                reason: "The goal has no pseudocode node yet.",
              },
            ],
            selected_action: "design_solution",
            confidence: 0.86,
            rationale: "Start by designing pseudocode.",
            self_check: {
              selected_action_is_allowed: true,
              based_only_on_visible_graph_state: true,
              no_pseudocode_or_code_generated: true,
              no_hidden_answers_or_fixture_outputs: true,
            },
          }),
        }),
      })),
    );

    const result = await decideNextActionWithOllama({
      store,
      currentNode: goal,
      model: "test-model",
    });

    expect(result.decision.selected_action).toBe("design_solution");
    expect(result.baseline.selected_action).toBe("design_solution");
    expect(result.prompt).toContain("Allowed actions");
  });

  it("rejects decisions that fail the anti-leak self-check", async () => {
    const store = await tempStore();
    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      action_used: "start",
      goal: "Return a label for an input count.",
      summary: "Return a label for an input count.",
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => ({
          model: "test-model",
          response: JSON.stringify({
            current_node: goal.id,
            state_assessment: {
              goal_size: "small",
              logic_clarity: "low",
              has_pseudocode: false,
              has_code: false,
              compile_status: "not_checked",
              error_type: "none",
              repair_attempts: 0,
              decomposition_needed: false,
            },
            candidate_actions: [
              {
                action: "design_solution",
                score: 0.86,
                reason: "The goal has no pseudocode node yet.",
              },
            ],
            selected_action: "design_solution",
            confidence: 0.86,
            rationale: "Start by designing pseudocode.",
            self_check: {
              selected_action_is_allowed: true,
              based_only_on_visible_graph_state: true,
              no_pseudocode_or_code_generated: false,
              no_hidden_answers_or_fixture_outputs: true,
            },
          }),
        }),
      })),
    );

    await expect(
      decideNextActionWithOllama({
        store,
        currentNode: goal,
        model: "test-model",
      }),
    ).rejects.toThrow("self_check failed");
  });
});

async function tempStore(): Promise<GraphStore> {
  return new GraphStore(await createTempDir("sophia-llm-decision-"));
}
