import type { Diagnostic } from "../lang/diagnostics.js";
import type { PseudocodeCheckResult } from "./check.js";
import { extractNamedSection } from "../lang/braces.js";
import { outlinePseudocode } from "./outline.js";
import { hasPseudocodeSection } from "./document.js";

export interface PseudoRepairContext {
  diagnostic_summary: Array<{
    code: string;
    count: number;
    severity: Diagnostic["severity"];
  }>;
  missing_sections: string[];
  weak_checks: string[];
  outline: ReturnType<typeof outlinePseudocode>;
  sections_present: string[];
}

export function buildPseudoRepairContext(options: {
  pseudocode: string;
  checkResult: PseudocodeCheckResult;
}): PseudoRepairContext {
  const summary = new Map<
    string,
    { code: string; count: number; severity: Diagnostic["severity"] }
  >();
  for (const diagnostic of options.checkResult.diagnostics) {
    const existing = summary.get(diagnostic.code);
    if (existing) {
      existing.count += 1;
    } else {
      summary.set(diagnostic.code, {
        code: diagnostic.code,
        count: 1,
        severity: diagnostic.severity,
      });
    }
  }

  return {
    diagnostic_summary: [...summary.values()].sort((left, right) =>
      left.code.localeCompare(right.code),
    ),
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
    ].filter(
      (section) =>
        extractNamedSection(options.pseudocode, section) !== null ||
        hasPseudocodeSection(options.pseudocode, section),
    ),
  };
}
