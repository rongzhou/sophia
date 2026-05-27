export interface OllamaGenerateOptions {
  model: string;
  prompt: string;
  temperature?: number;
  topP?: number;
  repeatPenalty?: number;
  host?: string;
  timeoutMs?: number;
  numPredict?: number;
}

export interface OllamaGenerateResult {
  model: string;
  response: string;
}

export async function generateWithOllama(
  options: OllamaGenerateOptions,
): Promise<OllamaGenerateResult> {
  const host = options.host ?? process.env.OLLAMA_HOST ?? "http://127.0.0.1:11434";
  const timeoutMs = options.timeoutMs ?? ollamaTimeoutMs();
  const numPredict = options.numPredict ?? ollamaNumPredict();
  const modelOptions: Record<string, number> = {
    temperature: options.temperature ?? 0.2,
    top_p: options.topP ?? 0.9,
    repeat_penalty: options.repeatPenalty ?? 1.05,
  };
  if (numPredict) {
    modelOptions.num_predict = numPredict;
  }
  const response = await fetch(`${host}/api/generate`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    signal: AbortSignal.timeout(timeoutMs),
    body: JSON.stringify({
      model: options.model,
      prompt: options.prompt,
      stream: false,
      options: modelOptions,
    }),
  }).catch((error: unknown) => {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(
      `Could not complete Ollama generate at ${host} within ${timeoutMs}ms. Start Ollama, verify the model with "ollama list", or increase SOPHIA_OLLAMA_TIMEOUT_MS. Cause: ${detail}`,
    );
  });

  if (!response.ok) {
    throw new Error(`Ollama generate failed: ${response.status} ${await response.text()}`);
  }

  const payload = (await response.json()) as { model?: string; response?: string };
  if (typeof payload.response !== "string") {
    throw new Error("Ollama response did not include a string response field.");
  }

  return {
    model: payload.model ?? options.model,
    response: payload.response,
  };
}

function ollamaTimeoutMs(): number {
  const raw = process.env.SOPHIA_OLLAMA_TIMEOUT_MS;
  if (!raw) return 900_000;
  const parsed = Number.parseInt(raw, 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : 900_000;
}

function ollamaNumPredict(): number | null {
  const raw = process.env.SOPHIA_OLLAMA_NUM_PREDICT;
  if (!raw) return null;
  const parsed = Number.parseInt(raw, 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}
