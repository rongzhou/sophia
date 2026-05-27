import { error, type Diagnostic } from "../lang/diagnostics.js";
import { stripSemanticAssistFromFiles } from "../lang/strip_assist.js";
import { emitTypeScript, type SophiaSourceFile } from "./ts_emit_module.js";

export type StripAssistEquivalenceDiagnostic = Diagnostic;

export interface StripAssistEquivalenceResult {
  ok: boolean;
  diagnostics: StripAssistEquivalenceDiagnostic[];
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
      error(
        "BUILD-STRIP-ASSIST-001",
        "<strip-assist>",
        "Removing Semantic Assist attributes changed the generated TypeScript artifact.",
      ),
    ],
  };
}
