import { readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { createTempDir } from "../helpers/sophia_workspace.js";
import { describe, expect, it } from "vitest";
import { readSophiaFilesFromDomains } from "../../src/cli/cli_utils.js";
import {
  initWorkspace,
  loadWorkspaceConfig,
  sophiaTomlTemplate,
} from "../../src/workspace/workspace.js";

describe("initWorkspace", () => {
  it("creates standard workspace files and directories", async () => {
    const root = await createTempDir("sophia-workspace-");

    const result = await initWorkspace(root);

    expect(result.created).toEqual(["domains", "sophia-runs", "sophia.toml"]);
    expect(await readFile(path.join(root, "sophia.toml"), "utf8")).toContain(
      'domain_root = "domains"',
    );
    expect(await readFile(path.join(root, "sophia.toml"), "utf8")).toContain(
      'generated_dir = "sophia-runs/generated"',
    );
    expect(
      JSON.parse(await readFile(path.join(root, "sophia-runs/graph", "edges.json"), "utf8")),
    ).toEqual([]);
  });

  it("does not overwrite an existing sophia.toml", async () => {
    const root = await createTempDir("sophia-workspace-");
    await writeFile(path.join(root, "sophia.toml"), '[project]\nname = "custom"\n', "utf8");

    const result = await initWorkspace(root);

    expect(result.existing).toContain("sophia.toml");
    expect(await readFile(path.join(root, "sophia.toml"), "utf8")).toBe(
      '[project]\nname = "custom"\n',
    );
  });

  it("requires complete source and build config", async () => {
    const root = await createTempDir("sophia-workspace-");
    await writeFile(
      path.join(root, "sophia.toml"),
      [
        "[source]",
        'domain_root = "src/domains"',
        'generated_dir = "out/generated"',
        "",
        "[layout]",
        'strategy = "domain_first"',
        "one_top_level_node_per_file = true",
        "forbid_global_kind_dirs = true",
        "",
        "[build]",
        'out_dir = "out/build"',
        "",
        "[check]",
        "require_strip_assist_equivalence = true",
        "forbid_implicit_imports = true",
        "forbid_shadowing = true",
        "require_explicit_cross_domain_boundary = true",
      ].join("\n"),
      "utf8",
    );

    await expect(loadWorkspaceConfig(root)).rejects.toThrow(
      "Missing required sophia.toml value: build.target",
    );
  });

  it("requires explicit layout and check config", async () => {
    const root = await createTempDir("sophia-workspace-");
    await writeFile(
      path.join(root, "sophia.toml"),
      [
        "[source]",
        'domain_root = "domains"',
        'generated_dir = "sophia-runs/generated"',
        "",
        "[build]",
        'target = "typescript"',
        'out_dir = "sophia-runs/build"',
      ].join("\n"),
      "utf8",
    );

    await expect(loadWorkspaceConfig(root)).rejects.toThrow(
      "Missing required sophia.toml value: layout.strategy",
    );
  });

  it("loads complete source and build config", async () => {
    const root = await createTempDir("sophia-workspace-");
    await writeFile(
      path.join(root, "sophia.toml"),
      [
        "[source]",
        'domain_root = "src/domains"',
        'generated_dir = "out/generated"',
        "",
        "[layout]",
        'strategy = "domain_first"',
        "one_top_level_node_per_file = true",
        "forbid_global_kind_dirs = true",
        "",
        "[build]",
        'target = "typescript"',
        'out_dir = "out/build"',
        "",
        "[check]",
        "require_strip_assist_equivalence = true",
        "forbid_implicit_imports = false",
        "forbid_shadowing = true",
        "require_explicit_cross_domain_boundary = true",
      ].join("\n"),
      "utf8",
    );

    await expect(loadWorkspaceConfig(root)).resolves.toEqual({
      source: {
        domain_root: "src/domains",
        generated_dir: "out/generated",
      },
      layout: {
        strategy: "domain_first",
        one_top_level_node_per_file: true,
        forbid_global_kind_dirs: true,
      },
      build: {
        target: "typescript",
        out_dir: "out/build",
      },
      check: {
        require_strip_assist_equivalence: true,
        forbid_implicit_imports: false,
        forbid_shadowing: true,
        require_explicit_cross_domain_boundary: true,
      },
    });
  });

  it("loads the standard boolean workspace config", async () => {
    const root = await createTempDir("sophia-workspace-");
    await initWorkspace(root);

    await expect(loadWorkspaceConfig(root)).resolves.toMatchObject({
      layout: {
        one_top_level_node_per_file: true,
        forbid_global_kind_dirs: true,
      },
      check: {
        require_strip_assist_equivalence: true,
        forbid_implicit_imports: true,
        forbid_shadowing: true,
        require_explicit_cross_domain_boundary: true,
      },
    });
  });

  it("ignores comments outside strings", async () => {
    const root = await createTempDir("sophia-workspace-");
    await writeFile(
      path.join(root, "sophia.toml"),
      [
        "[source]",
        'domain_root = "src#domains" # comment',
        'generated_dir = "out/generated"',
        "",
        "[layout]",
        'strategy = "domain_first"',
        "one_top_level_node_per_file = true",
        "forbid_global_kind_dirs = true",
        "",
        "[build]",
        'target = "typescript"',
        'out_dir = "out/build"',
        "",
        "[check]",
        "require_strip_assist_equivalence = true",
        "forbid_implicit_imports = true",
        "forbid_shadowing = true",
        "require_explicit_cross_domain_boundary = true",
      ].join("\n"),
      "utf8",
    );

    await expect(loadWorkspaceConfig(root)).resolves.toMatchObject({
      source: { domain_root: "src#domains" },
    });
  });
});

describe("sophiaTomlTemplate", () => {
  it("escapes project names in TOML strings", () => {
    expect(sophiaTomlTemplate('a"b')).toContain('name = "a\\"b"');
  });
});

describe("readSophiaFilesFromDomains", () => {
  it("treats a missing domains directory as an empty source set", async () => {
    const root = await createTempDir("sophia-empty-domains-");
    await initWorkspace(root);
    await rm(path.join(root, "domains"), { recursive: true, force: true });

    await expect(readSophiaFilesFromDomains(root)).resolves.toEqual({});
  });
});
