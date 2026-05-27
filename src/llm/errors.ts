export class LlmCallParseError extends Error {
  constructor(
    message: string,
    readonly prompt: string,
    readonly rawResponse: string,
  ) {
    super(message);
    this.name = "LlmCallParseError";
  }
}

export class LlmCallExecutionError extends Error {
  constructor(
    message: string,
    readonly prompt: string,
    readonly rawResponse = "",
  ) {
    super(message);
    this.name = "LlmCallExecutionError";
  }
}

export function isLlmCallError(error: unknown): error is LlmCallParseError | LlmCallExecutionError {
  return error instanceof LlmCallParseError || error instanceof LlmCallExecutionError;
}
