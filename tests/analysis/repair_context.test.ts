import { describe, expect, it } from "vitest";
import { buildRepairContext } from "../../src/analysis/repair_context.js";

describe("buildRepairContext", () => {
  it("summarizes diagnostic codes and includes local snippets without inventing fixes", () => {
    const context = buildRepairContext({
      files: {
        "domains/Demo/actions/Demo.sophia": [
          "action Demo {",
          "  capability: DemoCapability",
          "  output { result: Unit }",
          "  effects { }",
          "  body {",
          "    var value = 1",
          "    return unit",
          "  }",
          "}",
        ].join("\n"),
      },
      checkResult: {
        ok: false,
        diagnostics: [
          {
            code: "CHECK-SYNTAX-006",
            severity: "error",
            location: "domains/Demo/actions/Demo.sophia:6",
            problem: "Unsupported var.",
            repair: "Use let mutable.",
          },
        ],
      },
    });

    expect(context.diagnostic_summary).toEqual([
      { code: "CHECK-SYNTAX-006", count: 1, severity: "error" },
    ]);
    expect(context.affected_files).toHaveLength(1);
    expect(context.affected_files[0]?.path).toBe("domains/Demo/actions/Demo.sophia");
    expect(context.affected_files[0]?.snippets).toEqual([
      { line: 5, text: "  body {" },
      { line: 6, text: "    var value = 1" },
      { line: 7, text: "    return unit" },
    ]);
    expect(JSON.stringify(context)).not.toContain("let mutable value = 1");
  });

  it("infers useful snippets for file-level diagnostics without line numbers", () => {
    const context = buildRepairContext({
      files: {
        "domains/Demo/actions/Demo.sophia": [
          "action Demo {",
          "  body {",
          "    var value = 1",
          "    Console.Write(value)",
          "    return unit",
          "  }",
          "}",
        ].join("\n"),
      },
      checkResult: {
        ok: false,
        diagnostics: [
          {
            code: "CHECK-SYNTAX-006",
            severity: "error",
            location: "domains/Demo/actions/Demo.sophia",
            problem: "Unsupported var.",
          },
          {
            code: "CHECK-BODY-002",
            severity: "error",
            location: "domains/Demo/actions/Demo.sophia",
            problem: "Direct console call.",
          },
        ],
      },
    });

    expect(context.affected_files[0]?.snippets).toEqual([
      { line: 3, text: "    var value = 1" },
      { line: 4, text: "    Console.Write(value)" },
    ]);
  });
});
