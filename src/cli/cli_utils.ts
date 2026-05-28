import path from "node:path";
import { loadWorkspaceConfig } from "../workspace/workspace.js";
import { collectSophiaFiles, isNotFoundError } from "../util/fs.js";
import { isRecord } from "../util/json.js";
import { normalizeRelativePath } from "../util/strings.js";

export function isNodeId(value: string): boolean {
  return /^N\d{4,}$/.test(value);
}

export function parseNonNegativeIntegerOption(value: string, optionName: string): number {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isInteger(parsed) || parsed < 0) {
    throw new Error(`${optionName} must be a non-negative integer.`);
  }
  return parsed;
}

export function parsePositiveIntegerOption(value: string, optionName: string): number {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${optionName} must be a positive integer.`);
  }
  return parsed;
}

export function parseJsonOption(value: string, optionName: string): unknown {
  try {
    return JSON.parse(value);
  } catch (error) {
    throw new Error(
      `${optionName} must be valid JSON: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

export function parseJsonObjectOption(value: string, optionName: string): Record<string, unknown> {
  const parsed = parseJsonOption(value, optionName);
  if (!isRecord(parsed)) {
    throw new Error(`${optionName} must be a JSON object.`);
  }
  return parsed;
}

export function parseStringArrayOption(value: string, optionName: string): string[] {
  const parsed = parseJsonOption(value, optionName);
  if (!Array.isArray(parsed) || parsed.some((item) => typeof item !== "string")) {
    throw new Error(`${optionName} must be a JSON string array.`);
  }
  return parsed;
}

export function parseJsonArrayOption<T>(value: string, optionName: string): T[] {
  const parsed = parseJsonOption(value, optionName);
  if (!Array.isArray(parsed)) {
    throw new Error(`${optionName} must be a JSON array.`);
  }
  return parsed as T[];
}

export function parseEnumOption<const T extends string>(
  value: string,
  optionName: string,
  allowed: readonly T[],
): T {
  if (!allowed.includes(value as T)) {
    throw new Error(`${optionName} must be one of: ${allowed.join(", ")}.`);
  }
  return value as T;
}

export function setFailedExitIf(failed: boolean): void {
  if (failed) process.exitCode = 1;
}

export function printJson(value: unknown): void {
  console.log(JSON.stringify(value, null, 2));
}

export function printJsonLine(value: unknown): void {
  console.log(JSON.stringify(value));
}

export async function readSophiaFilesFromDomains(root: string): Promise<Record<string, string>> {
  const config = await loadWorkspaceConfig(root);
  const domainRoot = normalizeRelativePath(config.source.domain_root);
  const domainsRoot = path.join(root, domainRoot);
  return collectSophiaFiles(domainsRoot, domainRoot).catch((error: unknown) => {
    if (!isNotFoundError(error)) throw error;
    return {};
  });
}
