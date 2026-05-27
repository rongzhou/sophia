import { readFile } from "node:fs/promises";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { buildAsgIndex } from "../../src/analysis/indexer.js";
import {
  createSophiaWorkspace,
  createTempDir,
  writeProjectFile,
  writeSophiaToml,
} from "../helpers/sophia_workspace.js";

describe("buildAsgIndex", () => {
  it("generates a stable sorted ASG index for materialized domain files", async () => {
    const root = await createSophiaWorkspace("sophia-index-");
    await writeProjectFile(root, "domains/Demo/actions/Hello.sophia", "action Hello {}\n");
    await writeProjectFile(
      root,
      "domains/Demo/entities/Account.sophia",
      "entity Account { fields { balance: Int } }\n",
    );
    await writeProjectFile(
      root,
      "domains/Demo/capabilities/ConsoleCapability.sophia",
      "capability ConsoleCapability { allow { Console.Write } }\n",
    );
    await writeProjectFile(
      root,
      "domains/Demo/storages/Accounts.sophia",
      "storage Accounts { key: Persisted<Text> value: Account }\n",
    );
    await writeProjectFile(
      root,
      "domains/Demo/errors/AccountError.sophia",
      "error AccountError { variant InvalidAmount { amount: Int } }\n",
    );
    await writeProjectFile(
      root,
      "domains/Demo/states/TodoStatus.sophia",
      "state TodoStatus { value Pending { } value Done { } }\n",
    );
    await writeProjectFile(root, "domains/Demo/domain.sophia", "domain Demo {}\n");

    const result = await buildAsgIndex(root);
    const output = JSON.parse(
      await readFile(path.join(root, "sophia-runs/generated/asg_index.json"), "utf8"),
    );

    expect(result.ok).toBe(true);
    expect(result.diagnostics).toEqual([]);
    expect(output).toEqual({
      nodes: {
        Account: {
          domain: "Demo",
          kind: "Entity",
          path: "domains/Demo/entities/Account.sophia",
        },
        Accounts: {
          domain: "Demo",
          kind: "Storage",
          path: "domains/Demo/storages/Accounts.sophia",
        },
        AccountError: {
          domain: "Demo",
          kind: "Error",
          path: "domains/Demo/errors/AccountError.sophia",
        },
        ConsoleCapability: {
          domain: "Demo",
          kind: "Capability",
          path: "domains/Demo/capabilities/ConsoleCapability.sophia",
        },
        Demo: {
          domain: "Demo",
          kind: "Domain",
          path: "domains/Demo/domain.sophia",
        },
        Hello: {
          domain: "Demo",
          kind: "Action",
          path: "domains/Demo/actions/Hello.sophia",
        },
        TodoStatus: {
          domain: "Demo",
          kind: "State",
          path: "domains/Demo/states/TodoStatus.sophia",
        },
      },
      version: 1,
    });
    expect(Object.keys(output.nodes)).toEqual([
      "Account",
      "AccountError",
      "Accounts",
      "ConsoleCapability",
      "Demo",
      "Hello",
      "TodoStatus",
    ]);
  });

  it("reports duplicate top-level names without hiding the diagnostic", async () => {
    const root = await createSophiaWorkspace("sophia-index-");
    await writeProjectFile(root, "domains/One/actions/Demo.sophia", "action Demo {}\n");
    await writeProjectFile(root, "domains/Two/actions/Demo.sophia", "action Demo {}\n");

    const result = await buildAsgIndex(root);

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("INDEX-NODE-001");
  });

  it("reports files outside the v0 supported layout", async () => {
    const root = await createSophiaWorkspace("sophia-index-");
    await writeProjectFile(root, "domains/Demo/misc/Hello.sophia", "action Hello {}\n");

    const result = await buildAsgIndex(root);

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("INDEX-FILE-001");
  });

  it("honors configured domain root and generated directory", async () => {
    const root = await createTempDir("sophia-index-");
    await writeSophiaToml(root, {
      domainRoot: "src/domains",
      generatedDir: "out/generated",
      buildOutDir: "out/build",
    });
    await writeProjectFile(root, "src/domains/Demo/domain.sophia", "domain Demo {}\n");
    await writeProjectFile(root, "src/domains/Demo/actions/Hello.sophia", "action Hello {}\n");
    await writeProjectFile(
      root,
      "src/domains/Demo/capabilities/ConsoleCapability.sophia",
      "capability ConsoleCapability { allow { Console.Write } }\n",
    );

    const result = await buildAsgIndex(root);
    const output = JSON.parse(
      await readFile(path.join(root, "out/generated/asg_index.json"), "utf8"),
    );

    expect(result.ok).toBe(true);
    expect(result.output_path).toBe("out/generated/asg_index.json");
    expect(output.nodes.Hello).toEqual({
      domain: "Demo",
      kind: "Action",
      path: "src/domains/Demo/actions/Hello.sophia",
    });
  });

  it("uses parser diagnostics for invalid materialized files", async () => {
    const root = await createSophiaWorkspace("sophia-index-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/Broken.sophia",
      `
action Broken {
  capability: DemoCapability
  output { result: Unit }
  effects { }
  errors { }
  body { return unit }
  storage { todos }
}
`,
    );

    const result = await buildAsgIndex(root);

    expect(result.ok).toBe(false);
    expect(result.diagnostics).toContainEqual(
      expect.objectContaining({
        code: "INDEX-PARSE-001",
        problem: expect.stringContaining("PARSE-BLOCK-001"),
      }),
    );
  });
});
