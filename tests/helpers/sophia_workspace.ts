import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

export async function createTempDir(prefix: string): Promise<string> {
  return mkdtemp(path.join(os.tmpdir(), prefix));
}

export async function createSophiaWorkspace(prefix: string): Promise<string> {
  const root = await createTempDir(prefix);
  await writeSophiaToml(root);
  return root;
}

export async function createSophiaWorkspaceWithDemoDomain(prefix: string): Promise<string> {
  const root = await createSophiaWorkspace(prefix);
  await writeDemoDomain(root);
  await writePureCapability(root);
  return root;
}

export async function writeProjectFile(
  root: string,
  relativePath: string,
  content: string,
): Promise<void> {
  const filePath = path.join(root, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content, "utf8");
}

export async function writeSophiaToml(
  root: string,
  options: {
    domainRoot?: string;
    generatedDir?: string;
    buildOutDir?: string;
  } = {},
): Promise<void> {
  const domainRoot = options.domainRoot ?? "domains";
  const generatedDir = options.generatedDir ?? "sophia-runs/generated";
  const buildOutDir = options.buildOutDir ?? "sophia-runs/build";
  await writeProjectFile(
    root,
    "sophia.toml",
    [
      "[source]",
      `domain_root = "${domainRoot}"`,
      `generated_dir = "${generatedDir}"`,
      "",
      "[layout]",
      'strategy = "domain_first"',
      "one_top_level_node_per_file = true",
      "forbid_global_kind_dirs = true",
      "",
      "[build]",
      'target = "typescript"',
      `out_dir = "${buildOutDir}"`,
      "",
      "[check]",
      "require_strip_assist_equivalence = true",
      "forbid_implicit_imports = true",
      "forbid_shadowing = true",
      "require_explicit_cross_domain_boundary = true",
    ].join("\n"),
  );
}

export async function writeDemoDomain(root: string, domain = "Demo"): Promise<void> {
  await writeProjectFile(root, `domains/${domain}/domain.sophia`, `domain ${domain} {}\n`);
}

export async function writePureCapability(
  root: string,
  domain = "Demo",
  capability = "PureCapability",
): Promise<void> {
  await writeProjectFile(
    root,
    `domains/${domain}/capabilities/${capability}.sophia`,
    `capability ${capability} { allow { } }\n`,
  );
}
