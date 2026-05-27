import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const TEMPLATE_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
  "data",
  "prompts",
);

export function loadPromptTemplate(name: string): string {
  const templatePath = path.join(TEMPLATE_ROOT, name);
  const relative = path.relative(TEMPLATE_ROOT, templatePath);
  if (relative.startsWith("..") || path.isAbsolute(relative)) {
    throw new Error(`Prompt template path escapes data/prompts: ${name}`);
  }
  return readFileSync(templatePath, "utf8").trimEnd();
}

export function loadPromptData<T>(name: string): T {
  return JSON.parse(loadPromptTemplate(name)) as T;
}

export function renderPromptTemplate(
  name: string,
  values: Record<string, string | number | boolean>,
): string {
  const template = loadPromptTemplate(name);
  return template.replace(/\{\{([a-zA-Z0-9_]+)\}\}/g, (match, key: string) => {
    const value = values[key];
    if (value === undefined) {
      throw new Error(`Prompt template ${name} has no value for placeholder: ${key}`);
    }
    return String(value);
  });
}
