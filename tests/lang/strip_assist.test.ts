import { describe, expect, it } from "vitest";
import { parseSophiaSource } from "../../src/lang/ast/parser.js";
import { stripSemanticAssistFromAst } from "../../src/lang/ast/strip_assist.js";

describe("stripSemanticAssistFromAst", () => {
  it("removes semantic assist attributes while preserving formal attributes", () => {
    const parsed = parseSophiaSource(
      `
action SanitizeTitle {
  meaning: "Human-facing explanation."
  purpose: "Help the model preserve intent."
  capability: PureCapability
  intent_conversion: true
  input { title: Raw<Text> }
  output { result: Sanitized<Text> }
  effects { }
  body {
    return title
  }
}
`,
      "domains/Demo/actions/SanitizeTitle.sophia",
    );

    expect(parsed.ok).toBe(true);
    expect(parsed.ast).not.toBeNull();

    const stripped = stripSemanticAssistFromAst(parsed.ast!);

    expect(stripped.attributes).toEqual([
      { name: "capability", value: "PureCapability" },
      { name: "intent_conversion", value: "true" },
    ]);
    expect(stripped.blocks.map((block) => block.name)).toEqual([
      "body",
      "effects",
      "input",
      "output",
    ]);
  });
});
