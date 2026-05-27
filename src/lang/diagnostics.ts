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

export function error(
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

export function warning(
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
