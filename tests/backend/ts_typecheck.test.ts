import { describe, expect, it } from "vitest";
import { buildTypeScript } from "../../src/backend/ts_codegen.js";
import { typecheckGeneratedTypeScript } from "../../src/backend/ts_typecheck.js";
import {
  createSophiaWorkspaceWithDemoDomain,
  writeProjectFile,
} from "../helpers/sophia_workspace.js";

describe("typecheckGeneratedTypeScript", () => {
  it("strictly typechecks the generated TypeScript entrypoint", async () => {
    const root = await createSophiaWorkspaceWithDemoDomain("sophia-generated-typecheck-");
    await writeProjectFile(
      root,
      "domains/Demo/actions/DoubleInput.sophia",
      `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
    );

    const build = await buildTypeScript(root);
    const result = typecheckGeneratedTypeScript(root, build.files[0]);

    expect(build.ok).toBe(true);
    expect(result).toEqual({
      ok: true,
      source: "sophia-runs/build/index.ts",
      diagnostics: [],
    });
  });
});
