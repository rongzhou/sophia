import { describe, expect, it } from "vitest";
import { buildSophiaScaffold } from "../../src/pseudo/scaffold.js";

describe("buildSophiaScaffold", () => {
  it("generates deterministic Sophia structure from pseudocode without filling business body", () => {
    const scaffold = buildSophiaScaffold(
      `
program PrintNumbers {
  purpose { "Print and return numbers." }
  inputs { none }
  outputs { numbers: List<Int> }
  effects { Console.Write }
  algorithm {
    create empty list numbers
    repeat 3 times {
      append current to numbers
      print current
    }
    return numbers
  }
}
`,
      { output: { name: "numbers", type: "List<Int>" }, effects: ["Console.Write"] },
    );

    expect(Object.keys(scaffold.files).sort()).toEqual([
      "domains/PrintNumbersDomain/actions/PrintNumbers.sophia",
      "domains/PrintNumbersDomain/capabilities/PrintNumbersCapability.sophia",
      "domains/PrintNumbersDomain/domain.sophia",
    ]);
    expect(scaffold.files["domains/PrintNumbersDomain/actions/PrintNumbers.sophia"]).toContain(
      "capability: PrintNumbersCapability",
    );
    expect(scaffold.files["domains/PrintNumbersDomain/actions/PrintNumbers.sophia"]).toContain(
      "numbers: List<Int>",
    );
    expect(scaffold.files["domains/PrintNumbersDomain/actions/PrintNumbers.sophia"]).toContain(
      "Console.Write",
    );
    expect(scaffold.files["domains/PrintNumbersDomain/actions/PrintNumbers.sophia"]).toContain(
      "[TODO: LLM-fill from pseudo.algorithm]",
    );
    expect(scaffold.files["domains/PrintNumbersDomain/actions/PrintNumbers.sophia"]).not.toContain(
      "repeat 3 times",
    );
    expect(scaffold.files["domains/PrintNumbersDomain/actions/PrintNumbers.sophia"]).not.toContain(
      "append current",
    );
  });

  it("infers input and text list contracts from generic pseudo descriptions", () => {
    const scaffold = buildSophiaScaffold(`
program AddItem {
  purpose { "Add text to a list." }
  inputs {
    items: List<Text>
    text: Text
  }
  outputs { updated_items: List<Text> }
  algorithm {
    append text to items
    return items
  }
}
`);

    const action = scaffold.files["domains/AddItemDomain/actions/AddItem.sophia"];
    expect(action).toContain("items: List<Text>");
    expect(action).toContain("text: Text");
    expect(action).toContain("updated_items: List<Text>");
    expect(action).not.toContain("set items = items + [text]");
  });

  it("uses explicit integer contracts without filling arithmetic bodies", () => {
    const scaffold = buildSophiaScaffold(`
program DoubleInput {
  purpose { "Double an input." }
  inputs { count: Int }
  outputs { result: Int }
  algorithm {
    set result to count multiplied by 2
    return result
  }
}
`);

    const action = scaffold.files["domains/DoubleInputDomain/actions/DoubleInput.sophia"];
    expect(action).toContain("count: Int");
    expect(action).toContain("result: Int");
    expect(action).not.toContain("count * 2");
  });

  it("uses public structure overrides for names and paths without filling the body", () => {
    const scaffold = buildSophiaScaffold(
      `
program PrintNumbers {
  purpose { "Print and return numbers." }
  inputs { none }
  outputs { numbers: List<Int> }
  effects { Console.Write := "print each number" }
  algorithm {
    create empty list numbers
    return numbers
  }
}
`,
      {
        domain: "MathDomain",
        capability: "MathConsoleCapability",
        action: "PrintNumbers",
        output: { name: "numbers", type: "List<Int>" },
        effects: ["Console.Write"],
      },
    );

    expect(Object.keys(scaffold.files).sort()).toEqual([
      "domains/MathDomain/actions/PrintNumbers.sophia",
      "domains/MathDomain/capabilities/MathConsoleCapability.sophia",
      "domains/MathDomain/domain.sophia",
    ]);
    const action = scaffold.files["domains/MathDomain/actions/PrintNumbers.sophia"];
    expect(action).toContain("capability: MathConsoleCapability");
    expect(action).toContain("[TODO: LLM-fill from pseudo.algorithm]");
    expect(action).not.toContain("return numbers");
  });

  it("uses explicit Bool contracts without filling branch bodies", () => {
    const scaffold = buildSophiaScaffold(`
program CheckAllowed {
  purpose { "Return whether a count is allowed." }
  inputs { is_locked: Bool }
  outputs { allowed: Bool }
  effects { none }
  algorithm {
    if not is_locked {
      return true
    } else {
      return false
    }
  }
}
`);

    const action = scaffold.files["domains/CheckAllowedDomain/actions/CheckAllowed.sophia"];
    expect(action).toContain("is_locked: Bool");
    expect(action).toContain("allowed: Bool");
  });

  it("generates entity scaffold files from explicit pseudo entity declarations", () => {
    const scaffold = buildSophiaScaffold(
      `
program DepositUnlockedAccount {
  purpose { "Deposit into an account when it is unlocked." }
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
  effects { none }
  algorithm {
    return Account with updated balance
  }
}
`,
      {
        domain: "AccountWorkflowDomain",
        capability: "AccountWorkflowCapability",
        action: "DepositUnlockedAccount",
      },
    );

    expect(Object.keys(scaffold.files).sort()).toContain(
      "domains/AccountWorkflowDomain/entities/Account.sophia",
    );
    expect(scaffold.files["domains/AccountWorkflowDomain/entities/Account.sophia"]).toContain(
      "balance: Int",
    );
    expect(scaffold.files["domains/AccountWorkflowDomain/entities/Account.sophia"]).toContain(
      "is_locked: Bool",
    );
    expect(
      scaffold.files["domains/AccountWorkflowDomain/actions/DepositUnlockedAccount.sophia"],
    ).toContain("account: Account");
  });

  it("uses public state scaffold contracts without filling match body", () => {
    const scaffold = buildSophiaScaffold(
      `
program StateStatusLabel {
  purpose { "Return a label for a semantic state." }
  inputs { status := "current semantic state" }
  outputs { result := "text label" }
  effects { none }
  algorithm {
    if status means the first state then return its label
    otherwise return the other label
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

    expect(Object.keys(scaffold.files).sort()).toContain(
      "domains/StateMatchDomain/states/TaskStatus.sophia",
    );
    expect(scaffold.files["domains/StateMatchDomain/states/TaskStatus.sophia"]).toContain(
      "value Pending",
    );
    const action = scaffold.files["domains/StateMatchDomain/actions/StateStatusLabel.sophia"];
    expect(action).toContain("status: TaskStatus");
    expect(action).toContain("result: Text");
    expect(action).toContain("[TODO: LLM-fill from pseudo.algorithm]");
    expect(action).not.toContain("match status");
    expect(action).not.toContain("TaskStatus.Pending");
  });

  it("does not infer record-like main action contracts from prose descriptions", () => {
    const scaffold = buildSophiaScaffold(`
program ValidateAndApplyDelta {
  purpose { "Update an item if validation passes." }
  entities {
    Item {
      value := "integer"
      is_active := "boolean"
    }
  }
  inputs {
    item := "record-like entity with value and is_active fields"
    delta := "integer"
  }
  outputs {
    result := "record-like entity of type Item representing the updated or original item"
  }
  effects { none }
  algorithm {
    subaction ValidateDelta { return allowed }
    subaction ApplyDelta { return updated item }
    main_flow ValidateAndApplyDelta { return result }
  }
}
`,
      {
        domain: "ItemOperations",
        capability: "RecordValidation",
        action: "ValidateAndApplyDelta",
      },
    );

    const action = scaffold.files["domains/ItemOperations/actions/ValidateAndApplyDelta.sophia"];
    expect(action).not.toContain("item: Item");
    expect(action).not.toContain("delta: Int");
    expect(action).toContain("result: Unit");
  });
});
