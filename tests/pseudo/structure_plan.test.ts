import { describe, expect, it } from "vitest";
import {
  buildImplementationStructurePlan,
  pseudocodeForImplementationPrompt,
} from "../../src/pseudo/structure_plan.js";
import { buildImplementDesignPrompt } from "../../src/llm/tasks/implement_design.js";
import { samplePseudocodeJson } from "../helpers/sophia_workspace.js";

describe("pseudocodeForImplementationPrompt", () => {
  it("redacts expected outputs and validation sequence details before implementation", () => {
    const sanitized = pseudocodeForImplementationPrompt(
      samplePseudocodeJson({
        program_name: "Demo",
        purpose: "Build a list.",
        outputs: [{ name: "numbers", meaning: "list" }],
        algorithm: [
          "create empty list numbers",
          "repeat 3 times, append current to numbers",
          "return numbers",
        ],
        constraints: [
          'The sequence must be "2, 4, 6".',
          "Do not hardcode the full list.",
        ],
        expected: { result: "[2, 4, 6]" },
      }),
    );

    expect(sanitized).toContain('"expected": "<redacted for implementation');
    expect(sanitized).toContain("Do not hardcode the full list.");
    expect(sanitized).not.toContain("[2, 4, 6]");
    expect(sanitized).not.toContain('The sequence must be "2, 4, 6".');
  });

  it("keeps algorithm literals that are required behavior, not validation answers", () => {
    const prompt = buildImplementDesignPrompt(
      samplePseudocodeJson({
        program_name: "Hello",
        purpose: "Print a greeting.",
        outputs: [{ name: "result", meaning: "unit" }],
        algorithm: ['print "Hello, Sophia!"', "return unit"],
        expected: { stdout: "Hello, Sophia!\n", result: "unit" },
      }),
    );

    expect(prompt).toContain('print \\"Hello, Sophia!\\"');
    expect(prompt).not.toContain("stdout");
  });

  it("redacts exact validation constraints while preserving branch behavior", () => {
    const prompt = buildImplementDesignPrompt(
      samplePseudocodeJson({
        program_name: "Label",
        purpose: "Return a label.",
        inputs: [{ name: "count", meaning: "integer" }],
        outputs: [{ name: "label", meaning: "text" }],
        algorithm: ['if count == 0, return "zero"', 'otherwise return "positive"'],
        constraints: [
          'Return exactly "zero" when count equals 0.',
          'Return exactly "positive" when count is greater than 0.',
        ],
        expected: { result_when_count_is_0: "zero" },
      }),
    );

    expect(prompt).toContain('return \\"zero\\"');
    expect(prompt).toContain('return \\"positive\\"');
    expect(prompt).not.toContain("result_when_count_is_0");
    expect(prompt).not.toContain('Return exactly "zero"');
  });

  it("builds a deterministic structure plan without executable body answers", () => {
    const plan = buildImplementationStructurePlan(
      samplePseudocodeJson({
        program_name: "print_label",
        purpose: "Print and return a label.",
        inputs: [{ name: "count", meaning: "integer" }],
        outputs: [{ name: "label", meaning: "text" }],
        effects: ["prints label"],
        algorithm: ['if count == 0, print "zero" and return "zero"', 'otherwise return "positive"'],
        expected: { result_when_count_is_0: "zero" },
      }),
    );

    expect(plan.symbols).toEqual({
      domain: "PrintLabelDomain",
      capability: "PrintLabelCapability",
      action: "PrintLabel",
    });
    expect(plan.files.action).toBe("domains/PrintLabelDomain/actions/PrintLabel.sophia");
    expect(plan.action_contract_hints.inputs).toEqual([]);
    expect(plan.action_contract_hints.output).toEqual({
      name: "label",
      type: "Text",
      source: "label := text",
    });
    expect(plan.action_contract_hints.effects).toEqual([]);
    expect(JSON.stringify(plan)).not.toContain("result_when_count_is_0");
    expect(JSON.stringify(plan)).not.toContain('return "zero"');
  });

  it("ignores pseudo implementation hints for deterministic scaffold names", () => {
    const plan = buildImplementationStructurePlan(
      samplePseudocodeJson({
        program_name: "NumberLabeler",
        inputs: [{ name: "count", meaning: "Int" }],
        outputs: [{ name: "result", meaning: "Text" }],
        effects: ["Console.Write"],
        implementation_hints: {
          domain: "WrongDomain",
          action: "GetLabel",
          capability: "WrongCapability",
        },
      }),
    );

    expect(plan.symbols).toEqual({
      domain: "NumberLabelerDomain",
      action: "NumberLabeler",
      capability: "NumberLabelerCapability",
    });
  });

  it("allows a public structure override to define scaffold names without changing semantics", () => {
    const plan = buildImplementationStructurePlan(
      samplePseudocodeJson({
        program_name: "ProcessDepositPipeline",
        definitions: [
          {
            name: "Account",
            fields: [
              { name: "balance", type: "Int" },
              { name: "is_locked", type: "Bool" },
            ],
          },
        ],
        inputs: [
          { name: "account", meaning: "Account" },
          { name: "amount", meaning: "Int" },
        ],
        outputs: [{ name: "result", meaning: "Account" }],
        algorithm: ["return updated account"],
      }),
      {
        program: "ProcessDepositPipeline",
        domain: "ActionPipelineDomain",
        action: "ProcessDepositPipeline",
        capability: "ActionPipelinePureCapability",
      },
    );

    expect(plan.symbols).toEqual({
      domain: "ActionPipelineDomain",
      action: "ProcessDepositPipeline",
      capability: "ActionPipelinePureCapability",
    });
    expect(plan.files.entities).toEqual(["domains/ActionPipelineDomain/entities/Account.sophia"]);
    expect(plan.action_contract_hints.inputs).toContainEqual({
      name: "account",
      type: "Account",
      source: "account := Account",
    });
  });

  it("uses public state and action contract overrides without executable body semantics", () => {
    const plan = buildImplementationStructurePlan(
      samplePseudocodeJson({
        program_name: "StateStatusLabel",
        purpose: "Return a label for the provided semantic state.",
        inputs: [{ name: "status", meaning: "current status" }],
        outputs: [{ name: "result", meaning: "status label" }],
        algorithm: ["branch on the semantic status and return the corresponding label"],
      }),
      {
        domain: "StateMatchDomain",
        action: "StateStatusLabel",
        capability: "StatePureCapability",
        states: [{ name: "TaskStatus", values: ["Pending", "Done"] }],
        inputs: [{ name: "status", type: "TaskStatus" }],
        output: { name: "result", type: "Text" },
        effects: [],
      },
    );

    expect(plan.files.states).toEqual(["domains/StateMatchDomain/states/TaskStatus.sophia"]);
    expect(plan.action_contract_hints.inputs).toEqual([
      { name: "status", type: "TaskStatus", source: "status: TaskStatus" },
    ]);
    expect(plan.action_contract_hints.output).toEqual({
      name: "result",
      type: "Text",
      source: "result: Text",
    });
    expect(JSON.stringify(plan)).not.toContain("return the corresponding label");
  });

  it("lets public state overrides win over pseudo entity guesses with the same name", () => {
    const plan = buildImplementationStructurePlan(
      samplePseudocodeJson({
        program_name: "StateStatusLabel",
        definitions: [{ name: "TaskStatus", fields: [{ name: "state", type: "Text" }] }],
      }),
      {
        domain: "StateMatchDomain",
        action: "StateStatusLabel",
        capability: "StatePureCapability",
        states: [{ name: "TaskStatus", values: ["Pending", "Done"] }],
        inputs: [{ name: "status", type: "TaskStatus" }],
        output: { name: "result", type: "Text" },
        effects: [],
      },
    );

    expect(plan.files.entities).toEqual([]);
    expect(plan.files.states).toEqual(["domains/StateMatchDomain/states/TaskStatus.sophia"]);
    expect(plan.action_contract_hints.entities).toEqual([]);
  });

  it("includes explicit entity declarations in the structure plan", () => {
    const plan = buildImplementationStructurePlan(
      samplePseudocodeJson({
        program_name: "DepositUnlockedAccount",
        definitions: [
          {
            name: "Account",
            fields: [
              { name: "balance", type: "Int" },
              { name: "is_locked", type: "Bool" },
            ],
          },
        ],
        inputs: [
          { name: "account", meaning: "Account" },
          { name: "amount", meaning: "Int" },
        ],
        outputs: [{ name: "result", meaning: "Account" }],
      }),
    );

    expect(plan.files.entities).toEqual([
      "domains/DepositUnlockedAccountDomain/entities/Account.sophia",
    ]);
    expect(plan.action_contract_hints.entities).toEqual([
      {
        name: "Account",
        fields: [
          { name: "balance", type: "Int", source: "balance: Int" },
          { name: "is_locked", type: "Bool", source: "is_locked: Bool" },
        ],
      },
    ]);
    expect(plan.action_contract_hints.inputs).toContainEqual({
      name: "account",
      type: "Account",
      source: "account := Account",
    });
  });

  it("does not infer record-like contracts from descriptive prose", () => {
    const plan = buildImplementationStructurePlan(
      samplePseudocodeJson({
        program_name: "ProcessRecord",
        definitions: [{ name: "Account", meaning: "record-like account" }],
        inputs: [
          { name: "account", meaning: "record-like entity with balance and is_locked fields" },
          { name: "amount", meaning: "integer deposit amount" },
        ],
        outputs: [{ name: "result", meaning: "record-like entity after update" }],
      }),
    );

    expect(plan.action_contract_hints.entities).toEqual([{ name: "Account", fields: [] }]);
    expect(plan.action_contract_hints.inputs).toEqual([]);
    expect(plan.action_contract_hints.output).toBeNull();
  });
});
