import { readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";
import { z } from "zod";

export const BenchmarkCaseSchema = z.object({
  name: z.string().min(1),
  input: z.unknown(),
  expected_result: z.unknown(),
  expected_effects: z.array(z.string()).default([]),
});

export const BenchmarkTaskSchema = z.object({
  id: z.string().regex(/^[a-z0-9_]+$/),
  title: z.string().min(1),
  category: z.string().min(1),
  scaffold: z.object({
    program: z.string().regex(/^[A-Z][A-Za-z0-9]*$/),
    domain: z.string().regex(/^[A-Z][A-Za-z0-9]*$/),
    action: z.string().regex(/^[A-Z][A-Za-z0-9]*$/),
    capability: z.string().regex(/^[A-Z][A-Za-z0-9]*$/),
    states: z
      .array(
        z.object({
          name: z.string().regex(/^[A-Z][A-Za-z0-9]*$/),
          values: z.array(z.string().regex(/^[A-Z][A-Za-z0-9]*$/)).min(1),
        }),
      )
      .optional(),
    inputs: z
      .array(
        z.object({
          name: z.string().regex(/^[a-z_][A-Za-z0-9_]*$/),
          type: z.string().min(1),
        }),
      )
      .optional(),
    output: z
      .object({
        name: z.string().regex(/^[a-z_][A-Za-z0-9_]*$/),
        type: z.string().min(1),
      })
      .optional(),
    effects: z.array(z.string().min(1)).optional(),
  }),
  prompt_goal: z.string().min(1),
  public_forbidden: z.array(z.string()).default([]),
  hidden_cases: z.array(BenchmarkCaseSchema).min(1),
});

export type BenchmarkTask = z.infer<typeof BenchmarkTaskSchema>;
export type BenchmarkCase = z.infer<typeof BenchmarkCaseSchema>;

export async function loadBenchmarkTask(taskPath: string): Promise<BenchmarkTask> {
  const content = await readFile(taskPath, "utf8");
  return BenchmarkTaskSchema.parse(JSON.parse(content));
}

export async function loadBenchmarkSuite(suitePath: string): Promise<BenchmarkTask[]> {
  const taskFiles = await findTaskFiles(suitePath);
  const tasks = await Promise.all(taskFiles.map(loadBenchmarkTask));
  return tasks.sort((left, right) => left.id.localeCompare(right.id));
}

async function findTaskFiles(root: string): Promise<string[]> {
  const info = await stat(root);
  if (info.isFile()) return [root];
  const entries = await readdir(root, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const entryPath = path.join(root, entry.name);
      if (entry.isDirectory()) return findTaskFiles(entryPath);
      return entry.name === "task.json" ? [entryPath] : [];
    }),
  );
  return nested.flat().sort();
}
