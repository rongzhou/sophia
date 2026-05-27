import { mkdir, mkdtemp } from "node:fs/promises";
import path from "node:path";
import { isFileExistsError } from "../util/fs.js";

export const SOPHIA_RUNS_DIR = "sophia-runs";
export const SOPHIA_GRAPH_DIR = path.join(SOPHIA_RUNS_DIR, "graph");
export const SOPHIA_EXPERIMENTS_DIR = path.join(SOPHIA_RUNS_DIR, "experiments");
export const SOPHIA_BUILD_DIR = path.join(SOPHIA_RUNS_DIR, "build");
export const SOPHIA_SCRATCH_DIR = path.join(SOPHIA_RUNS_DIR, "scratch");

export function graphPath(root: string, graphRelativePath = SOPHIA_GRAPH_DIR): string {
  return path.join(root, graphRelativePath);
}

export function graphNodesPath(root: string, graphRelativePath = SOPHIA_GRAPH_DIR): string {
  return path.join(graphPath(root, graphRelativePath), "nodes");
}

export function experimentsPath(root: string): string {
  return path.join(root, SOPHIA_EXPERIMENTS_DIR);
}

export function experimentTaskPath(root: string, taskId: string): string {
  return path.join(experimentsPath(root), safePathSegment(taskId));
}

export async function createRunDirectory(root: string, taskId: string): Promise<string> {
  const taskRoot = experimentTaskPath(root, taskId);
  await ensureDir(taskRoot);
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const suffix = attempt === 0 ? "" : `-${String(attempt + 1).padStart(2, "0")}`;
    const runPath = path.join(taskRoot, `${runTimestamp()}${suffix}`);
    try {
      await mkdir(runPath);
      return runPath;
    } catch (error) {
      if (!isFileExistsError(error)) throw error;
    }
  }
  throw new Error(`Unable to allocate run directory for task ${taskId}.`);
}

export async function ensureRunStageDirectories(runRoot: string): Promise<void> {
  await Promise.all(
    ["goal", "pseudo", "sophia", "executable", "graph"].map((stage) =>
      ensureDir(path.join(runRoot, stage)),
    ),
  );
}

export async function createScratchDirectory(root: string, label: string): Promise<string> {
  const scratchRoot = path.join(root, SOPHIA_SCRATCH_DIR);
  await ensureDir(scratchRoot);
  return mkdtemp(path.join(scratchRoot, `${safePathSegment(label)}-`));
}

export async function ensureDir(dir: string): Promise<void> {
  await mkdir(dir, { recursive: true });
}

function runTimestamp(): string {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

function safePathSegment(value: string): string {
  const segment = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return segment || "task";
}
