import { errorDiagnostic, type Diagnostic } from "../lang/ast/diagnostics.js";
import { stripSemanticAssistFromFiles } from "../lang/ast/strip_assist.js";
import { emitTypeScript, type SophiaSourceFile } from "./ts_emit_module.js";

export interface StripAssistEquivalenceResult {
  ok: boolean;
  diagnostics: Diagnostic[];
}

export function checkStripAssistTypeScriptEquivalence(
  parsedFiles: SophiaSourceFile[],
): StripAssistEquivalenceResult {
  const emitted = emitTypeScript(parsedFiles);
  const strippedEmitted = emitTypeScript(stripSemanticAssistFromFiles(parsedFiles));
  if (strippedEmitted === emitted) {
    return {
      ok: true,
      diagnostics: [],
    };
  }
  return {
    ok: false,
    diagnostics: [
      errorDiagnostic(
        "BUILD-STRIP-ASSIST-001",
        "<strip-assist>",
        "Removing Semantic Assist attributes changed the generated TypeScript artifact.",
      ),
    ],
  };
}
