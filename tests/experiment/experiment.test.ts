import { describe, expect, it } from "vitest";
import {
  buildDirectTsPrompt,
  validateDirectTsOutput,
  verifyDirectTsCodeAgainstTask,
} from "../../src/experiment/direct_ts_runner.js";
import { buildPublicGoalForTask } from "../../src/experiment/public_goal.js";
import { loadBenchmarkSuite, loadBenchmarkTask } from "../../src/experiment/task.js";
import { verifySophiaFilesAgainstTask } from "../../src/experiment/verify.js";

describe("benchmark experiments", () => {
  it("loads category A tasks without placing hidden verifier values in the public goal", async () => {
    const tasks = await loadBenchmarkSuite("benchmarks/category_a");

    expect(tasks.map((task) => task.id)).toEqual(["account_pipeline", "item_delta_pipeline"]);
    expectPublicGoalsDoNotLeakHiddenCases(tasks, { checkCaseNames: true });
  });

  it("loads the paper-level benchmark suite without placing hidden verifier values in the public goal", async () => {
    const tasks = await loadBenchmarkSuite("benchmarks");

    expect(tasks).toHaveLength(16);
    expect(tasks.map((task) => task.id)).toContain("return_constant");
    expect(tasks.map((task) => task.id)).toContain("rabbit_ten");
    expect(tasks.map((task) => task.id)).toContain("optional_label_default");
    expect(tasks.map((task) => task.id)).toContain("state_status_label");
    expect(tasks.map((task) => task.id)).toContain("zero_or_positive_label");
    expectPublicGoalsDoNotLeakHiddenCases(tasks, { checkCaseNames: false });
  });

  it("keeps formal scaffold contracts out of the public design goal", async () => {
    const task = await loadBenchmarkTask("benchmarks/L3/state_status_label/task.json");
    const publicText = buildPublicGoalForTask(task);

    expect(publicText).toContain("declared state named TaskStatus containing values Pending and Done");
    expect(publicText).toContain("input named status");
    expect(publicText).not.toContain("Public state contract:");
    expect(publicText).not.toContain("status: TaskStatus");
    expect(publicText).not.toContain("result: Text");
    expect(publicText).not.toContain("TaskStatus.Pending");
    expect(publicText).not.toContain(JSON.stringify(task.hidden_cases[0]?.input));
    expect(publicText).not.toContain(JSON.stringify(task.hidden_cases[0]?.expected_result));
  });

  function expectPublicGoalsDoNotLeakHiddenCases(
    tasks: Awaited<ReturnType<typeof loadBenchmarkSuite>>,
    options: { checkCaseNames: boolean },
  ): void {
    for (const task of tasks) {
      const publicText = buildPublicGoalForTask(task);
      expect(publicText).not.toContain(`Program: ${task.scaffold.program}`);
      expect(publicText).not.toContain(`Domain: ${task.scaffold.domain}`);
      expect(publicText).not.toContain(`Main action: ${task.scaffold.action}`);
      expect(publicText).not.toContain(`Capability: ${task.scaffold.capability}`);
      expect(publicText).not.toMatch(/\b[A-Za-z_]\w*\s*:\s*(?:Int|Text|Bool|Unit|List<|Optional<)/);
      expect(publicText).not.toContain("Console.Write");
      expect(publicText).not.toContain("scaffold");
      for (const testCase of task.hidden_cases) {
        expect(publicText).not.toContain(JSON.stringify(testCase.input));
        if (isCompositeVerifierValue(testCase.expected_result)) {
          expect(publicText).not.toContain(JSON.stringify(testCase.expected_result));
        }
        if (options.checkCaseNames) {
          expect(publicText).not.toContain(testCase.name);
        }
      }
    }
  }

  function isCompositeVerifierValue(value: unknown): boolean {
    return typeof value === "object" && value !== null;
  }

  it("builds a Direct-TS baseline prompt without hidden verifier values", async () => {
    const task = await loadBenchmarkTask("benchmarks/category_a/item_delta_pipeline/task.json");
    const prompt = buildDirectTsPrompt(task);

    expect(prompt).toContain("export function runAction");
    expect(prompt).toContain("For pure return-only tasks, do not call effects.write.");
    expect(prompt).toContain(task.prompt_goal);
    for (const testCase of task.hidden_cases) {
      expect(prompt).not.toContain(JSON.stringify(testCase.input));
      expect(prompt).not.toContain(JSON.stringify(testCase.expected_result));
      expect(prompt).not.toContain(testCase.name);
    }
  });

  it("verifies a Direct-TS candidate against hidden benchmark cases", async () => {
    const task = await loadBenchmarkTask("benchmarks/category_a/item_delta_pipeline/task.json");
    const result = await verifyDirectTsCodeAgainstTask(
      `
export function runAction(input: unknown, effects: { write(value: string): void }): unknown {
  void effects;
  const data = input as { item: { value: number; is_active: boolean }; delta: number };
  if (data.item.is_active && data.delta > 0) {
    return { value: data.item.value + data.delta, is_active: data.item.is_active };
  }
  return data.item;
}
`,
      task,
    );

    expect(result.ok).toBe(true);
    expect(result.cases).toHaveLength(4);
  });

  it("rejects Direct-TS candidates that rely on forbidden ambient APIs", () => {
    expect(() =>
      validateDirectTsOutput({
        status: "written",
        code: `
export function runAction(input: unknown, effects: { write(value: string): void }): unknown {
  void input;
  void effects;
  return Date.now();
}
`,
        notes: [],
        questions: [],
        self_check: {
          exports_run_action: true,
          no_hidden_expected_outputs: true,
          no_tests_or_fixtures: true,
          generic_logic: true,
        },
      }),
    ).toThrow("forbidden ambient API");
  });

  it("verifies Sophia candidate files against hidden benchmark cases", async () => {
    const task = await loadBenchmarkTask("benchmarks/category_a/item_delta_pipeline/task.json");
    const result = await verifySophiaFilesAgainstTask(
      {
        "domains/ItemOperations/domain.sophia": "domain ItemOperations {}\n",
        "domains/ItemOperations/entities/Item.sophia": `
entity Item {
  fields {
    value: Int
    is_active: Bool
  }
}
`,
        "domains/ItemOperations/capabilities/RecordValidation.sophia": `
capability RecordValidation {
  allow { }
}
`,
        "domains/ItemOperations/actions/ValidateItemAndDelta.sophia": `
action ValidateItemAndDelta {
  input { item: Item delta: Int }
  output { result: Bool }
  capability: RecordValidation
  effects { }
  body {
    let is_active = item.is_active
    let has_positive_delta = delta > 0
    let can_apply = is_active and has_positive_delta
    return can_apply
  }
}
`,
        "domains/ItemOperations/actions/ApplyDelta.sophia": `
action ApplyDelta {
  input { item: Item delta: Int }
  output { result: Item }
  capability: RecordValidation
  effects { }
  body {
    let updated_value = item.value + delta
    let updated = Item { value = updated_value, is_active = item.is_active }
    return updated
  }
}
`,
        "domains/ItemOperations/actions/ValidateAndApplyDelta.sophia": `
action ValidateAndApplyDelta {
  input { item: Item delta: Int }
  output { result: Item }
  capability: RecordValidation
  effects { }
  body {
    let can_apply = ValidateItemAndDelta { item = item, delta = delta }
    if can_apply {
      let updated = ApplyDelta { item = item, delta = delta }
      return updated
    } else {
      return item
    }
  }
}
`,
      },
      task,
    );

    expect(result.ok).toBe(true);
    expect(result.cases).toHaveLength(4);
  });

  it("verifies Text concatenation benchmark candidates", async () => {
    const task = await loadBenchmarkTask("benchmarks/L1/concat_text/task.json");
    const result = await verifySophiaFilesAgainstTask(
      {
        "domains/TextDomain/domain.sophia": "domain TextDomain {}\n",
        "domains/TextDomain/capabilities/TextPureCapability.sophia": `
capability TextPureCapability {
  allow { }
}
`,
        "domains/TextDomain/actions/ConcatText.sophia": `
action ConcatText {
  input { left: Text right: Text }
  output { result: Text }
  capability: TextPureCapability
  effects { }
  body {
    return left + right
  }
}
`,
      },
      task,
    );

    expect(result.ok).toBe(true);
    expect(result.cases).toHaveLength(3);
  });

  it("verifies L3 match benchmark fixtures against hidden cases", async () => {
    const optionalTask = await loadBenchmarkTask("benchmarks/L3/optional_label_default/task.json");
    const optionalResult = await verifySophiaFilesAgainstTask(
      {
        "domains/OptionalMatchDomain/domain.sophia": "domain OptionalMatchDomain {}\n",
        "domains/OptionalMatchDomain/capabilities/OptionalPureCapability.sophia": `
capability OptionalPureCapability {
  allow { }
}
`,
        "domains/OptionalMatchDomain/actions/OptionalLabelDefault.sophia": `
action OptionalLabelDefault {
  input { label: Optional<Text> }
  output { result: Text }
  capability: OptionalPureCapability
  effects { }
  body {
    match label {
      Some(value) {
        return value
      }
      None {
        return "missing"
      }
    }
  }
}
`,
      },
      optionalTask,
    );

    const stateTask = await loadBenchmarkTask("benchmarks/L3/state_status_label/task.json");
    const stateResult = await verifySophiaFilesAgainstTask(
      {
        "domains/StateMatchDomain/domain.sophia": "domain StateMatchDomain {}\n",
        "domains/StateMatchDomain/states/TaskStatus.sophia": `
state TaskStatus {
  value Pending { }
  value Done { }
}
`,
        "domains/StateMatchDomain/capabilities/StatePureCapability.sophia": `
capability StatePureCapability {
  allow { }
}
`,
        "domains/StateMatchDomain/actions/StateStatusLabel.sophia": `
action StateStatusLabel {
  input { status: TaskStatus }
  output { result: Text }
  capability: StatePureCapability
  effects { }
  body {
    match status {
      TaskStatus.Pending {
        return "pending"
      }
      TaskStatus.Done {
        return "done"
      }
    }
  }
}
`,
      },
      stateTask,
    );

    expect(optionalResult.ok).toBe(true);
    expect(optionalResult.cases).toHaveLength(3);
    expect(stateResult.ok).toBe(true);
    expect(stateResult.cases).toHaveLength(2);
  });

  it("reports hidden verification failure without exposing expected values to repair", async () => {
    const task = await loadBenchmarkTask("benchmarks/category_a/account_pipeline/task.json");
    const result = await verifySophiaFilesAgainstTask(
      {
        "domains/ActionPipelineDomain/domain.sophia": "domain ActionPipelineDomain {}\n",
        "domains/ActionPipelineDomain/entities/PipelineAccount.sophia": `
entity PipelineAccount {
  fields {
    balance: Int
    is_locked: Bool
  }
}
`,
        "domains/ActionPipelineDomain/capabilities/ActionPipelinePureCapability.sophia": `
capability ActionPipelinePureCapability {
  allow { }
}
`,
        "domains/ActionPipelineDomain/actions/ProcessDepositPipeline.sophia": `
action ProcessDepositPipeline {
  input { account: PipelineAccount amount: Int }
  output { result: PipelineAccount }
  capability: ActionPipelinePureCapability
  effects { }
  body {
    return account
  }
}
`,
      },
      task,
    );

    expect(result.ok).toBe(false);
    expect(result.cases.some((testCase) => !testCase.ok)).toBe(true);
  });
});
