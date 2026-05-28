export const OLLAMA_HOST_ENV = "OLLAMA_HOST";
export const SOPHIA_OLLAMA_TIMEOUT_MS_ENV = "SOPHIA_OLLAMA_TIMEOUT_MS";
export const SOPHIA_OLLAMA_NUM_PREDICT_ENV = "SOPHIA_OLLAMA_NUM_PREDICT";

const DEFAULT_OLLAMA_HOST = "http://127.0.0.1:11434";
const DEFAULT_OLLAMA_TIMEOUT_MS = 900_000;

export interface OllamaRuntimeConfig {
  host: string;
  timeoutMs: number;
  numPredict: number | null;
}

export function readOllamaRuntimeConfig(): OllamaRuntimeConfig {
  return {
    host: process.env[OLLAMA_HOST_ENV] ?? DEFAULT_OLLAMA_HOST,
    timeoutMs: readPositiveIntegerEnv(SOPHIA_OLLAMA_TIMEOUT_MS_ENV) ?? DEFAULT_OLLAMA_TIMEOUT_MS,
    numPredict: readPositiveIntegerEnv(SOPHIA_OLLAMA_NUM_PREDICT_ENV),
  };
}

export function applyOllamaRuntimeOverrides(options: {
  timeoutMs?: number;
  numPredict?: number;
}): void {
  if (options.timeoutMs !== undefined) {
    process.env[SOPHIA_OLLAMA_TIMEOUT_MS_ENV] = String(options.timeoutMs);
  }
  if (options.numPredict !== undefined) {
    process.env[SOPHIA_OLLAMA_NUM_PREDICT_ENV] = String(options.numPredict);
  }
}

function readPositiveIntegerEnv(name: string): number | null {
  const raw = process.env[name];
  if (!raw) return null;
  const parsed = Number.parseInt(raw, 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}
