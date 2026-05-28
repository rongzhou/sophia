export type DiagnosticSeverity = "error" | "warning" | "info";

export interface Diagnostic {
  code: string;
  severity: DiagnosticSeverity;
  problem: string;
  repair?: string;
  location?: string;
}

export interface CheckResult {
  ok: boolean;
  diagnostics: Diagnostic[];
}

export interface DiagnosticSummaryItem {
  code: string;
  count: number;
  severity: DiagnosticSeverity;
}

export function diagnostic(input: {
  code: string;
  severity: DiagnosticSeverity;
  problem: string;
  repair?: string;
  location?: string;
}): Diagnostic {
  return {
    code: input.code,
    severity: input.severity,
    problem: input.problem,
    ...(input.repair ? { repair: input.repair } : {}),
    ...(input.location ? { location: input.location } : {}),
  };
}

export function errorDiagnostic(
  code: string,
  location: string | undefined,
  problem: string,
  repair?: string,
): Diagnostic {
  return diagnostic({
    code,
    severity: "error",
    problem,
    ...(location ? { location } : {}),
    ...(repair ? { repair } : {}),
  });
}

export function warningDiagnostic(
  code: string,
  location: string | undefined,
  problem: string,
  repair?: string,
): Diagnostic {
  return diagnostic({
    code,
    severity: "warning",
    problem,
    ...(location ? { location } : {}),
    ...(repair ? { repair } : {}),
  });
}

export function hasErrors(result: CheckResult): boolean {
  return result.diagnostics.some((diagnostic) => diagnostic.severity === "error");
}

export function summarizeDiagnostics(diagnostics: Diagnostic[]): DiagnosticSummaryItem[] {
  const summary = new Map<string, DiagnosticSummaryItem>();
  for (const diagnostic of diagnostics) {
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
  return [...summary.values()].sort((left, right) => left.code.localeCompare(right.code));
}
