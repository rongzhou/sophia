import { afterEach, describe, expect, it, vi } from "vitest";
import {
  buildImplementDesignPrompt,
  implementDesignWithOllama,
  validateImplementationOutputForPseudocode,
} from "../../src/llm/tasks/implement_design.js";
import { LlmCallParseError } from "../../src/llm/errors.js";
import { buildDesignSolutionPrompt } from "../../src/llm/tasks/design_solution.js";
import { buildRepairPrompt, repairCodeWithOllama } from "../../src/llm/tasks/repair.js";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("prompt policy", () => {
  it("design-solution prompt does not expose implementation-stage vocabulary", () => {
    const prompt = buildDesignSolutionPrompt("Implement an account pipeline.");

    expect(prompt).not.toContain("PascalCaseDomainName");
    expect(prompt).not.toContain("implementation_hints");
    expect(prompt).not.toContain("action: MainActionName");
    expect(prompt).not.toContain("Sophia");
    expect(prompt).not.toContain("scaffold");
    expect(prompt).not.toContain("capability");
    expect(prompt).not.toContain("domain");
    expect(prompt).toContain("algorithm design");
    expect(prompt).toContain("JSON structure is allowed");
  });

  it("design-solution prompt preserves meaningful logical decomposition boundaries", () => {
    const prompt = buildDesignSolutionPrompt(
      "Implement a reusable pipeline with separate validation, update, and orchestration actions.",
    );

    expect(prompt).toContain("reusable logical steps");
    expect(prompt).toContain("represent them as named logical steps");
    expect(prompt).toContain("If a helper step performs only effects");
    expect(prompt).not.toContain("subaction");
    expect(prompt).not.toContain("main_flow");
  });

  it("keeps implementation prompt generic and avoids embedding repaired implementation snippets", () => {
    const prompt = buildImplementDesignPrompt(`
program Demo {
  purpose { "Return generated numbers." }
  inputs { none }
  outputs { numbers := "a generated list" }
  algorithm {
    create empty list numbers
    repeat 3 times {
      set next to 1
      append next to numbers
    }
    return numbers
  }
}
`);

    expect(prompt).not.toContain("let mutable numbers");
    expect(prompt).not.toContain("set numbers = numbers.append");
    expect(prompt).not.toContain("PrintFirstTenRabbitNumbers");
    expect(prompt).toContain("Deterministic structure plan");
    expect(prompt).toContain("Deterministic Sophia scaffold");
    expect(prompt).toContain("Action-rooted semantic context");
    expect(prompt).toContain('"root"');
    expect(prompt).toContain('"sources"');
    expect(prompt).toContain("[TODO: LLM-fill from pseudo.algorithm]");
    expect(prompt).toContain("Use this only as a structural plan");
    expect(prompt).toContain("Action output fields are not implicit variables");
    expect(prompt).toContain("Every non-Unit action body must reach a return expr");
    expect(prompt).toContain('Do not write a "call" keyword');
    expect(prompt).toContain('Never write a "call" keyword');
    expect(prompt).toContain("Text concatenation with no implicit conversion");
    expect(prompt).toContain('"prefix: " + label');
    expect(prompt).toContain("Local let declarations never use type annotations");
    expect(prompt).toContain("let mutable result = item");
    expect(prompt).toContain("pseudo_outline.mutable_state_candidates");
    expect(prompt).toContain("empty list initialization: let mutable values = []");
    expect(prompt).toContain("Do not write empty List<Int>");
    expect(prompt).toContain("A Unit action must return unit");
    expect(prompt).toContain("let ignored = HelperAction");
    expect(prompt).toContain("Do not invent Int.toText");
    expect(prompt).toContain("Do not copy pseudocode branch notation");
    expect(prompt).toContain("Sophia v0 uses `if condition { ... } else { ... }`");
  });

  it("passes mutable state candidates to the implementation model", () => {
    const prompt = buildImplementDesignPrompt(`
program Validate {
  purpose { "Validate a value." }
  inputs { value: Int }
  outputs { result: Bool }
  algorithm {
    set is_valid to false
    if value > 0 {
      set is_valid to true
    }
    return is_valid
  }
}
`);

    expect(prompt).toContain('"mutable_state_candidates"');
    expect(prompt).toContain('"is_valid"');
    expect(prompt).toContain("let mutable");
  });

  it("can implement against a public structure override without requiring pseudo hints", () => {
    const prompt = buildImplementDesignPrompt(
      `
program ProcessDepositPipeline {
  purpose { "Process a deposit." }
  entities {
    Account {
      balance: Int
      is_locked: Bool
    }
  }
  inputs { account: Account, amount: Int }
  outputs { result: Account }
  algorithm {
    subaction ValidateDeposit { return whether deposit is allowed }
    subaction UpdateAccountBalance { return updated account }
    main_flow ProcessDepositPipeline { call validation before update }
  }
}
`,
      {
        program: "ProcessDepositPipeline",
        domain: "ActionPipelineDomain",
        action: "ProcessDepositPipeline",
        capability: "ActionPipelinePureCapability",
      },
    );

    expect(prompt).toContain("domains/ActionPipelineDomain/actions/ProcessDepositPipeline.sophia");
    expect(prompt).toContain("ActionPipelinePureCapability");
    expect(prompt).toContain("required helper action boundaries");
    expect(prompt).not.toContain("result_when");
  });

  it("uses generic syntax examples instead of target-shaped account examples", () => {
    const prompt = buildImplementDesignPrompt(`
program GenericRecordFlow {
  purpose { "Update a record-like value." }
  entities {
    SampleItem {
      value: Int
      is_active: Bool
    }
  }
  inputs {
    item: SampleItem
    delta: Int
  }
  outputs { result: SampleItem }
  algorithm {
    return SampleItem with updated value
  }
}
`);

    expect(prompt).toContain("entity Item");
    expect(prompt).toContain("OtherAction { item = item, delta = delta }");
    expect(prompt).not.toContain("entity Account");
    expect(prompt).not.toContain("account.balance");
    expect(prompt).not.toContain("amount = amount");
  });

  it("does not expose expected result literals to the implementation model", () => {
    const prompt = buildImplementDesignPrompt(`
program BuildThreeNumbers {
  purpose { "Build a list." }
  inputs { none }
  outputs { numbers := "list" }
  algorithm {
    create empty list numbers
    set current to 2
    repeat 3 times {
      append current to numbers
      set current to current + 2
    }
    return numbers
  }
  constraints {
    "The sequence must be 2, 4, 6."
    "Do not hardcode the full list."
  }
  expected {
    result := "[2, 4, 6]"
  }
}
`);

    expect(prompt).not.toContain("[2, 4, 6]");
    expect(prompt).not.toContain("The sequence must be 2, 4, 6.");
    expect(prompt).toContain("<redacted for implementation");
  });

  it("repair prompt gives syntax rules without telling the model to pass tests", () => {
    const prompt = buildRepairPrompt(
      {
        "domains/demo/actions/demo.sophia": "action Demo { body { var x = 1 } }",
      },
      {
        ok: false,
        diagnostics: [
          {
            code: "CHECK-SYNTAX-006",
            severity: "error",
            problem: "Unsupported var.",
          },
        ],
      },
      `
program Demo {
  algorithm { return x }
  forbidden { "Do not use storage." }
}
`,
    );

    expect(prompt).toContain("Apply only the diagnostics");
    expect(prompt).not.toMatch(/make (the )?tests pass/i);
    expect(prompt).not.toMatch(/ignore diagnostics/i);
    expect(prompt).toContain("Compact repair context");
    expect(prompt).toContain("Action-rooted semantic context");
    expect(prompt).toContain(
      "A variable updated with set must have been declared with let mutable.",
    );
    expect(prompt).toContain("Text concatenation with no implicit conversion");
    expect(prompt).toContain("CHECK-FILE-003");
    expect(prompt).toContain("domain Name {}");
    expect(prompt).toContain("Ancestor .pseudo semantic constraints");
    expect(prompt).toContain("Deterministic Sophia scaffold");
    expect(prompt).toContain("[TODO: LLM-fill from pseudo.algorithm]");
    expect(prompt).toContain("Do not use storage.");
  });

  it("repair prompt redacts expected outputs and validation-only constraints", () => {
    const prompt = buildRepairPrompt(
      {
        "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: DemoCapability
  output { result: List<Int> }
  effects { }
  body { return [2, 4, 6] }
}
`,
      },
      {
        ok: false,
        diagnostics: [
          {
            code: "CHECK-VAR-001",
            severity: "error",
            problem: "Identifier is not declared: values.",
          },
        ],
      },
      `
program BuildValues {
  purpose { "Build values." }
  inputs { none }
  outputs { result: List<Int> }
  algorithm {
    create empty list values
    return values
  }
  constraints {
    "The sequence must be 2, 4, 6."
    "Do not hardcode the full list."
  }
  expected {
    result := "[2, 4, 6]"
  }
}
`,
    );

    expect(prompt).toContain("<redacted validation detail>");
    expect(prompt).toContain("Do not hardcode the full list.");
    expect(prompt).not.toContain('result := "[2, 4, 6]"');
    expect(prompt).not.toContain("The sequence must be 2, 4, 6.");
  });
});

describe("implementation output validation", () => {
  const minimalPseudocode = `
program Demo {
  purpose { "Demo." }
  inputs { none }
  outputs { result: Unit }
  algorithm { return unit }
}
`;

  it("rejects unsafe paths", () => {
    expect(() =>
      validateImplementationOutputForPseudocode(
        {
          files: {
            "../escape.sophia": "domain Bad {}",
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        minimalPseudocode,
      ),
    ).toThrow("Invalid Sophia output path");
  });

  it("rejects paths outside the v0 domain layout", () => {
    expect(() =>
      validateImplementationOutputForPseudocode(
        {
          files: {
            "domains/demo/misc/demo.sophia": "action Demo {}",
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        minimalPseudocode,
      ),
    ).toThrow("Invalid Sophia output path");
  });

  it("requires domain, capability, and action files", () => {
    expect(() =>
      validateImplementationOutputForPseudocode(
        {
          files: {
            "domains/Demo/domain.sophia": "domain Demo {}",
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        minimalPseudocode,
      ),
    ).toThrow("capability");
  });

  it("requires implementation output to preserve deterministic scaffold structure", () => {
    const pseudocode = `
program DoubleInput {
  purpose { "Double an input." }
  inputs { count := "integer" }
  outputs { result := "the input multiplied by two" }
  algorithm {
    set result to count multiplied by 2
    return result
  }
}
`;

    expect(() =>
      validateImplementationOutputForPseudocode(
        {
          files: {
            "domains/OtherDomain/domain.sophia": "domain OtherDomain {}",
            "domains/OtherDomain/capabilities/OtherCapability.sophia":
              "capability OtherCapability { allow { } }",
            "domains/OtherDomain/actions/Other.sophia": `
action Other {
  capability: OtherCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body { return count * 2 }
}
`,
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        pseudocode,
      ),
    ).toThrow("preserve deterministic scaffold file paths");
  });

  it("rejects implementation output that leaves scaffold TODO comments", () => {
    const pseudocode = `
program DoubleInput {
  purpose { "Double an input." }
  inputs { count := "integer" }
  outputs { result := "the input multiplied by two" }
  algorithm {
    set result to count multiplied by 2
    return result
  }
}
`;

    expect(() =>
      validateImplementationOutputForPseudocode(
        {
          files: {
            "domains/DoubleInputDomain/domain.sophia": "domain DoubleInputDomain {}",
            "domains/DoubleInputDomain/capabilities/DoubleInputCapability.sophia":
              "capability DoubleInputCapability { allow { } }",
            "domains/DoubleInputDomain/actions/DoubleInput.sophia": `
action DoubleInput {
  capability: DoubleInputCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    // [TODO: LLM-fill from pseudo.algorithm]
  }
}
`,
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        pseudocode,
      ),
    ).toThrow("scaffold TODO");
  });

  it("accepts implementation output that fills scaffold body while preserving contract", () => {
    const pseudocode = `
program DoubleInput {
  purpose { "Double an input." }
  inputs { count := "integer" }
  outputs { result := "the input multiplied by two" }
  algorithm {
    set result to count multiplied by 2
    return result
  }
}
`;

    expect(
      validateImplementationOutputForPseudocode(
        {
          files: {
            "domains/DoubleInputDomain/domain.sophia": "domain DoubleInputDomain {}",
            "domains/DoubleInputDomain/capabilities/DoubleInputCapability.sophia":
              "capability DoubleInputCapability { allow { } }",
            "domains/DoubleInputDomain/actions/DoubleInput.sophia": `
action DoubleInput {
  capability: DoubleInputCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    let mutable result = count * 2
    return result
  }
}
`,
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        pseudocode,
      ).files["domains/DoubleInputDomain/actions/DoubleInput.sophia"],
    ).toContain("count * 2");
  });

  it("requires public state scaffold contracts to remain state files", () => {
    const pseudocode = `
program StateStatusLabel {
  purpose { "Return a label for a semantic state." }
  inputs { status := "current semantic state" }
  outputs { result := "text label" }
  algorithm {
    if status means first state then return the first label
    otherwise return the second label
  }
}
`;

    const structureOverride = {
      domain: "StateMatchDomain",
      action: "StateStatusLabel",
      capability: "StatePureCapability",
      states: [{ name: "TaskStatus", values: ["Pending", "Done"] }],
      inputs: [{ name: "status", type: "TaskStatus" }],
      output: { name: "result", type: "Text" },
      effects: [],
    };

    expect(() =>
      validateImplementationOutputForPseudocode(
        {
          files: {
            "domains/StateMatchDomain/domain.sophia": "domain StateMatchDomain {}",
            "domains/StateMatchDomain/entities/TaskStatus.sophia":
              "entity TaskStatus { fields { } }",
            "domains/StateMatchDomain/capabilities/StatePureCapability.sophia":
              "capability StatePureCapability { allow { } }",
            "domains/StateMatchDomain/actions/StateStatusLabel.sophia": `
action StateStatusLabel {
  capability: StatePureCapability
  input { status: TaskStatus }
  output { result: Text }
  effects { }
  body { return "pending" }
}
`,
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        pseudocode,
        structureOverride,
      ),
    ).toThrow("missing domains/StateMatchDomain/states/TaskStatus.sophia");
  });

  it("normalizes explicit public state file values without filling branch logic", () => {
    const pseudocode = `
program StateStatusLabel {
  purpose { "Return a label for a semantic state." }
  inputs { status := "current semantic state" }
  outputs { result := "text label" }
  algorithm {
    branch on status and return the corresponding label
  }
}
`;

    const output = validateImplementationOutputForPseudocode(
      {
        files: {
          "domains/StateMatchDomain/domain.sophia": "domain StateMatchDomain {}",
          "domains/StateMatchDomain/states/TaskStatus.sophia": `
state TaskStatus {
  value pending { }
  value done { }
}
`,
          "domains/StateMatchDomain/capabilities/StatePureCapability.sophia":
            "capability StatePureCapability { allow { } }",
          "domains/StateMatchDomain/actions/StateStatusLabel.sophia": `
action StateStatusLabel {
  capability: StatePureCapability
  input { status: TaskStatus }
  output { result: Text }
  effects { }
  body { return "pending" }
}
`,
        },
        notes: [],
        self_check: {
          no_var: true,
          no_direct_console_write: true,
          no_for_or_while: true,
          preserved_constraints: true,
        },
      },
      pseudocode,
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

    expect(output.files["domains/StateMatchDomain/states/TaskStatus.sophia"]).toContain(
      "value Pending",
    );
    expect(output.files["domains/StateMatchDomain/actions/StateStatusLabel.sophia"]).toContain(
      'return "pending"',
    );
  });

  it("accepts additional action files when the main scaffold contract is preserved", () => {
    const pseudocode = `
program ProcessDepositPipeline {
  purpose { "Process deposit through explicit subactions." }
  entities {
    PipelineAccount {
      balance: Int
      is_locked: Bool
    }
  }
  inputs {
    account: PipelineAccount
    amount: Int
  }
  outputs { result: PipelineAccount }
  algorithm {
    subaction CanDepositPipeline { return not account.is_locked and amount > 0 }
    subaction ApplyDepositPipeline { return updated account }
    main action ProcessDepositPipeline { call both subactions }
  }
}
`;

    const output = validateImplementationOutputForPseudocode(
      {
        files: {
          "domains/ActionPipelineDomain/domain.sophia": "domain ActionPipelineDomain {}",
          "domains/ActionPipelineDomain/entities/PipelineAccount.sophia": `
entity PipelineAccount {
  fields {
    balance: Int
    is_locked: Bool
  }
}
`,
          "domains/ActionPipelineDomain/capabilities/ActionPipelinePureCapability.sophia":
            "capability ActionPipelinePureCapability { allow { } }",
          "domains/ActionPipelineDomain/actions/CanDepositPipeline.sophia": `
action CanDepositPipeline {
  capability: ActionPipelinePureCapability
  input { account: PipelineAccount amount: Int }
  output { result: Bool }
  effects { }
  body { return not account.is_locked and amount > 0 }
}
`,
          "domains/ActionPipelineDomain/actions/ApplyDepositPipeline.sophia": `
action ApplyDepositPipeline {
  capability: ActionPipelinePureCapability
  input { account: PipelineAccount amount: Int }
  output { result: PipelineAccount }
  effects { }
  body {
    let updated_balance = account.balance + amount
    return PipelineAccount { balance = updated_balance, is_locked = account.is_locked }
  }
}
`,
          "domains/ActionPipelineDomain/actions/ProcessDepositPipeline.sophia": `
action ProcessDepositPipeline {
  capability: ActionPipelinePureCapability
  input { account: PipelineAccount amount: Int }
  output { result: PipelineAccount }
  effects { }
  body {
    let can_deposit = CanDepositPipeline { account = account, amount = amount }
    if can_deposit {
      return ApplyDepositPipeline { account = account, amount = amount }
    } else {
      return account
    }
  }
}
`,
        },
        notes: [],
        self_check: {
          no_var: true,
          no_direct_console_write: true,
          no_for_or_while: true,
          preserved_constraints: true,
        },
      },
      pseudocode,
      {
        domain: "ActionPipelineDomain",
        capability: "ActionPipelinePureCapability",
        action: "ProcessDepositPipeline",
      },
    );

    expect(
      Object.keys(output.files).filter((filePath) => filePath.includes("/actions/")),
    ).toHaveLength(3);
  });

  it("validates scaffold paths against a public structure override", () => {
    const pseudocode = `
program ProcessDepositPipeline {
  purpose { "Process deposit through explicit subactions." }
  entities {
    Account {
      balance: Int
      is_locked: Bool
    }
  }
  inputs { account: Account, amount: Int }
  outputs { result: Account }
  algorithm {
    subaction CanDeposit { return allowed }
    main_flow ProcessDepositPipeline { call CanDeposit }
  }
}
`;

    expect(() =>
      validateImplementationOutputForPseudocode(
        {
          files: {
            "domains/ActionPipelineDomain/domain.sophia": "domain ActionPipelineDomain {}",
            "domains/ActionPipelineDomain/entities/Account.sophia": `
entity Account {
  fields {
    balance: Int
    is_locked: Bool
  }
}
`,
            "domains/ActionPipelineDomain/capabilities/ActionPipelinePureCapability.sophia":
              "capability ActionPipelinePureCapability { allow { } }",
            "domains/ActionPipelineDomain/actions/CanDeposit.sophia": `
action CanDeposit {
  capability: ActionPipelinePureCapability
  input { account: Account amount: Int }
  output { result: Bool }
  effects { }
  body { return amount > 0 and not account.is_locked }
}
`,
            "domains/ActionPipelineDomain/actions/ProcessDepositPipeline.sophia": `
action ProcessDepositPipeline {
  capability: ActionPipelinePureCapability
  input { account: Account amount: Int }
  output { result: Account }
  effects { }
  body {
    let ok = CanDeposit { account = account, amount = amount }
    if ok {
      return account
    } else {
      return account
    }
  }
}
`,
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        pseudocode,
        {
          program: "ProcessDepositPipeline",
          domain: "ActionPipelineDomain",
          action: "ProcessDepositPipeline",
          capability: "ActionPipelinePureCapability",
        },
      ),
    ).not.toThrow();
  });

  it("requires explicit pseudo subactions to be preserved and called", () => {
    const pseudocode = `
program Flow {
  purpose { "Use helper steps." }
  inputs { value: Int }
  outputs { result: Int }
  algorithm {
    subaction ValidateValue {
      return whether value is positive
    }
    main_flow Flow {
      if output of ValidateValue using value {
        return value
      } else {
        return 0
      }
    }
  }
}
`;

    expect(() =>
      validateImplementationOutputForPseudocode(
        {
          files: {
            "domains/FlowDomain/domain.sophia": "domain FlowDomain {}",
            "domains/FlowDomain/capabilities/FlowCapability.sophia":
              "capability FlowCapability { allow { } }",
            "domains/FlowDomain/actions/Flow.sophia": `
action Flow {
  capability: FlowCapability
  input { value: Int }
  output { result: Int }
  effects { }
  body {
    if value > 0 {
      return value
    } else {
      return 0
    }
  }
}
`,
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        pseudocode,
      ),
    ).toThrow("preserve pseudo subaction ValidateValue");
  });

  it("wraps invalid implementation output as a parse error with raw response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => ({
          model: "qwen-test",
          response: JSON.stringify({ files: { "domains/demo/domain.sophia": "domain Demo {}" } }),
        }),
      })),
    );

    await expect(
      implementDesignWithOllama({ pseudocode: "program Demo {}", model: "qwen-test" }),
    ).rejects.toBeInstanceOf(LlmCallParseError);
  });

  it("wraps invalid repair output as a parse error with raw response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => ({
          model: "qwen-test",
          response: JSON.stringify({ files: { "domains/demo/domain.sophia": "domain Demo {}" } }),
        }),
      })),
    );

    await expect(
      repairCodeWithOllama({
        files: { "domains/demo/actions/demo.sophia": "action Demo {}" },
        checkResult: {
          ok: false,
          diagnostics: [{ code: "CHECK-ACTION-001", severity: "error", problem: "missing" }],
        },
        model: "qwen-test",
        pseudocode: "program Demo {}",
      }),
    ).rejects.toBeInstanceOf(LlmCallParseError);
  });

  it("accepts scaffold-preserving List<Int> output fields", () => {
    expect(() =>
      validateImplementationOutputForPseudocode(
        {
          files: {
            "domains/DemoDomain/domain.sophia": "domain DemoDomain {}",
            "domains/DemoDomain/capabilities/DemoCapability.sophia":
              "capability DemoCapability { allow { } }",
            "domains/DemoDomain/actions/Demo.sophia": `
action Demo {
  capability: DemoCapability
  input { }
  output {
    result: List<Int>
  }
  effects { }
  errors { }
  body {
    return []
  }
}
`,
          },
          notes: [],
          self_check: {
            no_var: true,
            no_direct_console_write: true,
            no_for_or_while: true,
            preserved_constraints: true,
          },
        },
        `
program Demo {
  purpose { "Return a list." }
  inputs { none }
  outputs { result: List<Int> }
  algorithm { return empty list }
}
`,
      ),
    ).not.toThrow();
  });
});
