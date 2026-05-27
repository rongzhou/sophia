import { describe, expect, it } from "vitest";
import {
  hasBalancedSophiaBraces,
  parseSophiaImmediateNamedBlocks,
  parseSophiaSource,
  parseSophiaTopLevelDeclarations,
} from "../../src/lang/parser.js";

describe("parseSophiaSource", () => {
  it("parses a v0 action into a deterministic raw AST summary", () => {
    const result = parseSophiaSource(
      `
action Hello {
  meaning: "Return unit after printing."
  capability: ConsoleCapability
  input { }
  output { result: Unit }
  effects { Console.Write }
  errors { }
  body {
    print "Hello"
    return unit
  }
}
`,
      "domains/Demo/actions/Hello.sophia",
    );

    expect(result.ok).toBe(true);
    expect(result.ast).toMatchObject({
      kind: "action",
      name: "Hello",
      attributes: [
        { name: "capability", value: "ConsoleCapability" },
        { name: "meaning", value: '"Return unit after printing."' },
      ],
    });
    expect(result.ast?.attributes.map((attribute) => attribute.name)).not.toContain("result");
    expect(result.ast?.blocks.map((block) => block.name)).toEqual([
      "body",
      "effects",
      "errors",
      "input",
      "output",
    ]);
    expect(result.ast?.blocks.find((block) => block.name === "body")?.body).toContain(
      "return unit",
    );
  });

  it("rejects multiple top-level nodes", () => {
    const result = parseSophiaSource(
      `
domain One {}
domain Two {}
`,
      "domains/Demo/domain.sophia",
    );

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PARSE-FILE-002");
  });

  it("parses entity fields as a formal v0 top-level node", () => {
    const result = parseSophiaSource(
      `
entity Todo {
  fields {
    title: Text
    done: Bool
  }
}
`,
      "domains/Demo/entities/Todo.sophia",
    );

    expect(result.ok).toBe(true);
    expect(result.ast).toMatchObject({ kind: "entity", name: "Todo" });
    expect(result.ast?.blocks.map((block) => block.name)).toEqual(["fields"]);
  });

  it("parses storage key and value attributes as a formal v0 top-level node", () => {
    const result = parseSophiaSource(
      `
storage Todos {
  key: Persisted<Text>
  value: Sanitized<Text>
}
`,
      "domains/Demo/storages/Todos.sophia",
    );

    expect(result.ok).toBe(true);
    expect(result.ast).toMatchObject({ kind: "storage", name: "Todos" });
    expect(result.ast?.attributes).toEqual([
      { name: "key", value: "Persisted<Text>" },
      { name: "value", value: "Sanitized<Text>" },
    ]);
  });

  it("parses error variants as a formal v0 top-level node", () => {
    const result = parseSophiaSource(
      `
error AccountError {
  variant InvalidAmount {
    amount: Int
  }
}
`,
      "domains/Demo/errors/AccountError.sophia",
    );

    expect(result.ok).toBe(true);
    expect(result.ast).toMatchObject({ kind: "error", name: "AccountError" });
    expect(result.ast?.blocks).toEqual([
      {
        name: "InvalidAmount",
        body: "amount: Int",
      },
    ]);
  });

  it("parses state values as a formal v0 top-level node", () => {
    const result = parseSophiaSource(
      `
state TodoStatus {
  value Pending {
    meaning: "The Todo is open."
  }
  value Done { }
}
`,
      "domains/Demo/states/TodoStatus.sophia",
    );

    expect(result.ok).toBe(true);
    expect(result.ast).toMatchObject({ kind: "state", name: "TodoStatus" });
    expect(result.ast?.blocks.map((block) => block.name)).toEqual(["Done", "Pending"]);
  });

  it("rejects state value blocks without an explicit value keyword", () => {
    const result = parseSophiaSource(
      `
state TodoStatus {
  Pending { }
}
`,
      "domains/Demo/states/TodoStatus.sophia",
    );

    expect(result.ok).toBe(false);
    expect(result.diagnostics).toContainEqual(
      expect.objectContaining({
        code: "PARSE-BLOCK-001",
        problem: "Unsupported state block in Sophia v0: Pending. Use value Pending { ... }.",
      }),
    );
  });

  it("rejects error variant blocks without an explicit variant keyword", () => {
    const result = parseSophiaSource(
      `
error AccountError {
  InvalidAmount {
    amount: Int
  }
}
`,
      "domains/Demo/errors/AccountError.sophia",
    );

    expect(result.ok).toBe(false);
    expect(result.diagnostics).toContainEqual(
      expect.objectContaining({
        code: "PARSE-BLOCK-001",
        problem:
          "Unsupported error block in Sophia v0: InvalidAmount. Use variant InvalidAmount { ... }.",
      }),
    );
  });

  it("reports unbalanced braces", () => {
    const result = parseSophiaSource(
      `
action Broken {
  body {
    return unit
}
`,
      "domains/Demo/actions/Broken.sophia",
    );

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PARSE-SYNTAX-001");
  });

  it("rejects unsupported immediate blocks for v0 actions", () => {
    const result = parseSophiaSource(
      `
action Demo {
  capability: DemoCapability
  output { result: Unit }
  effects { }
  errors { }
  body { return unit }
  storage { todos }
}
`,
      "domains/Demo/actions/Demo.sophia",
    );

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PARSE-BLOCK-001");
  });

  it("rejects duplicate immediate blocks", () => {
    const result = parseSophiaSource(
      `
capability DemoCapability {
  allow { Console.Write }
  allow { Time.Now }
}
`,
      "domains/Demo/capabilities/DemoCapability.sophia",
    );

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PARSE-BLOCK-002");
  });

  it("exposes shared source-structure parsing for checker and parser callers", () => {
    const content = `
action First {
  output { result: Text }
  body {
    return "{not a brace}"
  }
}
capability ConsoleCapability {
  allow { Console.Write }
}
`;

    const declarations = parseSophiaTopLevelDeclarations(content);
    const actionBlocks = parseSophiaImmediateNamedBlocks(declarations[0]?.body ?? "");

    expect(hasBalancedSophiaBraces(content)).toBe(true);
    expect(declarations.map((declaration) => `${declaration.kind}:${declaration.name}`)).toEqual([
      "action:First",
      "capability:ConsoleCapability",
    ]);
    expect(actionBlocks.map((block) => block.name)).toEqual(["body", "output"]);
    expect(actionBlocks.find((block) => block.name === "body")?.body).toContain("{not a brace}");
  });
});
