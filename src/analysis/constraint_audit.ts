import type { CheckResult, Diagnostic } from "../lang/ast/diagnostics.js";
import { stripQuotedText } from "../lang/ast/braces.js";
import { parsePseudocodeJson } from "../pseudo/document.js";
import { escapeRegExp } from "../util/strings.js";

export function auditConstraints(options: {
  pseudocode: string;
  files: Record<string, string>;
}): CheckResult {
  const diagnostics: Diagnostic[] = [];
  const combined = Object.values(options.files).join("\n");
  const structuralCombined = stripQuotedText(combined);

  const forbiddenChecks: Array<[RegExp, string]> = [
    [/\bstorage\b|\bDB\./i, "storage"],
    [/\bTime\b|\bnow\s*\(/i, "time"],
    [/\bNetwork\b|\bfetch\s*\(/i, "network"],
    [/\brandom\b|\bMath\.random\b/i, "randomness"],
    [/\bfor\s*\(|\bfor\s+\w+\s+in\b/i, "for"],
    [/\bwhile\s*\(|\bwhile\s+/i, "while"],
    [/\bprint\s+|\bConsole\.Write\s*\(/i, "print"],
  ];

  for (const [pattern, label] of forbiddenChecks) {
    if (mentionsForbidden(options.pseudocode, label) && pattern.test(structuralCombined)) {
      diagnostics.push({
        code: "AUDIT-FORBIDDEN-001",
        severity: "error",
        problem: `Generated .sophia appears to violate forbidden constraint: ${label}.`,
        repair: `Remove ${label} usage and preserve the .pseudo forbidden constraints.`,
      });
    }
  }

  if (/Do not hardcode the full list/i.test(options.pseudocode)) {
    for (const expectedList of extractExpectedLists(options.pseudocode)) {
      if (expectedList.length < 3) continue;
      if (containsListLiteral(combined, expectedList)) {
        diagnostics.push({
          code: "AUDIT-HARDCODE-001",
          severity: "error",
          problem: "Generated .sophia appears to hardcode a full expected result list.",
          repair:
            "Compute the result using algorithmic state updates instead of embedding the full list.",
        });
      }
    }
  }

  if (/Do not hardcode the result|Do not hardcode .*direct return/i.test(options.pseudocode)) {
    for (const scalar of extractExpectedScalars(options.pseudocode)) {
      if (containsDirectScalarReturn(combined, scalar)) {
        diagnostics.push({
          code: "AUDIT-HARDCODE-002",
          severity: "error",
          problem:
            "Generated .sophia appears to hardcode an expected scalar result as a direct return.",
          repair:
            "Compute the result using algorithmic state updates instead of directly returning the expected scalar.",
        });
      }
    }
  }

  for (const repeatCount of extractRepeatCounts(options.pseudocode)) {
    const pattern = new RegExp(`repeat\\s+${escapeRegExp(repeatCount)}\\s+times`, "i");
    if (!pattern.test(combined)) {
      diagnostics.push({
        code: "AUDIT-LOOP-001",
        severity: "warning",
        problem: `The .pseudo uses repeat ${repeatCount} times, but generated .sophia does not preserve that loop.`,
        repair: "Preserve the bounded loop unless a formally equivalent construct is supported.",
      });
    }
  }

  return {
    ok: diagnostics.every((diagnostic) => diagnostic.severity !== "error"),
    diagnostics,
  };
}

function mentionsForbidden(pseudocode: string, label: string): boolean {
  return new RegExp(`Do not use ${label}|Do not ${label}`, "i").test(pseudocode);
}

function extractExpectedLists(pseudocode: string): string[][] {
  const lists: string[][] = [];
  for (const match of pseudocode.matchAll(/\[([^\]]+)\]/g)) {
    const values = match[1]
      ?.split(",")
      .map((value) => value.trim())
      .filter(Boolean);
    if (values && values.length > 0) {
      lists.push(values);
    }
  }
  return lists;
}

function extractRepeatCounts(pseudocode: string): string[] {
  return [...pseudocode.matchAll(/\brepeat\s+(\d+)\s+times\b/gi)]
    .map((match) => match[1])
    .filter((value): value is string => Boolean(value));
}

function extractExpectedScalars(pseudocode: string): string[] {
  const parsed = parsePseudocodeJson(pseudocode);
  if (parsed?.expected && typeof parsed.expected === "object" && !Array.isArray(parsed.expected)) {
    return Object.values(parsed.expected)
      .filter((value): value is string => typeof value === "string")
      .map((value) => value.trim())
      .filter((value) => Boolean(value && !value.includes("[") && value.length <= 40));
  }
  const expectedBlock = /\bexpected\s*\{([\s\S]*?)\}/i.exec(pseudocode)?.[1] ?? "";
  return [...expectedBlock.matchAll(/:=\s*"([^"]+)"/g)]
    .map((match) => match[1]?.trim())
    .filter((value): value is string =>
      Boolean(value && !value.includes("[") && value.length <= 40),
    );
}

function containsDirectScalarReturn(content: string, scalar: string): boolean {
  return new RegExp(`\\breturn\\s+["']?${escapeRegExp(scalar)}["']?\\b`, "i").test(content);
}

function containsListLiteral(content: string, values: string[]): boolean {
  const escaped = values.map((value) => escapeRegExp(value));
  const looseList = new RegExp(`\\[?\\s*${escaped.join("\\s*,\\s*")}\\s*\\]?`);
  return looseList.test(content);
}
