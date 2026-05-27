import { describe, expect, it } from "vitest";
import { parseSophiaEffectNames, parseSophiaFieldDeclarations } from "../../src/lang/signature.js";

describe("Sophia signature parsing", () => {
  it("parses field declarations shared by checker and backend metadata", () => {
    expect(
      parseSophiaFieldDeclarations(`
count: Int
items: List<Text>
maybe_title: Optional<Sanitized<Text>>
result: Unit
`),
    ).toEqual([
      { name: "count", type: "Int" },
      { name: "items", type: "List<Text>" },
      { name: "maybe_title", type: "Optional<Sanitized<Text>>" },
      { name: "result", type: "Unit" },
    ]);
  });

  it("parses capability and action effect names", () => {
    expect(parseSophiaEffectNames("Pure Console.Write")).toEqual(["Pure", "Console.Write"]);
    expect(parseSophiaEffectNames("\n  Console.Write\n")).toEqual(["Console.Write"]);
    expect(parseSophiaEffectNames('DB.Write("Todos") DB.Read("Accounts")')).toEqual([
      'DB.Write("Todos")',
      'DB.Read("Accounts")',
    ]);
  });
});
