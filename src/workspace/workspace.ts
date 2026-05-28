import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { GraphStore } from "../graph/core/store.js";
import { pathExists } from "../util/fs.js";
import { SOPHIA_BUILD_DIR, SOPHIA_GRAPH_DIR, SOPHIA_RUNS_DIR } from "./fs_layout.js";
import {
  parseMinimalToml,
  requireBooleanConfigValue,
  requireConfigValue,
} from "./toml.js";

const TOML_KEYS = {
  source: {
    domain_root: "source.domain_root",
    generated_dir: "source.generated_dir",
  },
  layout: {
    strategy: "layout.strategy",
    one_top_level_node_per_file: "layout.one_top_level_node_per_file",
    forbid_global_kind_dirs: "layout.forbid_global_kind_dirs",
  },
  build: {
    target: "build.target",
    out_dir: "build.out_dir",
  },
  check: {
    require_strip_assist_equivalence: "check.require_strip_assist_equivalence",
    forbid_implicit_imports: "check.forbid_implicit_imports",
    forbid_shadowing: "check.forbid_shadowing",
    require_explicit_cross_domain_boundary: "check.require_explicit_cross_domain_boundary",
  },
} as const;

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
version = "0.3.0"
sophia_version = "0.3"

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
  const domainRoot = requireConfigValue(parsed, TOML_KEYS.source.domain_root);
  const generatedDir = requireConfigValue(parsed, TOML_KEYS.source.generated_dir);
  const layoutStrategy = requireConfigValue(parsed, TOML_KEYS.layout.strategy);
  const oneTopLevelNodePerFile = requireBooleanConfigValue(
    parsed,
    TOML_KEYS.layout.one_top_level_node_per_file,
  );
  const forbidGlobalKindDirs = requireBooleanConfigValue(
    parsed,
    TOML_KEYS.layout.forbid_global_kind_dirs,
  );
  const buildTarget = requireConfigValue(parsed, TOML_KEYS.build.target);
  const buildOutDir = requireConfigValue(parsed, TOML_KEYS.build.out_dir);
  const requireStripAssistEquivalence = requireBooleanConfigValue(
    parsed,
    TOML_KEYS.check.require_strip_assist_equivalence,
  );
  const forbidImplicitImports = requireBooleanConfigValue(
    parsed,
    TOML_KEYS.check.forbid_implicit_imports,
  );
  const forbidShadowing = requireBooleanConfigValue(
    parsed,
    TOML_KEYS.check.forbid_shadowing,
  );
  const requireExplicitCrossDomainBoundary = requireBooleanConfigValue(
    parsed,
    TOML_KEYS.check.require_explicit_cross_domain_boundary,
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

// TOML helpers are extracted to toml.ts for clarity.
