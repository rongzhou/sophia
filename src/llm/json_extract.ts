import { PROMPT_PATHS, renderPromptTemplate } from "./prompt_templates.js";

export interface JsonExtractResult {
  ok: boolean;
  value?: unknown;
  error?: string;
}

export function extractJsonObject(text: string): JsonExtractResult {
  const trimmed = text.trim();
  const direct = tryParse(trimmed);
  if (direct.ok) return direct;

  const fenced = /```(?:json)?\s*([\s\S]*?)```/i.exec(trimmed);
  if (fenced?.[1]) {
    const parsed = tryParse(fenced[1].trim());
    if (parsed.ok) return parsed;
  }

  const firstBrace = trimmed.indexOf("{");
  const lastBrace = trimmed.lastIndexOf("}");
  if (firstBrace >= 0 && lastBrace > firstBrace) {
    return tryParse(trimmed.slice(firstBrace, lastBrace + 1));
  }

  return { ok: false, error: "No JSON object found in LLM response." };
}

export function buildJsonOnlyRetryPrompt(options: {
  originalPrompt: string;
  invalidResponse: string;
  parseError: string;
}): string {
  return renderPromptTemplate(PROMPT_PATHS.client.jsonOnlyRetry, {
    parse_error: options.parseError,
    original_prompt: options.originalPrompt,
    invalid_response: options.invalidResponse,
  });
}

function tryParse(text: string): JsonExtractResult {
  try {
    return { ok: true, value: JSON.parse(text) };
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : "Unknown JSON parse error.",
    };
  }
}
