import { describe, expect, it } from "vitest";
import {
  buildImplementationStructurePlan,
  pseudocodeForImplementationPrompt,
} from "../../src/pseudo/structure_plan.js";
import { buildImplementDesignPrompt } from "../../src/llm/tasks/implement_design.js";

describe("pseudocodeForImplementationPrompt", () => {
  it("redacts expected outputs and validation sequence details before implementation", () => {
    const sanitized = pseudocodeForImplementationPrompt(`
program Demo {
  purpose { "Build a list." }
  inputs { none }
  outputs { numbers := "list" }
  algorithm {
    create empty list numbers
    repeat 3 times {
      append current to numbers
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

    expect(sanitized).toContain("expected {");
    expect(sanitized).toContain("<redacted for implementation");
    expect(sanitized).toContain("Do not hardcode the full list.");
    expect(sanitized).not.toContain("[2, 4, 6]");
    expect(sanitized).not.toContain("The sequence must be 2, 4, 6.");
  });

  it("keeps algorithm literals that are required behavior, not validation answers", () => {
    const prompt = buildImplementDesignPrompt(`
program Hello {
  purpose { "Print a greeting." }
  inputs { none }
  outputs { result := "unit" }
  algorithm {
    print "Hello, Sophia!"
    return unit
  }
  expected {
    stdout := "Hello, Sophia!\\n"
    result := "unit"
  }
}
`);

    expect(prompt).toContain('print "Hello, Sophia!"');
    expect(prompt).not.toContain('stdout := "Hello, Sophia!\\n"');
  });

  it("redacts exact validation constraints while preserving branch behavior", () => {
    const prompt = buildImplementDesignPrompt(`
program Label {
  purpose { "Return a label." }
  inputs { count := "integer" }
  outputs { label := "text" }
  algorithm {
    if count == 0 {
      return "zero"
    } else {
      return "positive"
    }
  }
  constraints {
    "Return exactly zero when count equals 0."
    "Return exactly positive when count is greater than 0."
  }
  expected {
    result_when_count_is_0 := "zero"
  }
}
`);

    expect(prompt).toContain('return "zero"');
    expect(prompt).toContain('return "positive"');
    expect(prompt).not.toContain("Return exactly zero");
    expect(prompt).not.toContain("result_when_count_is_0");
  });

  it("does not infer output types from semantic label descriptions", () => {
    const plan = buildImplementationStructurePlan(`
program Label {
  purpose { "Classify a measured value." }
  inputs { value := "integer input" }
  outputs { result := "classification label" }
  algorithm {
    if value is below threshold then
      return the word cold
    otherwise
      return the word warm
    end if
  }
}
`);

    expect(plan.action_contract_hints.output).toBeNull();
  });

  it("does not infer input or output types from descriptive text-label prose", () => {
    const plan = buildImplementationStructurePlan(`
program Label {
  purpose { "Return a text label." }
  inputs { count := "non-negative integer" }
  outputs { result := "text label: cold if count equals zero, warm otherwise" }
  algorithm {
    if count equals zero then
      return the word cold
    otherwise
      return the word warm
    end if
  }
}
`);

    expect(plan.action_contract_hints.inputs).toEqual([]);
    expect(plan.action_contract_hints.output).toBeNull();
  });

  it("does not infer boolean outputs from descriptive prose", () => {
    const plan = buildImplementationStructurePlan(`
program CheckAllowed {
  purpose { "Return whether a count is allowed." }
  inputs { count := "integer input" }
  outputs { result := "true when count is allowed, false otherwise" }
  algorithm {
    return whether count is allowed
  }
}
`);

    expect(plan.action_contract_hints.output).toBeNull();
  });

  it("builds a deterministic structure plan without executable body answers", () => {
    const plan = buildImplementationStructurePlan(`
program print_label {
  purpose { "Print and return a label." }
  inputs {
    count := "integer"
  }
  outputs {
    label := "text"
  }
  effects { Console.Write }
  algorithm {
    if count == 0 {
      print "zero"
      return "zero"
    } else {
      print "positive"
      return "positive"
    }
  }
  expected {
    result_when_count_is_0 := "zero"
  }
}
`);

    expect(plan.symbols).toEqual({
      domain: "PrintLabelDomain",
      capability: "PrintLabelCapability",
      action: "PrintLabel",
    });
    expect(plan.files.action).toBe("domains/PrintLabelDomain/actions/PrintLabel.sophia");
    expect(plan.action_contract_hints.inputs).toEqual([]);
    expect(plan.action_contract_hints.output).toBeNull();
    expect(plan.action_contract_hints.effects).toEqual([]);
    expect(JSON.stringify(plan)).not.toContain("result_when_count_is_0");
    expect(JSON.stringify(plan)).not.toContain('return "zero"');
  });

  it("does not turn print wording into a formal scaffold effect contract", () => {
    const plan = buildImplementationStructurePlan(`
program PrintSemanticValue {
  purpose { "Print a semantic value." }
  inputs { value := "integer value" }
  outputs { result := "no returned value" }
  effects { "prints the value" }
  algorithm {
    print value
    return unit
  }
}
`);

    expect(plan.action_contract_hints.effects).toEqual([]);
  });

  it("ignores pseudo implementation hints for deterministic scaffold names", () => {
    const plan = buildImplementationStructurePlan(`
program NumberLabeler {
  purpose { "Return a label." }
  inputs { count: Int }
  outputs { result: Text }
  effects { Console.Write }
  algorithm {
    if count == 0 {
      print "zero"
      return "zero"
    } else {
      return "positive"
    }
  }
  implementation_hints {
    domain: NumberLabelerDomain
    action: GetLabel
    capability: TextLabelerCapability
  }
}
`);

    expect(plan.symbols).toEqual({
      domain: "NumberLabelerDomain",
      action: "NumberLabeler",
      capability: "NumberLabelerCapability",
    });
    expect(plan.files).toEqual({
      domain: "domains/NumberLabelerDomain/domain.sophia",
      entities: [],
      states: [],
      capability: "domains/NumberLabelerDomain/capabilities/NumberLabelerCapability.sophia",
      action: "domains/NumberLabelerDomain/actions/NumberLabeler.sophia",
    });
  });

  it("allows a public structure override to define scaffold names without changing semantics", () => {
    const plan = buildImplementationStructurePlan(
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
  algorithm { return updated account }
}
`,
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
    expect(plan.files.action).toBe(
      "domains/ActionPipelineDomain/actions/ProcessDepositPipeline.sophia",
    );
    expect(plan.files.entities).toEqual(["domains/ActionPipelineDomain/entities/Account.sophia"]);
  });

  it("uses public state and action contract overrides without executable body semantics", () => {
    const plan = buildImplementationStructurePlan(
      `
program StateStatusLabel {
  purpose { "Return a label for the provided semantic state." }
  inputs { status := "current status" }
  outputs { result := "status label" }
  algorithm {
    branch on the semantic status and return the corresponding label
  }
}
`,
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
    expect(plan.action_contract_hints.states).toEqual([
      {
        name: "TaskStatus",
        values: ["Pending", "Done"],
        source: "TaskStatus: Pending, Done",
      },
    ]);
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
      `
program StateStatusLabel {
  purpose { "Return a label for the provided semantic state." }
  entities {
    TaskStatus {
      state := "one of Pending or Done"
    }
  }
  inputs { status := "a TaskStatus entity representing the current state" }
  outputs { result := "text" }
  algorithm {
    match status.state:
      case Pending:
        return "pending"
      case Done:
        return "done"
  }
}
`,
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
    expect(plan.action_contract_hints.states).toEqual([
      {
        name: "TaskStatus",
        values: ["Pending", "Done"],
        source: "TaskStatus: Pending, Done",
      },
    ]);
  });

  it("includes explicit entity declarations in the structure plan", () => {
    const plan = buildImplementationStructurePlan(`
program DepositUnlockedAccount {
  purpose { "Deposit into an account." }
  entities {
    Account {
      balance: Int
      is_locked: Bool
    }
  }
  inputs {
    account: Account
    amount: Int
  }
  outputs { result: Account }
  algorithm { return updated account }
  implementation_hints {
    domain := "AccountWorkflowDomain"
    capability := "AccountWorkflowCapability"
    action := "DepositUnlockedAccount"
  }
}
`);

    expect(plan.files.entities).toEqual(["domains/DepositUnlockedAccountDomain/entities/Account.sophia"]);
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
      source: "account: Account",
    });
  });

  it("extracts comma-separated inline input hints independently", () => {
    const plan = buildImplementationStructurePlan(`
program InlineInputs {
  purpose { "Use inline inputs." }
  entities {
    Item {
      value: Int
      is_active: Bool
    }
  }
  inputs { item: Item, delta: Int }
  outputs { result: Item }
  algorithm { return item }
}
`);

    expect(plan.action_contract_hints.inputs).toEqual([
      { name: "item", type: "Item", source: "item: Item" },
      { name: "delta", type: "Int", source: "delta: Int" },
    ]);
    expect(plan.action_contract_hints.output).toEqual({
      name: "result",
      type: "Item",
      source: "result: Item",
    });
  });

  it("does not infer record-like contracts from declared entity field prose", () => {
    const plan = buildImplementationStructurePlan(`
program ProcessRecord {
  purpose { "Update a record-like value." }
  entities {
    Account {
      balance := "integer current account balance"
      is_locked := "boolean lock flag"
    }
  }
  inputs {
    account := "record-like entity with balance and is_locked fields"
    amount := "integer deposit amount"
  }
  outputs {
    result := "record-like entity representing the account after the update"
  }
  algorithm { return updated account }
}
`);

    expect(plan.action_contract_hints.entities).toEqual([{ name: "Account", fields: [] }]);
    expect(plan.action_contract_hints.inputs).toEqual([]);
    expect(plan.action_contract_hints.output).toBeNull();
  });

  it("does not infer primitive or entity types from descriptive prose", () => {
    const plan = buildImplementationStructurePlan(`
program ValidateAndApplyDelta {
  purpose { "Validate an item and a numeric delta." }
  entities {
    Item {
      value := "integer current value"
      is_active := "boolean active flag"
    }
  }
  inputs {
    item := "the source Item record to potentially update"
    delta := "integer amount to add to item.value"
  }
  outputs {
    result := "Item record representing the final state"
  }
  algorithm { return updated item }
}
`);

    expect(plan.action_contract_hints.entities).toEqual([{ name: "Item", fields: [] }]);
    expect(plan.action_contract_hints.inputs).toEqual([]);
    expect(plan.action_contract_hints.output).toBeNull();
  });

  it("does not treat lists of declared entity names as scalar entity fields", () => {
    const plan = buildImplementationStructurePlan(`
program WrappedList {
  purpose { "Describe a list wrapper." }
  entities {
    Item {
      value := "integer"
    }
    ItemSequence {
      items := "list of Item values"
    }
  }
  inputs { none }
  outputs { result := "ItemSequence containing generated values" }
  algorithm { return sequence }
}
`);

    expect(plan.action_contract_hints.entities).toEqual([
      { name: "Item", fields: [] },
      { name: "ItemSequence", fields: [] },
    ]);
    expect(plan.action_contract_hints.output).toBeNull();
  });
});
