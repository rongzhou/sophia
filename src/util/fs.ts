import { open, readFile, readdir, unlink } from "node:fs/promises";
import path from "node:path";

export function isNotFoundError(error: unknown): boolean {
  return typeof error === "object" && error !== null && "code" in error && error.code === "ENOENT";
}

export function isFileExistsError(error: unknown): boolean {
  return typeof error === "object" && error !== null && "code" in error && error.code === "EEXIST";
}

export async function pathExists(filePath: string): Promise<boolean> {
  return readFile(filePath)
    .then(() => true)
    .catch((error: unknown) => {
      if (isNotFoundError(error) || isDirectoryError(error)) return false;
      throw error;
    });
}

export function isDirectoryError(error: unknown): boolean {
  return typeof error === "object" && error !== null && "code" in error && error.code === "EISDIR";
}

export async function collectSophiaFiles(
  absoluteDir: string,
  relativeDir: string,
  files: Record<string, string> = {},
): Promise<Record<string, string>> {
  const entries = await readdir(absoluteDir, { withFileTypes: true });
  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    const absolutePath = path.join(absoluteDir, entry.name);
    const relativePath = path.posix.join(relativeDir, entry.name);
    if (entry.isDirectory()) {
      await collectSophiaFiles(absolutePath, relativePath, files);
    } else if (entry.isFile() && entry.name.endsWith(".sophia")) {
      files[relativePath] = await readFile(absolutePath, "utf8");
    }
  }
  return files;
}

export async function withFileLock<T>(options: {
  lockPath: string;
  attempts: number;
  retryMs: number;
  operation: () => Promise<T>;
  errorLabel: string;
}): Promise<T> {
  let lastError: unknown;
  for (let attempt = 0; attempt < options.attempts; attempt += 1) {
    try {
      const handle = await open(options.lockPath, "wx");
      try {
        return await options.operation();
      } finally {
        await handle.close();
        await unlink(options.lockPath).catch(() => undefined);
      }
    } catch (error) {
      lastError = error;
      if (!isFileExistsError(error)) throw error;
      await sleep(options.retryMs);
    }
  }
  throw new Error(`Could not acquire ${options.errorLabel}: ${String(lastError)}`);
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}
