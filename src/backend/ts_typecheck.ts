import path from "node:path";
import ts from "typescript";
import { errorDiagnostic, type Diagnostic } from "../lang/ast/diagnostics.js";
import { SOPHIA_BUILD_DIR } from "../workspace/fs_layout.js";

export interface TypeScriptTypecheckResult {
  ok: boolean;
  source: string;
  diagnostics: Diagnostic[];
}

export function typecheckGeneratedTypeScript(
  root: string,
  sourcePath = `${SOPHIA_BUILD_DIR}/index.ts`,
): TypeScriptTypecheckResult {
  const absoluteSourcePath = path.join(root, sourcePath);
  const program = ts.createProgram([absoluteSourcePath], {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.NodeNext,
    moduleResolution: ts.ModuleResolutionKind.NodeNext,
    strict: true,
    skipLibCheck: true,
    noEmit: true,
  });
  const diagnostics = ts
    .getPreEmitDiagnostics(program)
    .filter((diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error)
    .map((diagnostic) =>
      errorDiagnostic(
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
