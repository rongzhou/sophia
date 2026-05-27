import { describe, expect, it } from "vitest";
import { buildJsonOnlyRetryPrompt, extractJsonObject } from "../../src/llm/json_extract.js";

describe("extractJsonObject", () => {
  it("parses a bare JSON object", () => {
    const result = extractJsonObject('{"ok":true}');
    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ ok: true });
  });

  it("extracts fenced json", () => {
    const result = extractJsonObject('Here:\n```json\n{"ok":true}\n```');
    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ ok: true });
  });

  it("extracts fenced output without language hint", () => {
    const result = extractJsonObject('```\n{"ok":true}\n```');
    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ ok: true });
  });

  it("recovers an embedded object surrounded by prose", () => {
    const result = extractJsonObject('thinking...\n{"a":1, "b":[1,2]}\nDone.');
    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ a: 1, b: [1, 2] });
  });

  it("returns ok=false when no JSON object is present", () => {
    const result = extractJsonObject("just prose");
    expect(result.ok).toBe(false);
    expect(result.error).toBeDefined();
  });

  it("surfaces parse errors when only invalid JSON is found", () => {
    const result = extractJsonObject('{"a":}');
    expect(result.ok).toBe(false);
    expect(result.error).toBeDefined();
  });
});

describe("buildJsonOnlyRetryPrompt", () => {
  it("renders the retry template with the supplied fields", () => {
    const prompt = buildJsonOnlyRetryPrompt({
      originalPrompt: "ORIGINAL",
      invalidResponse: "INVALID",
      parseError: "ERR",
    });
    expect(prompt).toContain("ORIGINAL");
    expect(prompt).toContain("INVALID");
    expect(prompt).toContain("ERR");
  });
});
