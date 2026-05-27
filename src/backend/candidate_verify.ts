import { mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { buildTypeScript, type TypeScriptBuildResult } from "./ts_codegen.js";
import { type TypeScriptTypecheckResult, typecheckGeneratedTypeScript } from "./ts_typecheck.js";
import { sophiaTomlTemplate } from "../workspace/workspace.js";
import { createScratchDirectory, SOPHIA_BUILD_DIR } from "../workspace/fs_layout.js";

export interface CandidateTypeScriptVerificationResult {
  ok: boolean;
  build: TypeScriptBuildResult;
  typecheck: TypeScriptTypecheckResult | null;
}

export async function verifyCandidateTypeScriptBuild(
  sourceFiles: Record<string, string>,
): Promise<CandidateTypeScriptVerificationResult> {
  const root = await createScratchDirectory(process.cwd(), "candidate");
  try {
    await writeFile(path.join(root, "sophia.toml"), `${sophiaTomlTemplate("candidate")}\n`, "utf8");
    for (const [filePath, content] of Object.entries(sourceFiles)) {
      const absolutePath = path.join(root, filePath);
      await mkdir(path.dirname(absolutePath), { recursive: true });
      await writeFile(absolutePath, content, "utf8");
    }

    const build = await buildTypeScript(root);
    const typecheck = build.ok
      ? typecheckGeneratedTypeScript(root, build.files[0] ?? `${SOPHIA_BUILD_DIR}/index.ts`)
      : null;
    return {
      ok: build.ok && (typecheck?.ok ?? false),
      build,
      typecheck,
    };
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}
