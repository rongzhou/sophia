import path from "node:path";
import ts from "typescript";
import { error } from "../lang/diagnostics.js";
import type { TypeScriptBuildDiagnostic } from "./ts_codegen.js";
import { SOPHIA_BUILD_DIR } from "../workspace/fs_layout.js";

export interface TypeScriptTypecheckResult {
  ok: boolean;
  source: string;
  diagnostics: TypeScriptBuildDiagnostic[];
}

export function typecheckGeneratedTypeScript(
  root: string,
  sourcePath = `${SOPHIA_BUILD_DIR}/index.ts`,
): TypeScriptTypecheckResult {
  const absoluteSourcePath = path.join(root, sourcePath);
  const program = ts.createProgram([absoluteSourcePath], {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ES2022,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    strict: true,
    skipLibCheck: true,
    noEmit: true,
  });
  const diagnostics = ts
    .getPreEmitDiagnostics(program)
    .filter((diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error)
    .map((diagnostic) =>
      error(
        "BUILD-TYPECHECK-001",
        diagnostic.file ? path.relative(root, diagnostic.file.fileName) : sourcePath,
        ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
      ),
    );
  return {
    ok: diagnostics.length === 0,
    source: sourcePath,
    diagnostics,
  };
}
