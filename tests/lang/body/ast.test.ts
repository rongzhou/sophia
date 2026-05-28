import { describe, expect, it } from "vitest";
import { flattenSophiaBodyStatements, parseSophiaBody } from "../../../src/lang/body/ast.js";

describe("parseSophiaBody", () => {
  it("parses the current v0 statement and block subset into a deterministic AST", () => {
    const result = parseSophiaBody(
      `
let mutable numbers = []
repeat 3 times {
  set numbers = numbers.append(1)
}
if count == 0 {
  print "zero"
} else {
  print "positive"
}
return numbers
`,
      "domains/Demo/actions/Demo.sophia",
    );

    expect(result.diagnostics).toEqual([]);
    expect(result.statements).toMatchObject([
      { kind: "let", mutable: true, name: "numbers", expression: "[]" },
      { kind: "repeat", count: 3, body: [{ kind: "set", name: "numbers" }] },
      {
        kind: "if",
        condition: "count == 0",
        thenBody: [{ kind: "print", expression: '"zero"' }],
        elseBody: [{ kind: "print", expression: '"positive"' }],
      },
      { kind: "return", expression: "numbers" },
    ]);
  });

  it("reports unsupported statements and block structure errors", () => {
    const result = parseSophiaBody(
      `
push numbers 1
repeat 1 times {
  print "x"
} else {
  print "y"
}
`,
      "domains/Demo/actions/Demo.sophia",
    );

    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-BODY-004", "CHECK-BLOCK-001"]),
    );
  });

  it("flattens nested statements in source order", () => {
    const result = parseSophiaBody(
      `
if ready {
  let label = "ok"
} else {
  return "wait"
}
return "done"
`,
      "domains/Demo/actions/Demo.sophia",
    );

    expect(
      flattenSophiaBodyStatements(result.statements).map((statement) => statement.kind),
    ).toEqual(["if", "let", "return", "return"]);
  });

  it("parses match cases and optional Some bindings", () => {
    const result = parseSophiaBody(
      `
match maybe_label {
  Some(label) {
    return label
  }
  None {
    return "missing"
  }
}
`,
      "domains/Demo/actions/Demo.sophia",
    );

    expect(result.diagnostics).toEqual([]);
    expect(result.statements).toMatchObject([
      {
        kind: "match",
        expression: "maybe_label",
        cases: [
          {
            pattern: "Some",
            binding: "label",
            body: [{ kind: "return", expression: "label" }],
          },
          {
            pattern: "None",
            binding: null,
            body: [{ kind: "return", expression: '"missing"' }],
          },
        ],
      },
    ]);
    expect(
      flattenSophiaBodyStatements(result.statements).map((statement) => statement.kind),
    ).toEqual(["match", "return", "return"]);
  });

  it("rejects catch-all match cases", () => {
    const result = parseSophiaBody(
      `
match flag {
  true {
    return "yes"
  }
  _ {
    return "fallback"
  }
}
`,
      "domains/Demo/actions/Demo.sophia",
    );

    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toContain(
      "Sophia match does not support catch-all _ cases.",
    );
  });
});
