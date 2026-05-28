import type { DiagnosticSummaryItem } from "../lang/ast/diagnostics.js";
import { summarizeDiagnostics } from "../lang/ast/diagnostics.js";
import type { PseudocodeCheckResult } from "./check.js";
import { outlinePseudocode } from "./outline.js";
import { hasPseudoSection } from "./document.js";

export interface PseudoRepairContext {
  diagnostic_summary: DiagnosticSummaryItem[];
  missing_sections: string[];
  weak_checks: string[];
  outline: ReturnType<typeof outlinePseudocode>;
  sections_present: string[];
}

export function buildPseudoRepairContext(options: {
  pseudocode: string;
  checkResult: PseudocodeCheckResult;
}): PseudoRepairContext {
  return {
    diagnostic_summary: summarizeDiagnostics(options.checkResult.diagnostics),
    missing_sections: Object.entries(options.checkResult.checks)
      .filter(([key, value]) => key.startsWith("has_") && !value)
      .map(([key]) => key.replace(/^has_/, "")),
    weak_checks: Object.entries(options.checkResult.checks)
      .filter(([key, value]) => !key.startsWith("has_") && !value)
      .map(([key]) => key),
    outline: outlinePseudocode(options.pseudocode),
    sections_present: [
      "purpose",
      "definitions",
      "inputs",
      "outputs",
      "effects",
      "algorithm",
      "constraints",
      "forbidden",
      "expected",
    ].filter((section) => hasPseudoSection(options.pseudocode, section)),
  };
}
