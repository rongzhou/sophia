import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { GraphStore } from "../graph/store.js";
import { isNotFoundError, pathExists } from "../util/fs.js";
import { SOPHIA_BUILD_DIR, SOPHIA_GRAPH_DIR, SOPHIA_RUNS_DIR } from "./fs_layout.js";

export interface InitWorkspaceResult {
  root: string;
  created: string[];
  existing: string[];
}

export interface WorkspaceConfig {
  source: {
    domain_root: string;
    generated_dir: string;
  };
  layout: {
    strategy: string;
    one_top_level_node_per_file: boolean;
    forbid_global_kind_dirs: boolean;
  };
  build: {
    target: string;
    out_dir: string;
  };
  check: {
    require_strip_assist_equivalence: boolean;
    forbid_implicit_imports: boolean;
    forbid_shadowing: boolean;
    require_explicit_cross_domain_boundary: boolean;
  };
}

export async function initWorkspace(root: string): Promise<InitWorkspaceResult> {
  const created: string[] = [];
  const existing: string[] = [];

  await ensureDirectory(root, "domains", created, existing);
  await ensureDirectory(root, SOPHIA_RUNS_DIR, created, existing);
  await ensureSophiaToml(root, created, existing);

  const store = new GraphStore(root);
  await store.init();
  existing.push(SOPHIA_GRAPH_DIR);

  return {
    root,
    created: created.sort(),
    existing: [...new Set(existing)].sort(),
  };
}

async function ensureDirectory(
  root: string,
  relativePath: string,
  created: string[],
  existing: string[],
): Promise<void> {
  const absolutePath = path.join(root, relativePath);
  const existed = await pathExists(absolutePath);
  await mkdir(absolutePath, { recursive: true });
  (existed ? existing : created).push(relativePath);
}

async function ensureSophiaToml(
  root: string,
  created: string[],
  existing: string[],
): Promise<void> {
  const relativePath = "sophia.toml";
  const absolutePath = path.join(root, relativePath);
  if (await pathExists(absolutePath)) {
    existing.push(relativePath);
    return;
  }
  await writeFile(absolutePath, `${sophiaTomlTemplate(projectNameFromRoot(root))}\n`, "utf8");
  created.push(relativePath);
}

export function sophiaTomlTemplate(projectName: string): string {
  return `[project]
name = "${escapeTomlString(projectName)}"
version = "0.1.0"
sophia_version = "0.1"

[source]
domain_root = "domains"
generated_dir = "${SOPHIA_RUNS_DIR}/generated"

[layout]
strategy = "domain_first"
one_top_level_node_per_file = true
forbid_global_kind_dirs = true

[build]
target = "typescript"
out_dir = "${SOPHIA_BUILD_DIR}"

[check]
require_strip_assist_equivalence = true
forbid_implicit_imports = true
forbid_shadowing = true
require_explicit_cross_domain_boundary = true`;
}

export async function loadWorkspaceConfig(root: string): Promise<WorkspaceConfig> {
  const configPath = path.join(root, "sophia.toml");
  const content = await readFile(configPath, "utf8");
  if (!content.trim()) {
    throw new Error(`Workspace config is empty: sophia.toml`);
  }

  const parsed = parseMinimalToml(content);
  const domainRoot = requireConfigValue(parsed, "source.domain_root");
  const generatedDir = requireConfigValue(parsed, "source.generated_dir");
  const layoutStrategy = requireConfigValue(parsed, "layout.strategy");
  const oneTopLevelNodePerFile = requireBooleanConfigValue(
    parsed,
    "layout.one_top_level_node_per_file",
  );
  const forbidGlobalKindDirs = requireBooleanConfigValue(parsed, "layout.forbid_global_kind_dirs");
  const buildTarget = requireConfigValue(parsed, "build.target");
  const buildOutDir = requireConfigValue(parsed, "build.out_dir");
  const requireStripAssistEquivalence = requireBooleanConfigValue(
    parsed,
    "check.require_strip_assist_equivalence",
  );
  const forbidImplicitImports = requireBooleanConfigValue(parsed, "check.forbid_implicit_imports");
  const forbidShadowing = requireBooleanConfigValue(parsed, "check.forbid_shadowing");
  const requireExplicitCrossDomainBoundary = requireBooleanConfigValue(
    parsed,
    "check.require_explicit_cross_domain_boundary",
  );
  return {
    source: {
      domain_root: domainRoot,
      generated_dir: generatedDir,
    },
    layout: {
      strategy: layoutStrategy,
      one_top_level_node_per_file: oneTopLevelNodePerFile,
      forbid_global_kind_dirs: forbidGlobalKindDirs,
    },
    build: {
      target: buildTarget,
      out_dir: buildOutDir,
    },
    check: {
      require_strip_assist_equivalence: requireStripAssistEquivalence,
      forbid_implicit_imports: forbidImplicitImports,
      forbid_shadowing: forbidShadowing,
      require_explicit_cross_domain_boundary: requireExplicitCrossDomainBoundary,
    },
  };
}

function projectNameFromRoot(root: string): string {
  const name = path.basename(root).trim();
  return name || "sophia_project";
}

function escapeTomlString(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

type TomlValue = string | boolean | number | string[];

function parseMinimalToml(content: string): Map<string, TomlValue> {
  const values = new Map<string, TomlValue>();
  let section = "";
  for (const rawLine of content.split("\n")) {
    const line = stripTomlComment(rawLine).trim();
    if (!line) continue;
    const sectionMatch = /^\[([A-Za-z_][\w.]*)\]$/.exec(line);
    if (sectionMatch?.[1]) {
      section = sectionMatch[1];
      continue;
    }
    const assignmentMatch = /^([A-Za-z_]\w*)\s*=\s*(.+)$/.exec(line);
    if (assignmentMatch?.[1] && assignmentMatch[2] !== undefined) {
      values.set(`${section}.${assignmentMatch[1]}`, parseTomlValue(assignmentMatch[2].trim()));
    }
  }
  return values;
}

function parseTomlValue(rawValue: string): TomlValue {
  const stringMatch = /^"((?:\\"|\\\\|[^"])*)"$/.exec(rawValue);
  if (stringMatch?.[1] !== undefined) {
    return unescapeTomlString(stringMatch[1]);
  }
  if (rawValue === "true") return true;
  if (rawValue === "false") return false;
  if (/^-?\d+(?:\.\d+)?$/.test(rawValue)) return Number(rawValue);
  const arrayMatch = /^\[(.*)\]$/.exec(rawValue);
  if (arrayMatch?.[1] !== undefined) {
    const body = arrayMatch[1].trim();
    if (!body) return [];
    return body.split(",").map((item) => {
      const parsed = parseTomlValue(item.trim());
      if (typeof parsed !== "string") {
        throw new Error(`Only string arrays are supported in sophia.toml: ${rawValue}`);
      }
      return parsed;
    });
  }
  throw new Error(`Unsupported sophia.toml value: ${rawValue}`);
}

function stripTomlComment(line: string): string {
  let inString = false;
  let escaped = false;
  let result = "";
  for (const char of line) {
    if (escaped) {
      result += char;
      escaped = false;
      continue;
    }
    if (char === "\\") {
      result += char;
      escaped = true;
      continue;
    }
    if (char === '"') {
      inString = !inString;
      result += char;
      continue;
    }
    if (char === "#" && !inString) break;
    result += char;
  }
  return result;
}

function unescapeTomlString(value: string): string {
  return value.replace(/\\"/g, '"').replace(/\\\\/g, "\\");
}

function requireConfigValue(values: Map<string, TomlValue>, key: string): string {
  const value = values.get(key);
  if (typeof value !== "string" || !value) {
    throw new Error(`Missing required sophia.toml value: ${key}`);
  }
  return value;
}

function requireBooleanConfigValue(values: Map<string, TomlValue>, key: string): boolean {
  const value = values.get(key);
  if (typeof value !== "boolean") {
    throw new Error(`Missing required sophia.toml boolean value: ${key}`);
  }
  return value;
}
