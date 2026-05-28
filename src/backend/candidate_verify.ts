import { buildTypeScript, type TypeScriptBuildResult } from "./ts_codegen.js";
import { type TypeScriptTypecheckResult, typecheckGeneratedTypeScript } from "./ts_typecheck.js";
import { SOPHIA_BUILD_DIR, withSophiaScratchWorkspace } from "../workspace/fs_layout.js";

export interface CandidateTypeScriptVerificationResult {
  ok: boolean;
  build: TypeScriptBuildResult;
  typecheck: TypeScriptTypecheckResult | null;
}

export async function verifyCandidateTypeScriptBuild(
  sourceFiles: Record<string, string>,
): Promise<CandidateTypeScriptVerificationResult> {
  return withSophiaScratchWorkspace({
    root: process.cwd(),
    label: "candidate",
    projectName: "candidate",
    files: sourceFiles,
    run: async (root) => {
      const build = await buildTypeScript(root);
      const typecheck = build.ok
        ? typecheckGeneratedTypeScript(root, build.files[0] ?? `${SOPHIA_BUILD_DIR}/index.ts`)
        : null;
      return {
        ok: build.ok && (typecheck?.ok ?? false),
        build,
        typecheck,
      };
    },
  });
}
