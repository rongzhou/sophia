import { readFile } from "node:fs/promises";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { buildTypeScript } from "../../src/backend/ts_codegen.js";
import {
  createSophiaWorkspace,
  createSophiaWorkspaceWithDemoDomain,
  writeDemoDomain,
  writeProjectFile,
  writePureCapability,
  writeSophiaToml,
} from "../helpers/sophia_workspace.js";

describe("buildTypeScript", () => {
  it("builds deterministic TypeScript for checked v0 action files", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
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
    let mutable doubled = count * 2
    return doubled
  }
}
`,
    );

    const first = await buildTypeScript(root);
    const firstOutput = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");
    const second = await buildTypeScript(root);
    const secondOutput = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(first.ok).toBe(true);
    expect(second.ok).toBe(true);
    expect(first.files).toEqual(["sophia-runs/build/index.ts"]);
    expect(firstOutput).toBe(secondOutput);
    expect(firstOutput).toContain("export function DoubleInput");
    expect(firstOutput).toContain("export const actions =");
    expect(firstOutput).toContain('"input": [');
    expect(firstOutput).toContain('"name": "count"');
    expect(firstOutput).toContain('"type": "Int"');
    expect(firstOutput).toContain("let doubled = count * 2;");
    expect(firstOutput).toContain("return doubled;");
  });

  it("emits explicit inferred array types for empty list initialization", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/ListDemo.sophia",
      `
action ListDemo {
  capability: PureCapability
  output { result: List<Int> }
  effects { }
  body {
    let mutable numbers = []
    return numbers
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("let numbers: number[] = [];");
  });

  it("emits explicit Int-to-Text conversion", async () => {
    const root = await createSophiaWorkspace("sophia-build-");
    await writeProjectFile(root, "domains/Demo/domain.sophia", "domain Demo {}\n");
    await writeProjectFile(
      root,
      "domains/Demo/capabilities/ConsoleCapability.sophia",
      `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/PrintCount.sophia",
      `
action PrintCount {
  capability: ConsoleCapability
  input { count: Int }
  output { result: Unit }
  effects { Console.Write }
  body {
    print to_text(count)
    return unit
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("effects.write(String(String(count)));");
  });

  it("emits action calls as generated TypeScript function calls", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
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

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("let doubled = DoubleInput({ count: count }, effects);");
    expect(output).toContain("return DoubleInput({ count: doubled }, effects);");
  });

  it("emits error metadata and raise statements", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/errors/AccountError.sophia",
      `
error AccountError {
  variant InvalidAmount {
    amount: Int
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/ValidateAmount.sophia",
      `
action ValidateAmount {
  capability: PureCapability
  input { amount: Int }
  output { result: Int }
  effects { }
  errors { InvalidAmount }
  body {
    if amount <= 0 {
      raise InvalidAmount { amount = amount }
    }
    return amount
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("export const errors =");
    expect(output).toContain('"InvalidAmount"');
    expect(output).toContain('throw { kind: "InvalidAmount", amount: amount };');
    expect(output).toContain('"errors": [');
  });

  it("emits state metadata, TypeScript state constants, and state value expressions", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/states/TodoStatus.sophia",
      `
state TodoStatus {
  value Pending { }
  value Done { }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/ReturnDone.sophia",
      `
action ReturnDone {
  capability: PureCapability
  output { result: TodoStatus }
  effects { }
  body {
    return TodoStatus.Done
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("export const states =");
    expect(output).toContain("export const TodoStatus =");
    expect(output).toContain("export type TodoStatus =");
    expect(output).toContain("return TodoStatus.Done;");
  });

  it("emits match over state and Optional values", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/states/TodoStatus.sophia",
      `
state TodoStatus {
  value Pending { }
  value Done { }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/StatusLabel.sophia",
      `
action StatusLabel {
  capability: PureCapability
  input { status: TodoStatus }
  output { result: Text }
  effects { }
  body {
    match status {
      TodoStatus.Pending {
        return "pending"
      }
      TodoStatus.Done {
        return "done"
      }
    }
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/LabelOrDefault.sophia",
      `
action LabelOrDefault {
  capability: PureCapability
  input { label: Optional<Text> }
  output { result: Text }
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
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("const __match");
    expect(output).toContain("if (__match");
    expect(output).toContain("=== TodoStatus.Pending");
    expect(output).toContain("!== null");
    expect(output).toContain("const value = __match");
    expect(output).toContain('return "missing";');
  });

  it("refuses to build files that fail deterministic check", async () => {
    const root = await createSophiaWorkspace("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/Broken.sophia",
      `
action Broken {
  capability: PureCapability
  output { result: Unit }
  effects { }
  body {
    push value
    return unit
  }
}
`,
    );

    const result = await buildTypeScript(root);

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("BUILD-CHECK-001");
  });

  it("honors configured build out_dir", async () => {
    const root = await createSophiaWorkspace("sophia-build-");
    await writeSophiaToml(root, { buildOutDir: "out/build" });
    await writeDemoDomain(root);

    const result = await buildTypeScript(root);

    expect(result.ok).toBe(true);
    expect(result.files).toEqual(["out/build/index.ts"]);
    await expect(readFile(path.join(root, "out/build/index.ts"), "utf8")).resolves.toContain(
      "export const domains",
    );
  });

  it("keeps generated artifacts unchanged when semantic assist changes", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/StaticValue.sophia",
      `
action StaticValue {
  meaning: "First explanation."
  capability: PureCapability
  output { result: Int }
  effects { }
  body {
    return 7
  }
}
`,
    );

    const first = await buildTypeScript(root);
    const firstOutput = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");
    await writeProjectFile(
      root,
      "domains/Demo/actions/StaticValue.sophia",
      `
action StaticValue {
  meaning: "Second explanation with different words."
  capability: PureCapability
  output { result: Int }
  effects { }
  body {
    return 7
  }
}
`,
    );
    const second = await buildTypeScript(root);
    const secondOutput = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(first.ok).toBe(true);
    expect(second.ok).toBe(true);
    expect(secondOutput).toBe(firstOutput);
  });

  it("serializes concurrent writes to the generated TypeScript output", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/StaticValue.sophia",
      `
action StaticValue {
  capability: PureCapability
  output { result: Int }
  effects { }
  body {
    return 7
  }
}
`,
    );

    const results = await Promise.all([
      buildTypeScript(root),
      buildTypeScript(root),
      buildTypeScript(root),
      buildTypeScript(root),
    ]);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(results.every((result) => result.ok)).toBe(true);
    expect(output).toContain("export function StaticValue");
    expect(output).not.toContain(".tmp");
  });

  it("preserves if else branches in generated TypeScript", async () => {
    const root = await createSophiaWorkspace("sophia-build-");
    await writeDemoDomain(root);
    await writePureCapability(root);
    await writeProjectFile(
      root,
      "domains/Demo/actions/Label.sophia",
      `
action Label {
  capability: PureCapability
  input { count: Int }
  output { result: Text }
  effects { }
  body {
    if count == 0 {
      return "zero"
    } else {
      return "positive"
    }
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("if (count === 0) {");
    expect(output).not.toContain("if (count == 0) {");
    expect(output).toContain("} else {");
    expect(output).toContain('return "zero";');
    expect(output).toContain('return "positive";');
  });

  it("emits Text concatenation as TypeScript string addition", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
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
    let combined = left + right
    return "value: " + combined
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("let combined = left + right;");
    expect(output).toContain('return "value: " + combined;');
  });

  it("emits strict inequality for Sophia not-equal comparisons", async () => {
    const root = await createSophiaWorkspace("sophia-build-");
    await writeDemoDomain(root);
    await writePureCapability(root);
    await writeProjectFile(
      root,
      "domains/Demo/actions/Label.sophia",
      `
action Label {
  capability: PureCapability
  input { count: Int }
  output { result: Text }
  effects { }
  body {
    if count != 0 {
      return "nonzero"
    } else {
      return "zero"
    }
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("if (count !== 0) {");
    expect(output).not.toContain("if (count != 0) {");
  });

  it("infers empty list type from nested arithmetic appends", async () => {
    const root = await createSophiaWorkspace("sophia-build-");
    await writeDemoDomain(root);
    await writeProjectFile(
      root,
      "domains/Demo/capabilities/ConsoleCapability.sophia",
      "capability ConsoleCapability { allow { Console.Write } }\n",
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/FilterAndDouble.sophia",
      `
action FilterAndDouble {
  capability: ConsoleCapability
  input {
    first: Int
    second: Int
  }
  output {
    result: List<Int>
  }
  effects {
    Console.Write
  }
  body {
    let mutable result = []
    let mutable count = 0
    if first > 0 {
      set result = result + [first * 2]
      set count = count + 1
    }
    if second > 0 {
      set result = result + [second * 2]
      set count = count + 1
    }
    if count == 0 {
      print "empty"
    }
    return result
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("let result: number[] = [];");
  });

  it("learns intervening variable types when inferring empty list appends", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/Countdown.sophia",
      `
action Countdown {
  capability: PureCapability
  output { numbers: List<Int> }
  effects { }
  body {
    let mutable numbers = []
    let mutable current = 5
    repeat 5 times {
      set numbers = numbers + [current]
      set current = current - 1
    }
    return numbers
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("let numbers: number[] = [];");
    expect(output).toContain("numbers = numbers.concat([current]);");
  });

  it("emits Bool outputs and boolean operator expressions", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/IsSmallPositive.sophia",
      `
action IsSmallPositive {
  capability: PureCapability
  input { count: Int }
  output { result: Bool }
  effects { }
  body {
    let is_positive = count > 0
    let is_small = count <= 10
    if is_positive and not is_small {
      return false
    } else {
      return is_positive and is_small
    }
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("): boolean {");
    expect(output).toContain("let is_positive = count > 0;");
    expect(output).toContain("if (is_positive && !is_small) {");
    expect(output).toContain("return is_positive && is_small;");
  });

  it("emits entity interfaces and object construction", async () => {
    const root = await createSophiaWorkspace("sophia-build-");
    await writeDemoDomain(root, "AccountDomain");
    await writePureCapability(root, "AccountDomain");
    await writeProjectFile(
      root,
      "domains/AccountDomain/entities/Account.sophia",
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
      "domains/AccountDomain/actions/Deposit.sophia",
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
    let updated = Account { balance = account.balance + amount, is_locked = account.is_locked }
    return updated
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("export const entities =");
    expect(output).toContain("export interface Account");
    expect(output).toContain("balance: number;");
    expect(output).toContain("is_locked: boolean;");
    expect(output).toContain("account: Account; amount: number");
    expect(output).toContain(
      "let updated = { balance: account.balance + amount, is_locked: account.is_locked };",
    );
    expect(output).toContain("): Account {");
  });

  it("emits Optional fields and Some/None expressions as nullable TypeScript", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-build-");
    await writeProjectFile(
      root,
      "domains/Demo/entities/MaybeLabel.sophia",
      `
entity MaybeLabel {
  fields {
    label: Optional<Text>
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/ReturnNone.sophia",
      `
action ReturnNone {
  capability: PureCapability
  output { result: Optional<Text> }
  effects { }
  body {
    return None
  }
}
`,
    );
    await writeProjectFile(
      root,
      "domains/Demo/actions/WrapLabel.sophia",
      `
action WrapLabel {
  capability: PureCapability
  input { label: Text }
  output { result: MaybeLabel }
  effects { }
  body {
    return MaybeLabel { label = Some(label) }
  }
}
`,
    );

    const result = await buildTypeScript(root);
    const output = await readFile(path.join(root, "sophia-runs/build/index.ts"), "utf8");

    expect(result.ok).toBe(true);
    expect(output).toContain("label: string | null;");
    expect(output).toContain("): string | null {");
    expect(output).toContain("return null;");
    expect(output).toContain("return { label: label };");
  });
});
