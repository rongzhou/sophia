import { describe, expect, it } from "vitest";
import { runTypeScriptAction, smokeTypeScriptActions } from "../../src/backend/ts_runner.js";
import {
  createSophiaWorkspaceWithDemoDomain,
  writeProjectFile,
} from "../helpers/sophia_workspace.js";

function runDiagnostic(code: string, problem: string): Array<Record<string, string>> {
  return [
    {
      code,
      severity: "error",
      location: "sophia-runs/build/index.ts",
      problem,
    },
  ];
}

describe("runTypeScriptAction", () => {
  it("builds and runs a generated pure action", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    let doubled = count * 2
    return doubled
  }
}
`,
    );

    const result = await runTypeScriptAction(root, "DoubleInput", { count: 7 });

    expect(result.ok).toBe(true);
    expect(result.result).toBe(14);
    expect(result.effects).toEqual([]);
  });

  it("builds and runs generated Text concatenation", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/ConcatText.sophia",
      `
action ConcatText {
  capability: PureCapability
  input { left: Text right: Text }
  output { result: Text }
  effects { }
  body {
    return left + right
  }
}
`,
    );

    const result = await runTypeScriptAction(root, "ConcatText", {
      left: "hello",
      right: "world",
    });

    expect(result.ok).toBe(true);
    expect(result.result).toBe("helloworld");
    expect(result.effects).toEqual([]);
  });

  it("builds and runs generated action calls", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/QuadrupleInput.sophia",
      `
action QuadrupleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    let doubled = DoubleInput { count = count }
    return DoubleInput { count = doubled }
  }
}
`,
    );

    const result = await runTypeScriptAction(root, "QuadrupleInput", { count: 7 });

    expect(result.ok).toBe(true);
    expect(result.result).toBe(28);
    expect(result.effects).toEqual([]);
  });

  it("runs an entity-based action pipeline with interface-checked calls", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/entities/PipelineAccount.sophia",
      `
entity PipelineAccount {
  fields {
    balance: Int
    is_locked: Bool
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/CanDepositPipeline.sophia",
      `
action CanDepositPipeline {
  capability: PureCapability
  input {
    account: PipelineAccount
    amount: Int
  }
  output {
    result: Bool
  }
  effects { }
  body {
    let unlocked = not account.is_locked
    let positive_amount = amount > 0
    return unlocked and positive_amount
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/ApplyDepositPipeline.sophia",
      `
action ApplyDepositPipeline {
  capability: PureCapability
  input {
    account: PipelineAccount
    amount: Int
  }
  output {
    result: PipelineAccount
  }
  effects { }
  body {
    let updated_balance = account.balance + amount
    return PipelineAccount { balance = updated_balance, is_locked = account.is_locked }
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/ProcessDepositPipeline.sophia",
      `
action ProcessDepositPipeline {
  capability: PureCapability
  input {
    account: PipelineAccount
    amount: Int
  }
  output {
    result: PipelineAccount
  }
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
    );

    const accepted = await runTypeScriptAction(root, "ProcessDepositPipeline", {
      account: { balance: 10, is_locked: false },
      amount: 7,
    });
    const locked = await runTypeScriptAction(root, "ProcessDepositPipeline", {
      account: { balance: 10, is_locked: true },
      amount: 7,
    });
    const invalidAmount = await runTypeScriptAction(root, "ProcessDepositPipeline", {
      account: { balance: 10, is_locked: false },
      amount: 0,
    });

    expect(accepted.ok).toBe(true);
    expect(accepted.result).toEqual({ balance: 17, is_locked: false });
    expect(locked.ok).toBe(true);
    expect(locked.result).toEqual({ balance: 10, is_locked: true });
    expect(invalidAmount.ok).toBe(true);
    expect(invalidAmount.result).toEqual({ balance: 10, is_locked: false });
  });

  it("captures generated action effects without using console output", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/capabilities/ConsoleCapability.sophia",
      "capability ConsoleCapability { allow { Console.Write } }\n",
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/PrintLabel.sophia",
      `
action PrintLabel {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body {
    print "ready"
    return unit
  }
}
`,
    );

    const result = await runTypeScriptAction(root, "PrintLabel", {});

    expect(result.ok).toBe(true);
    expect(result.result).toBeNull();
    expect(result.effects).toEqual(["ready"]);
  });

  it("fails when the generated build does not export the requested action", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");

    const result = await runTypeScriptAction(root, "MissingAction", {});

    expect(result.ok).toBe(false);
    expect(result.diagnostics).toEqual(
      runDiagnostic(
        "RUN-ACTION-001",
        "Generated build does not export action metadata for MissingAction.",
      ),
    );
  });

  it("validates generated action input before execution", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );

    const result = await runTypeScriptAction(root, "DoubleInput", { count: "7" });

    expect(result.ok).toBe(false);
    expect(result.diagnostics).toEqual(
      runDiagnostic("RUN-INPUT-004", "Input field count for DoubleInput must be Int."),
    );
  });

  it("validates Bool input and output at runtime", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/AllowWhenReady.sophia",
      `
action AllowWhenReady {
  capability: PureCapability
  input { ready: Bool blocked: Bool }
  output { result: Bool }
  effects { }
  body {
    return ready and not blocked
  }
}
`,
    );

    const passed = await runTypeScriptAction(root, "AllowWhenReady", {
      ready: true,
      blocked: false,
    });
    const failed = await runTypeScriptAction(root, "AllowWhenReady", {
      ready: "true",
      blocked: false,
    });

    expect(passed.ok).toBe(true);
    expect(passed.result).toBe(true);
    expect(failed.ok).toBe(false);
    expect(failed.diagnostics).toContainEqual(
      expect.objectContaining({
        code: "RUN-INPUT-004",
        problem: "Input field ready for AllowWhenReady must be Bool.",
      }),
    );
  });

  it("validates entity input and output at runtime", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/entities/Account.sophia",
      `
entity Account {
  fields {
    balance: Int
    is_locked: Bool
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/Deposit.sophia",
      `
action Deposit {
  capability: PureCapability
  input {
    account: Account
    amount: Int
  }
  output { result: Account }
  effects { }
  body {
    return Account { balance = account.balance + amount, is_locked = account.is_locked }
  }
}
`,
    );

    const passed = await runTypeScriptAction(root, "Deposit", {
      account: { balance: 10, is_locked: false },
      amount: 7,
    });
    const failed = await runTypeScriptAction(root, "Deposit", {
      account: { balance: "10", is_locked: false },
      amount: 7,
    });

    expect(passed.ok).toBe(true);
    expect(passed.result).toEqual({ balance: 17, is_locked: false });
    expect(failed.ok).toBe(false);
    expect(failed.diagnostics).toContainEqual(
      expect.objectContaining({
        code: "RUN-INPUT-004",
        problem: "Input field account for Deposit must be Account.",
      }),
    );
  });

  it("rejects non-object input before execution", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );

    const result = await runTypeScriptAction(root, "DoubleInput", 7);

    expect(result.ok).toBe(false);
    expect(result.diagnostics).toEqual(
      runDiagnostic("RUN-INPUT-001", "Input for DoubleInput must be a JSON object."),
    );
  });

  it("rejects missing and unknown input fields before execution", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );

    const missing = await runTypeScriptAction(root, "DoubleInput", {});
    const unknown = await runTypeScriptAction(root, "DoubleInput", { count: 7, extra: 1 });

    expect(missing.ok).toBe(false);
    expect(missing.diagnostics).toEqual(
      runDiagnostic("RUN-INPUT-003", "Input for DoubleInput is missing required field count."),
    );
    expect(unknown.ok).toBe(false);
    expect(unknown.diagnostics).toEqual(
      runDiagnostic("RUN-INPUT-002", "Input for DoubleInput contains unknown field extra."),
    );
  });

  it("validates list inputs and outputs", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/AddTextItem.sophia",
      `
action AddTextItem {
  capability: PureCapability
  input { items: List<Text> text: Text }
  output { result: List<Text> }
  effects { }
  body {
    let mutable updated_items = items
    set updated_items = updated_items.append(text)
    return updated_items
  }
}
`,
    );

    const passed = await runTypeScriptAction(root, "AddTextItem", { items: ["a"], text: "b" });
    const failed = await runTypeScriptAction(root, "AddTextItem", { items: [1], text: "b" });

    expect(passed.ok).toBe(true);
    expect(passed.result).toEqual(["a", "b"]);
    expect(failed.ok).toBe(false);
    expect(failed.diagnostics).toEqual(
      runDiagnostic("RUN-INPUT-004", "Input field items for AddTextItem must be List<Text>."),
    );
  });

  it("validates generated action output after execution", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-run-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/BrokenOutput.sophia",
      `
action BrokenOutput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count / 2
  }
}
`,
    );

    const result = await runTypeScriptAction(root, "BrokenOutput", { count: 3 });

    expect(result.ok).toBe(false);
    expect(result.result).toBe(1.5);
    expect(result.diagnostics).toEqual(
      runDiagnostic("RUN-OUTPUT-001", "Result for BrokenOutput must be Int."),
    );
  });
});

describe("smokeTypeScriptActions", () => {
  it("runs generated zero-input actions and skips actions that require input", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-smoke-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/SumThree.sophia",
      `
action SumThree {
  capability: PureCapability
  output { result: Int }
  effects { }
  body {
    let mutable total = 0
    let mutable current = 1
    repeat 3 times {
      set total = total + current
      set current = current + 1
    }
    return total
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );

    const result = await smokeTypeScriptActions(root);

    expect(result.ok).toBe(true);
    expect(result.actions_run).toBe(1);
    expect(result.actions_skipped).toBe(1);
    expect(result.actions).toContainEqual(
      expect.objectContaining({
        action: "SumThree",
        ok: true,
        skipped: false,
        result: 6,
      }),
    );
    expect(result.actions).toContainEqual(
      expect.objectContaining({
        action: "DoubleInput",
        ok: true,
        skipped: true,
        reason: "requires_input",
      }),
    );
  });

  it("runs input actions when smoke sample inputs are provided", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-smoke-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );

    const result = await smokeTypeScriptActions(root, {
      inputs: { DoubleInput: { count: 9 } },
    });

    expect(result.ok).toBe(true);
    expect(result.actions_run).toBe(1);
    expect(result.actions_skipped).toBe(0);
    expect(result.actions).toContainEqual(
      expect.objectContaining({
        action: "DoubleInput",
        ok: true,
        skipped: false,
        result: 18,
      }),
    );
  });

  it("can generate generic type-valid smoke inputs without expected outputs", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-smoke-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );

    const result = await smokeTypeScriptActions(root, { autoInputs: true });

    expect(result.ok).toBe(true);
    expect(result.actions_run).toBe(1);
    expect(result.actions_skipped).toBe(0);
    expect(result.actions).toContainEqual(
      expect.objectContaining({
        action: "DoubleInput",
        ok: true,
        skipped: false,
        result: 0,
      }),
    );
  });

  it("prefers explicit smoke inputs over generic generated inputs", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-smoke-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );

    const result = await smokeTypeScriptActions(root, {
      autoInputs: true,
      inputs: { DoubleInput: { count: 5 } },
    });

    expect(result.ok).toBe(true);
    expect(result.actions).toContainEqual(
      expect.objectContaining({
        action: "DoubleInput",
        result: 10,
      }),
    );
  });

  it("reports invalid smoke sample inputs without executing the action", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-smoke-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );

    const result = await smokeTypeScriptActions(root, {
      inputs: { DoubleInput: { count: "9" } },
    });

    expect(result.ok).toBe(false);
    expect(result.actions_run).toBe(1);
    expect(result.diagnostics).toEqual(
      runDiagnostic("RUN-INPUT-004", "Input field count for DoubleInput must be Int."),
    );
  });

  it("reports runtime validation failures from zero-input actions", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-smoke-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/BrokenOutput.sophia",
      `
action BrokenOutput {
  capability: PureCapability
  output { result: Int }
  effects { }
  body {
    let value = 1
    return value / 2
  }
}
`,
    );

    const result = await smokeTypeScriptActions(root);

    expect(result.ok).toBe(false);
    expect(result.actions_run).toBe(1);
    expect(result.diagnostics).toEqual(
      runDiagnostic("RUN-OUTPUT-001", "Result for BrokenOutput must be Int."),
    );
  });
});
