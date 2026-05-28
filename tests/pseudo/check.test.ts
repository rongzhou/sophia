import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { checkPseudocode } from "../../src/pseudo/check.js";
import { samplePseudocodeJson } from "../helpers/sophia_workspace.js";

describe("checkPseudocode", () => {
  it("accepts structured JSON algorithm pseudocode", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        purpose: "Return whether a count is positive.",
        inputs: [{ name: "count", meaning: "integer count" }],
        outputs: [{ name: "result", meaning: "true when count is positive, false otherwise" }],
        algorithm: ["If count is greater than zero, return true.", "Otherwise, return false."],
      }),
    );

    expect(result.ok).toBe(true);
  });

  it("rejects vague loop counts", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        purpose: "Compute rabbit numbers.",
        outputs: [{ name: "numbers", meaning: "list of values" }],
        algorithm: ["repeat several times", "do the calculation", "return numbers"],
      }),
    );

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-LOOP-001");
  });

  it("warns about direct list emptiness checks without rejecting pseudocode", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        purpose: "Build a filtered list.",
        inputs: [{ name: "first", meaning: "integer" }],
        outputs: [{ name: "result", meaning: "List<Int>" }],
        algorithm: [
          "set result to empty List<Int>",
          "if result is empty, print empty",
          "return result",
        ],
        effects: ["print when list is empty"],
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-LIST-001");
  });

  it("warns about increment shorthand without rejecting pseudocode", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        purpose: "Count positives.",
        inputs: [{ name: "first", meaning: "integer" }],
        outputs: [{ name: "result", meaning: "integer" }],
        algorithm: ["set result to 0", "if first > 0, increment result", "return result"],
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-STATE-001");
  });

  it("warns about explicit text conversion for console printing without rejecting pseudocode", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        purpose: "Print generated numbers.",
        outputs: [{ name: "result", meaning: "list of integers" }],
        algorithm: [
          "set square_text to convert square to Text",
          "print square as text",
          "return result",
        ],
        effects: ["print generated squares"],
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["PSEUDO-TEXT-001", "PSEUDO-TEXT-002"]),
    );
  });

  it("rejects else-nested input chains when building list results", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        purpose: "Build running totals for positive inputs.",
        inputs: [
          { name: "first", meaning: "integer" },
          { name: "second", meaning: "integer" },
          { name: "third", meaning: "integer" },
        ],
        outputs: [{ name: "result", meaning: "List<Int>" }],
        algorithm: [
          "set result to empty list",
          "append first to result",
          "if first > 0 { append first to result } else { if second > 0 { append second to result } else { if third > 0 { append third to result } } }",
          "return result",
        ],
      }),
    );

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-BRANCH-002");
  });

  it("allows independent input branches when building list results", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        purpose: "Build running totals for positive inputs.",
        inputs: [
          { name: "first", meaning: "integer" },
          { name: "second", meaning: "integer" },
          { name: "third", meaning: "integer" },
        ],
        outputs: [{ name: "result", meaning: "List<Int>" }],
        algorithm: [
          "set result to empty list",
          "if first > 0 { append first to result }",
          "if second > 0 { append second to result }",
          "if third > 0 { append third to result }",
          "return result",
        ],
      }),
    );

    expect(result.ok).toBe(true);
  });

  it("warns about multiple pseudocode outputs for the v0 single-output action model", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        outputs: [
          { name: "result", meaning: "integer" },
          { name: "helper", meaning: "boolean" },
        ],
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-OUTPUT-001");
  });

  it("warns about numeric 0/1 flags used as Bool-like conditions", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        purpose: "Use a flag.",
        inputs: [{ name: "value", meaning: "integer" }],
        outputs: [{ name: "result", meaning: "integer" }],
        algorithm: [
          "set is_valid to 0",
          "if value > 0, set is_valid to 1",
          "if is_valid { return value } else { return 0 }",
        ],
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-BOOL-001");
  });

  it("warns about implementation hints without rejecting pseudocode", () => {
    const result = checkPseudocode(
      samplePseudocodeJson({
        implementation_hints: {
          program: "Flow",
          domain: "FlowDomain",
          action: "Flow",
          capability: "FlowCapability",
        },
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-HINT-001");
  });

  it("rejects non-JSON pseudocode as missing JSON sections", () => {
    const result = checkPseudocode("{not json");

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "PSEUDO-SECTION-001",
    );
    expect(result.checks.has_purpose).toBe(false);
  });

  it("accepts the action pipeline fixture as structured pseudocode", () => {
    const fixture = readFileSync("fixtures/account/process_deposit_pipeline.pseudo", "utf8");
    const result = checkPseudocode(fixture);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(["PSEUDO-HINT-001"]);
    expect(result.checks.has_expected).toBe(true);
  });
});
