import type { z } from "zod";
import { buildJsonOnlyRetryPrompt, extractJsonObject } from "./json_extract.js";
import { generateWithOllama } from "./ollama.js";
import { LlmCallExecutionError, LlmCallParseError } from "./errors.js";
import { PROMPT_PATHS, renderPromptTemplate } from "./prompt_templates.js";

export interface GenerateOllamaJsonOptions<T> {
  model: string;
  prompt: string;
  operation: string;
  schema: z.ZodType<T>;
  temperature?: number;
  topP?: number;
  validate?: (output: T) => T;
  validationRetry?: boolean;
}

export interface GenerateOllamaJsonResult<T> {
  prompt: string;
  rawResponse: string;
  output: T;
}

export async function generateOllamaJson<T>(
  options: GenerateOllamaJsonOptions<T>,
): Promise<GenerateOllamaJsonResult<T>> {
  const response = await generateWithOllama({
    model: options.model,
    prompt: options.prompt,
    ...(options.temperature !== undefined ? { temperature: options.temperature } : {}),
    ...(options.topP !== undefined ? { topP: options.topP } : {}),
  }).catch((error: unknown) => {
    const message = error instanceof Error ? error.message : String(error);
    throw new LlmCallExecutionError(message, options.prompt);
  });

  let rawResponse = response.response;
  let extracted = extractJsonObject(rawResponse);
  if (!extracted.ok) {
    const retryPrompt = buildJsonOnlyRetryPrompt({
      originalPrompt: options.prompt,
      invalidResponse: rawResponse,
      parseError: extracted.error ?? "Unknown JSON parse error.",
    });
    const retryResponse = await generateWithOllama({
      model: options.model,
      prompt: retryPrompt,
      temperature: 0,
      ...(options.topP !== undefined ? { topP: options.topP } : {}),
    }).catch((error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      throw new LlmCallExecutionError(
        message,
        `${options.prompt}\n\n--- JSON RETRY ---\n${retryPrompt}`,
        rawResponse,
      );
    });
    rawResponse = `${rawResponse}\n\n--- JSON RETRY RESPONSE ---\n${retryResponse.response}`;
    extracted = extractJsonObject(retryResponse.response);
  }

  if (!extracted.ok) {
    throw new LlmCallParseError(
      `Could not parse ${options.operation} JSON: ${extracted.error}`,
      options.prompt,
      rawResponse,
    );
  }

  const firstParsed = parseAndValidate({
    schema: options.schema,
    value: extracted.value,
    ...(options.validate ? { validate: options.validate } : {}),
  });
  if (firstParsed.ok) {
    return {
      prompt: options.prompt,
      rawResponse,
      output: firstParsed.output,
    };
  }

  if (options.validationRetry) {
    const retryPrompt = buildValidationRetryPrompt({
      originalPrompt: options.prompt,
      invalidResponse: rawResponse,
      validationError: firstParsed.error,
      operation: options.operation,
    });
    const retryResponse = await generateWithOllama({
      model: options.model,
      prompt: retryPrompt,
      temperature: 0,
      ...(options.topP !== undefined ? { topP: options.topP } : {}),
    }).catch((error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      throw new LlmCallExecutionError(
        message,
        `${options.prompt}\n\n--- VALIDATION RETRY ---\n${retryPrompt}`,
        rawResponse,
      );
    });
    rawResponse = `${rawResponse}\n\n--- VALIDATION RETRY RESPONSE ---\n${retryResponse.response}`;
    const retryExtracted = extractJsonObject(retryResponse.response);
    if (!retryExtracted.ok) {
      throw new LlmCallParseError(
        `Could not parse ${options.operation} validation retry JSON: ${retryExtracted.error}`,
        options.prompt,
        rawResponse,
      );
    }
    const retryParsed = parseAndValidate({
      schema: options.schema,
      value: retryExtracted.value,
      ...(options.validate ? { validate: options.validate } : {}),
    });
    if (retryParsed.ok) {
      return {
        prompt: options.prompt,
        rawResponse,
        output: retryParsed.output,
      };
    }
    throw new LlmCallParseError(
      `Invalid ${options.operation} JSON after validation retry: ${retryParsed.error}`,
      options.prompt,
      rawResponse,
    );
  }

  throw new LlmCallParseError(
    `Invalid ${options.operation} JSON: ${firstParsed.error}`,
    options.prompt,
    rawResponse,
  );
}

function parseAndValidate<T>(options: {
  schema: z.ZodType<T>;
  validate?: (output: T) => T;
  value: unknown;
}):
  | { ok: true; output: T }
  | {
      ok: false;
      error: string;
    } {
  try {
    const parsed = options.schema.parse(options.value);
    return { ok: true, output: options.validate ? options.validate(parsed) : parsed };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

function buildValidationRetryPrompt(options: {
  originalPrompt: string;
  invalidResponse: string;
  validationError: string;
  operation: string;
}): string {
  return renderPromptTemplate(PROMPT_PATHS.client.validationRetry, {
    operation: options.operation,
    validation_error: options.validationError,
    original_prompt: options.originalPrompt,
    invalid_response: options.invalidResponse,
  });
}
