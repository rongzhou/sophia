import { describe, expect, it } from "vitest";
import {
  collectSophiaExpressionIdentifiers,
  inferSophiaExpressionType,
  parseEntityAssignments,
} from "../../src/lang/ast/expression.js";

describe("Sophia v0 expressions", () => {
  it("infers scalar, list, arithmetic, append, and concat expression types", () => {
    const types = new Map<string, string>([
      ["count", "Int"],
      ["limit", "Int"],
      ["is_ready", "Bool"],
      ["is_blocked", "Bool"],
      ["numbers", "List<Int>"],
      ["label", "Text"],
      ["labels", "List<Text>"],
    ]);

    expect(inferSophiaExpressionType("unit", types)).toBe("Unit");
    expect(inferSophiaExpressionType("None", types)).toBe("None");
    expect(inferSophiaExpressionType("Some(count)", types)).toBe("Optional<Int>");
    expect(inferSophiaExpressionType("true", types)).toBe("Bool");
    expect(inferSophiaExpressionType("count > 0", types)).toBe("Bool");
    expect(inferSophiaExpressionType("count <= limit", types)).toBe("Bool");
    expect(inferSophiaExpressionType('label == "ready"', types)).toBe("Bool");
    expect(inferSophiaExpressionType("is_ready and not is_blocked", types)).toBe("Bool");
    expect(inferSophiaExpressionType("count * 2", types)).toBe("Int");
    expect(inferSophiaExpressionType("to_text(count)", types)).toBe("Text");
    expect(inferSophiaExpressionType("to_text(label)", types)).toBe(null);
    expect(inferSophiaExpressionType('"ready"', types)).toBe("Text");
    expect(inferSophiaExpressionType('label + " ready"', types)).toBe("Text");
    expect(inferSophiaExpressionType('"status: " + label', types)).toBe("Text");
    expect(inferSophiaExpressionType("label + count", types)).toBe(null);
    expect(inferSophiaExpressionType("[1, 2, 3]", types)).toBe("List<Int>");
    expect(inferSophiaExpressionType('["a", "b"]', types)).toBe("List<Text>");
    expect(inferSophiaExpressionType("numbers.append(count)", types)).toBe("List<Int>");
    expect(inferSophiaExpressionType("labels + [label]", types)).toBe("List<Text>");
  });

  it("infers action call result types from action signatures", () => {
    const types = new Map<string, string>([
      ["count", "Int"],
      ["label", "Text"],
    ]);
    const actions = new Map([
      [
        "DoubleInput",
        {
          name: "DoubleInput",
          input: [{ name: "count", type: "Int" }],
          outputType: "Int",
          effects: new Set<string>(),
          errors: new Set<string>(),
        },
      ],
    ]);

    expect(
      inferSophiaExpressionType("DoubleInput { count = count }", types, new Map(), actions),
    ).toBe("Int");
    expect(
      inferSophiaExpressionType("DoubleInput { count = label }", types, new Map(), actions),
    ).toBe(null);
  });

  it("infers declared state value expression types", () => {
    const stateTypes = new Map([["TodoStatus", ["Pending", "Done"]]]);

    expect(
      inferSophiaExpressionType("TodoStatus.Done", new Map(), new Map(), new Map(), stateTypes),
    ).toBe("TodoStatus");
    expect(
      inferSophiaExpressionType("TodoStatus.Missing", new Map(), new Map(), new Map(), stateTypes),
    ).toBe(null);
  });

  it("propagates intent wrappers through scalar expressions", () => {
    const types = new Map<string, string>([
      ["raw_title", "Raw<Text>"],
      ["safe_suffix", "Sanitized<Text>"],
      ["safe_title", "Sanitized<Text>"],
      ["raw_count", "Raw<Int>"],
      ["count", "Int"],
    ]);

    expect(inferSophiaExpressionType('raw_title + "!"', types)).toBe("Raw<Text>");
    expect(inferSophiaExpressionType("safe_title + safe_suffix", types)).toBe("Sanitized<Text>");
    expect(inferSophiaExpressionType("raw_count + count", types)).toBe("Raw<Int>");
    expect(inferSophiaExpressionType("raw_count > count", types)).toBe("Bool");
    expect(inferSophiaExpressionType("safe_title + raw_title", types)).toBe("Raw<Text>");
  });

  it("extracts only variable identifiers from v0 expressions", () => {
    expect(collectSophiaExpressionIdentifiers("numbers.append(count + 1)")).toEqual([
      "numbers",
      "count",
    ]);
    expect(collectSophiaExpressionIdentifiers("to_text(count)")).toEqual(["count"]);
    expect(collectSophiaExpressionIdentifiers('"count" + label')).toEqual(["label"]);
    expect(collectSophiaExpressionIdentifiers("true and not ready")).toEqual(["ready"]);
  });

  it("parses top-level assignments without splitting nested expressions", () => {
    const assignments = parseEntityAssignments(
      'owner = Owner { name = "A, B", age = age }, labels = labels + ["x, y"]',
    );

    expect(assignments).toEqual(
      new Map([
        ["owner", 'Owner { name = "A, B", age = age }'],
        ["labels", 'labels + ["x, y"]'],
      ]),
    );
  });

  it("infers entity construction types with nested assignment expressions", () => {
    const types = new Map<string, string>([
      ["age", "Int"],
      ["labels", "List<Text>"],
    ]);
    const entityTypes = new Map([
      [
        "Owner",
        [
          { name: "name", type: "Text" },
          { name: "age", type: "Int" },
        ],
      ],
      [
        "Account",
        [
          { name: "owner", type: "Owner" },
          { name: "labels", type: "List<Text>" },
        ],
      ],
    ]);

    expect(
      inferSophiaExpressionType(
        'Account { owner = Owner { name = "A, B", age = age }, labels = labels + ["x, y"] }',
        types,
        entityTypes,
      ),
    ).toBe("Account");
  });
});
