import { describe, expect, it } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import { createDesignedPseudocodeNode } from "../../src/graph/llm_node_workflow.js";
import { GraphStore } from "../../src/graph/store.js";
import {
  buildDesignSolutionPrompt,
  validateDesignSolutionOutput,
} from "../../src/llm/tasks/design_solution.js";
import {
  buildReviseDesignPrompt,
  ReviseDesignOutputSchema,
  validateReviseDesignOutput,
} from "../../src/llm/tasks/revise_design.js";

describe("solution design output", () => {
  it("builds a generic goal-to-pseudo prompt without target-language syntax", () => {
    const prompt = buildDesignSolutionPrompt("Given a count, return count doubled.");

    expect(prompt).toContain("algorithm design");
    expect(prompt).toContain("JSON structure is allowed");
    expect(prompt).toContain("algorithm pseudocode, not a custom programming syntax");
    expect(prompt).toContain("Do not write program code");
    expect(prompt).toContain("type annotations");
    expect(prompt).not.toContain("count: Int");
    expect(prompt).not.toContain("Console.Write");
    expect(prompt).not.toContain("implementation_hints");
    expect(prompt).not.toContain("Sophia");
    expect(prompt).not.toContain("Sophia-Core");
    expect(prompt).not.toContain("scaffold");
    expect(prompt).not.toContain("capability");
    expect(prompt).not.toContain("domain");
    expect(prompt).toContain("describe it as a semantic input");
    expect(prompt).toContain("preserve the value names exactly as public labels");
    expect(prompt).toContain("left followed by right");
    expect(prompt).toContain("preserve them as literal text values");
    expect(prompt).toContain('"algorithm"');
    expect(prompt).toContain("named logical steps");
    expect(prompt).toContain("needs_clarification");
    expect(prompt).toContain("Goal:");
    expect(prompt).not.toContain("action DoubleInput");
    expect(prompt).not.toContain("return count * 2");
    expect(prompt).not.toMatch(/make (the )?tests pass/i);
  });

  it("rejects generated pseudo that is actually program code", () => {
    expect(() =>
      validateDesignSolutionOutput({
        status: "designed",
        pseudocode: `
action DoubleInput {
  body {
    return count * 2
  }
}
`,
        notes: [],
        questions: [],
        self_check: {
          has_required_sections: true,
          no_program_code: true,
          no_hidden_expected_outputs: true,
          concrete_algorithm_steps: true,
        },
      }),
    ).toThrow("program-like top-level code");
  });

  it("accepts structured JSON algorithm pseudocode", () => {
    expect(() =>
      validateDesignSolutionOutput({
        status: "designed",
        pseudocode: JSON.stringify({
          purpose: "Coordinate reusable steps.",
          inputs: [{ name: "value", meaning: "integer value" }],
          outputs: [{ name: "result", meaning: "integer result" }],
          algorithm: [
            "Helper step ValidateValue: decide whether value is positive.",
            "Set result to the output of ValidateValue using value.",
            "Return result.",
          ],
        }),
        notes: [],
        questions: [],
        self_check: {
          has_required_sections: true,
          no_program_code: true,
          no_hidden_expected_outputs: true,
          concrete_algorithm_steps: true,
        },
      }),
    ).not.toThrow();
  });

  it("rejects generated pseudo that contains formal type or effect syntax", () => {
    expect(() =>
      validateDesignSolutionOutput({
        status: "designed",
        pseudocode: `
program FlowDemo {
  purpose { "Coordinate reusable steps." }
  inputs { value: Int }
  outputs { result := "integer result" }
  effects { Console.Write }
  algorithm { return value }
}
`,
        notes: [],
        questions: [],
        self_check: {
          has_required_sections: true,
          no_program_code: true,
          no_hidden_expected_outputs: true,
          concrete_algorithm_steps: true,
        },
      }),
    ).toThrow("program-like top-level code");
  });

  it("records real LLM pseudocode artifacts under the goal node", async () => {
    const root = await createTempDir("sophia-design-solution-");
    const store = new GraphStore(root);
    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      action_used: "start",
      goal: "Return unit without effects.",
      summary: "Return unit without effects.",
    });

    const pseudo = await createDesignedPseudocodeNode({
      store,
      goalNode: goal,
      model: "test-model",
      result: {
        prompt: "prompt",
        rawResponse: "response",
        output: {
          status: "designed",
          pseudocode: JSON.stringify({
            purpose: "Return unit.",
            inputs: [],
            outputs: [{ name: "result", meaning: "no returned value" }],
            algorithm: ["Return no value."],
          }),
          notes: ["test"],
          questions: [],
          self_check: {
            has_required_sections: true,
            no_program_code: true,
            no_hidden_expected_outputs: true,
            concrete_algorithm_steps: true,
          },
        },
      },
    });

    expect(pseudo).toMatchObject({
      type: "PseudocodeNode",
      action_used: "design_solution",
      created_from: goal.id,
      model: "test-model",
    });
    expect(await store.readArtifact(pseudo, "content.pseudo")).toContain('"purpose":"Return unit."');
    expect(await store.readArtifact(pseudo, "prompt.txt")).toBe("prompt");
    expect(await store.readArtifact(pseudo, "response.txt")).toBe("response");
    expect(await store.listEdges()).toEqual([
      { from: goal.id, to: pseudo.id, type: "designs_solution" },
    ]);
  });
});

describe("design revision output", () => {
  it("defaults omitted optional arrays to reduce JSON-format brittleness", () => {
    const parsed = ReviseDesignOutputSchema.parse({
      status: "revised",
      pseudocode:
        JSON.stringify({
          purpose: "x",
          inputs: [],
          outputs: [{ name: "result", meaning: "no returned value" }],
          algorithm: ["return no value"],
        }),
    });

    expect(parsed.notes).toEqual([]);
    expect(parsed.questions).toEqual([]);
  });

  it("rejects revised pseudo that introduces program-like syntax", () => {
    expect(() =>
      validateReviseDesignOutput({
        status: "revised",
        pseudocode:
          'program Demo { purpose { "x" } inputs { value: Int } outputs { result := "integer" } algorithm { return value } }',
        notes: [],
        questions: [],
      }),
    ).toThrow("program-like pseudocode syntax");
  });

  it("keeps design and revision prompts free of formal syntax examples", () => {
    const designPrompt = buildDesignSolutionPrompt("Return a text label for an optional input.");
    const revisePrompt = buildReviseDesignPrompt(
      `
program Demo {
  purpose { "Return a semantic label." }
  inputs { value := "optional text input" }
  outputs { result := "text label" }
  implementation_hints {
    domain: DemoDomain
  }
  algorithm { return label }
}
`,
      {
        ok: false,
        diagnostics: [],
        checks: {
          has_purpose: true,
          has_inputs: true,
          has_outputs: true,
          has_algorithm: true,
          has_expected: false,
          loop_details_explicit: true,
          state_updates_explicit: true,
          no_vague_steps: true,
        },
      },
    );

    for (const prompt of [designPrompt, revisePrompt]) {
      expect(prompt).not.toContain("Console.Write");
      expect(prompt).not.toContain("List<");
      expect(prompt).not.toContain("Optional<");
      expect(prompt).not.toContain("implementation_hints");
      expect(prompt).not.toContain("Sophia");
      expect(prompt).not.toContain("Sophia-Core");
      expect(prompt).not.toContain("scaffold");
      expect(prompt).not.toContain("capability");
      expect(prompt).not.toMatch(/\b[A-Za-z_]\w*\s*:\s*(?:Int|Text|Bool|Unit)\b/);
    }
  });
});
