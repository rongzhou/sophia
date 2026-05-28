import type { CheckResult, Diagnostic, DiagnosticSummaryItem } from "../lang/ast/diagnostics.js";
import { summarizeDiagnostics } from "../lang/ast/diagnostics.js";

export interface RepairContext {
  diagnostic_summary: DiagnosticSummaryItem[];
  affected_files: Array<{
    path: string;
    diagnostics: Diagnostic[];
    snippets: Array<{
      line: number | null;
      text: string;
    }>;
  }>;
}

export function buildRepairContext(options: {
  files: Record<string, string>;
  checkResult: CheckResult;
}): RepairContext {
  const diagnosticsByFile = new Map<string, Diagnostic[]>();

  for (const diagnostic of options.checkResult.diagnostics) {
    const path = diagnostic.location ? parseLocation(diagnostic.location).path : null;
    if (!path || !(path in options.files)) continue;
    diagnosticsByFile.set(path, [...(diagnosticsByFile.get(path) ?? []), diagnostic]);
  }

  return {
    diagnostic_summary: summarizeDiagnostics(options.checkResult.diagnostics),
    affected_files: [...diagnosticsByFile.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([filePath, diagnostics]) => ({
        path: filePath,
        diagnostics,
        snippets: diagnostics.flatMap((diagnostic) =>
          snippetForDiagnostic(options.files[filePath] ?? "", diagnostic),
        ),
      })),
  };
}

function snippetForDiagnostic(
  content: string,
  diagnostic: Diagnostic,
): Array<{ line: number | null; text: string }> {
  if (!diagnostic.location) {
    return [];
  }
  const { line } = parseLocation(diagnostic.location);
  if (!line) {
    const inferred = inferredSnippetsForDiagnostic(content, diagnostic);
    return inferred.length > 0 ? inferred : [{ line: null, text: firstNonEmptyLine(content) }];
  }
  const lines = content.split("\n");
  const start = Math.max(1, line - 1);
  const end = Math.min(lines.length, line + 1);
  const snippets = [];
  for (let current = start; current <= end; current += 1) {
    snippets.push({ line: current, text: lines[current - 1]?.trimEnd() ?? "" });
  }
  return snippets;
}

function inferredSnippetsForDiagnostic(
  content: string,
  diagnostic: Diagnostic,
): Array<{ line: number | null; text: string }> {
  const patterns = patternsForDiagnostic(diagnostic.code);
  if (patterns.length === 0) return [];
  const lines = content.split("\n");
  const snippets: Array<{ line: number | null; text: string }> = [];
  lines.forEach((line, index) => {
    if (patterns.some((pattern) => pattern.test(line))) {
      snippets.push({ line: index + 1, text: line.trimEnd() });
    }
  });
  return snippets.slice(0, 5);
}

function patternsForDiagnostic(code: string): RegExp[] {
  switch (code) {
    case "CHECK-SYNTAX-006":
      return [/\bvar\s+\w+/];
    case "CHECK-BODY-002":
      return [/\bConsole\.Write\s*\(/];
    case "CHECK-BODY-003":
      return [/(^|[^\w.])append\s*\(/];
    case "CHECK-EFFECT-001":
      return [/\bprint\s+/, /\bConsole\.Write\s*\(/];
    default:
      return [];
  }
}

function parseLocation(location: string): { path: string; line: number | null } {
  const match = /^(.*?)(?::(\d+))?$/.exec(location);
  return {
    path: match?.[1] ?? location,
    line: match?.[2] ? Number(match[2]) : null,
  };
}

function firstNonEmptyLine(content: string): string {
  return (
    content
      .split("\n")
      .map((line) => line.trim())
      .find(Boolean) ?? ""
  );
}
