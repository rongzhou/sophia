import { describe, expect, it } from "vitest";
import { buildSophiaScaffold } from "../../src/pseudo/scaffold.js";
import { samplePseudocodeJson } from "../helpers/sophia_workspace.js";

describe("buildSophiaScaffold", () => {
  it("generates deterministic Sophia structure from JSON pseudocode without filling business body", () => {
    const scaffold = buildSophiaScaffold(
      samplePseudocodeJson({
        program_name: "PrintNumbers",
        purpose: "Print and return numbers.",
        outputs: [{ name: "numbers", meaning: "List<Int>" }],
        effects: ["Console.Write"],
        algorithm: [
          "create empty list numbers",
          "repeat 3 times, append current to numbers and print current",
          "return numbers",
        ],
      }),
      { output: { name: "numbers", type: "List<Int>" }, effects: ["Console.Write"] },
    );

    expect(Object.keys(scaffold.files).sort()).toEqual([
      "domains/PrintNumbersDomain/actions/PrintNumbers.sophia",
      "domains/PrintNumbersDomain/capabilities/PrintNumbersCapability.sophia",
      "domains/PrintNumbersDomain/domain.sophia",
    ]);
    const action = scaffold.files["domains/PrintNumbersDomain/actions/PrintNumbers.sophia"];
    expect(action).toContain("capability: PrintNumbersCapability");
    expect(action).toContain("numbers: List<Int>");
    expect(action).toContain("Console.Write");
    expect(action).toContain("[TODO: LLM-fill from pseudo.algorithm]");
    expect(action).not.toContain("repeat 3 times");
  });

  it("uses explicit contract hints from JSON descriptions when they are formal", () => {
    const scaffold = buildSophiaScaffold(
      samplePseudocodeJson({
        program_name: "AddItem",
        purpose: "Add text to a list.",
        inputs: [
          { name: "items", meaning: "List<Text>" },
          { name: "text", meaning: "Text" },
        ],
        outputs: [{ name: "updated_items", meaning: "List<Text>" }],
        algorithm: ["append text to items", "return items"],
      }),
    );

    const action = scaffold.files["domains/AddItemDomain/actions/AddItem.sophia"];
    expect(action).toContain("items: List<Text>");
    expect(action).toContain("text: Text");
    expect(action).toContain("updated_items: List<Text>");
    expect(action).not.toContain("set items = items + [text]");
  });

  it("uses public structure overrides for names and paths without filling the body", () => {
    const scaffold = buildSophiaScaffold(
      samplePseudocodeJson({
        program_name: "PrintNumbers",
        purpose: "Print and return numbers.",
        outputs: [{ name: "numbers", meaning: "List<Int>" }],
        effects: ["print each number"],
        algorithm: ["create empty list numbers", "return numbers"],
      }),
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

  it("generates entity scaffold files from explicit JSON definitions", () => {
    const scaffold = buildSophiaScaffold(
      samplePseudocodeJson({
        program_name: "DepositUnlockedAccount",
        purpose: "Deposit into an account when it is unlocked.",
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
        algorithm: ["return Account with updated balance"],
      }),
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
    expect(
      scaffold.files["domains/AccountWorkflowDomain/actions/DepositUnlockedAccount.sophia"],
    ).toContain("account: Account");
  });

  it("uses public state scaffold contracts without filling match body", () => {
    const scaffold = buildSophiaScaffold(
      samplePseudocodeJson({
        program_name: "StateStatusLabel",
        purpose: "Return a label for a semantic state.",
        inputs: [{ name: "status", meaning: "current semantic state" }],
        outputs: [{ name: "result", meaning: "text label" }],
        algorithm: ["if status means the first state then return its label"],
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

    expect(Object.keys(scaffold.files).sort()).toContain(
      "domains/StateMatchDomain/states/TaskStatus.sophia",
    );
    const action = scaffold.files["domains/StateMatchDomain/actions/StateStatusLabel.sophia"];
    expect(action).toContain("status: TaskStatus");
    expect(action).toContain("result: Text");
    expect(action).toContain("[TODO: LLM-fill from pseudo.algorithm]");
    expect(action).not.toContain("match status");
  });

  it("does not infer record-like main action contracts from prose descriptions", () => {
    const scaffold = buildSophiaScaffold(
      samplePseudocodeJson({
        program_name: "ValidateAndApplyDelta",
        purpose: "Update an item if validation passes.",
        definitions: [{ name: "Item", meaning: "record-like item" }],
        inputs: [
          { name: "item", meaning: "record-like entity with value and is_active fields" },
          { name: "delta", meaning: "integer" },
        ],
        outputs: [{ name: "result", meaning: "record-like entity of type Item" }],
        algorithm: [
          "ValidateDelta: return allowed.",
          "ApplyDelta: return updated item.",
          "ValidateAndApplyDelta: return result.",
        ],
      }),
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
