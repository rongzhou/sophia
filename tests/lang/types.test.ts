import { describe, expect, it } from "vitest";
import {
  isSophiaV0Type,
  isSophiaTypeAssignable,
  matchesSophiaRuntimeType,
  parseSophiaIntentType,
  parseSophiaOptionalType,
  sampleSophiaRuntimeValue,
  sophiaTypeToTypeScript,
} from "../../src/lang/ast/types.js";

describe("Sophia v0 types", () => {
  it("defines the shared v0 type whitelist", () => {
    expect(isSophiaV0Type("Unit")).toBe(true);
    expect(isSophiaV0Type("Bool")).toBe(true);
    expect(isSophiaV0Type("Int")).toBe(true);
    expect(isSophiaV0Type("Text")).toBe(true);
    expect(isSophiaV0Type("List<Int>")).toBe(true);
    expect(isSophiaV0Type("List<Text>")).toBe(true);
    expect(isSophiaV0Type("Uuid")).toBe(false);
  });

  it("maps Sophia types to generated TypeScript types", () => {
    expect(sophiaTypeToTypeScript("Unit")).toBe("Unit");
    expect(sophiaTypeToTypeScript("Bool")).toBe("boolean");
    expect(sophiaTypeToTypeScript("Int")).toBe("number");
    expect(sophiaTypeToTypeScript("Text")).toBe("string");
    expect(sophiaTypeToTypeScript("List<Int>")).toBe("number[]");
    expect(sophiaTypeToTypeScript("List<Text>")).toBe("string[]");
    expect(sophiaTypeToTypeScript("Optional<Int>")).toBe("number | null");
  });

  it("parses intent wrappers and erases them for TypeScript/runtime shape", () => {
    expect(parseSophiaIntentType("Raw<Text>")).toEqual({
      intent: "Raw",
      innerType: "Text",
    });
    expect(parseSophiaIntentType("Sanitized<Account>")).toEqual({
      intent: "Sanitized",
      innerType: "Account",
    });
    expect(parseSophiaIntentType("Text")).toBeNull();
    expect(sophiaTypeToTypeScript("Raw<Text>")).toBe("string");
    expect(sophiaTypeToTypeScript("Sanitized<List<Text>>")).toBe("string[]");
  });

  it("parses Optional wrappers and composes them with intent types", () => {
    expect(parseSophiaOptionalType("Optional<Text>")).toEqual({ innerType: "Text" });
    expect(parseSophiaOptionalType("Optional<Sanitized<Text>>")).toEqual({
      innerType: "Sanitized<Text>",
    });
    expect(sophiaTypeToTypeScript("Optional<Sanitized<Text>>")).toBe("string | null");
  });

  it("keeps intent assignability strict", () => {
    expect(isSophiaTypeAssignable("Text", "Text")).toBe(true);
    expect(isSophiaTypeAssignable("Raw<Text>", "Sanitized<Text>")).toBe(false);
    expect(isSophiaTypeAssignable("Text", "Raw<Text>")).toBe(false);
    expect(isSophiaTypeAssignable("Sanitized<Text>", "Text")).toBe(false);
    expect(isSophiaTypeAssignable("None", "Optional<Text>")).toBe(true);
    expect(isSophiaTypeAssignable("Text", "Optional<Text>")).toBe(false);
  });

  it("validates runtime values with the same v0 type names", () => {
    const entityTypes = new Map([
      [
        "Account",
        [
          { name: "balance", type: "Int" },
          { name: "is_locked", type: "Bool" },
        ],
      ],
    ]);
    const stateTypes = new Map([["TodoStatus", ["Pending", "Done"]]]);
    expect(matchesSophiaRuntimeType(null, "Unit")).toBe(true);
    expect(matchesSophiaRuntimeType(true, "Bool")).toBe(true);
    expect(matchesSophiaRuntimeType("true", "Bool")).toBe(false);
    expect(matchesSophiaRuntimeType(3, "Int")).toBe(true);
    expect(matchesSophiaRuntimeType(3.5, "Int")).toBe(false);
    expect(matchesSophiaRuntimeType("ready", "Text")).toBe(true);
    expect(matchesSophiaRuntimeType(null, "Optional<Text>")).toBe(true);
    expect(matchesSophiaRuntimeType("ready", "Optional<Text>")).toBe(true);
    expect(matchesSophiaRuntimeType(1, "Optional<Text>")).toBe(false);
    expect(matchesSophiaRuntimeType("ready", "Raw<Text>")).toBe(true);
    expect(matchesSophiaRuntimeType([1, 2], "List<Int>")).toBe(true);
    expect(matchesSophiaRuntimeType([1, "2"], "List<Int>")).toBe(false);
    expect(matchesSophiaRuntimeType(["a", "b"], "List<Text>")).toBe(true);
    expect(
      matchesSophiaRuntimeType({ balance: 10, is_locked: false }, "Account", entityTypes),
    ).toBe(true);
    expect(
      matchesSophiaRuntimeType({ balance: "10", is_locked: false }, "Account", entityTypes),
    ).toBe(false);
    expect(
      matchesSophiaRuntimeType(
        { balance: 10, is_locked: false },
        "Persisted<Account>",
        entityTypes,
      ),
    ).toBe(true);
    expect(matchesSophiaRuntimeType("Done", "TodoStatus", entityTypes, stateTypes)).toBe(true);
    expect(matchesSophiaRuntimeType("Closed", "TodoStatus", entityTypes, stateTypes)).toBe(false);
  });

  it("generates deterministic type-valid runtime samples without semantic answers", () => {
    const entityTypes = new Map([
      [
        "Account",
        [
          { name: "balance", type: "Int" },
          { name: "is_locked", type: "Bool" },
        ],
      ],
    ]);
    const stateTypes = new Map([["TodoStatus", ["Pending", "Done"]]]);
    expect(sampleSophiaRuntimeValue("Unit")).toBeNull();
    expect(sampleSophiaRuntimeValue("Bool")).toBe(false);
    expect(sampleSophiaRuntimeValue("Int")).toBe(0);
    expect(sampleSophiaRuntimeValue("Text")).toBe("");
    expect(sampleSophiaRuntimeValue("Optional<Text>")).toBeNull();
    expect(sampleSophiaRuntimeValue("Raw<Text>")).toBe("");
    expect(sampleSophiaRuntimeValue("List<Int>")).toEqual([]);
    expect(sampleSophiaRuntimeValue("List<Text>")).toEqual([]);
    expect(sampleSophiaRuntimeValue("Account", entityTypes)).toEqual({
      balance: 0,
      is_locked: false,
    });
    expect(sampleSophiaRuntimeValue("TodoStatus", entityTypes, stateTypes)).toBe("Pending");
    expect(sampleSophiaRuntimeValue("Unknown")).toBeUndefined();
  });
});
