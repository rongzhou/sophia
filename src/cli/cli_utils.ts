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
    throw new Error(`${optionName} must be a JSON object mapping action names to input objects.`);
  }
  return parsed;
}

export function setFailedExitIf(failed: boolean): void {
  if (failed) process.exitCode = 1;
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
