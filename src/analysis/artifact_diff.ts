import type { CheckResult, Diagnostic } from "../lang/ast/diagnostics.js";
import { errorDiagnostic, warningDiagnostic } from "../lang/ast/diagnostics.js";
import { parseSophiaImmediateNamedBlocks, parseSophiaTopLevelDeclarations } from "../lang/ast/parser.js";
import { parseSophiaEffectNames } from "../lang/ast/signature.js";

export interface ArtifactDiffResult extends CheckResult {
  files: {
    added: string[];
    removed: string[];
    changed: string[];
    unchanged: string[];
  };
  stats: {
    before_files: number;
    after_files: number;
    changed_files: number;
    removed_lines: number;
    added_lines: number;
  };
}

export function diffSophiaArtifacts(options: {
  before: Record<string, string>;
  after: Record<string, string>;
}): ArtifactDiffResult {
  const diagnostics: Diagnostic[] = [];
  const beforePaths = new Set(
    Object.keys(options.before).filter((filePath) => filePath.endsWith(".sophia")),
  );
  const afterPaths = new Set(
    Object.keys(options.after).filter((filePath) => filePath.endsWith(".sophia")),
  );
  const added = [...afterPaths].filter((filePath) => !beforePaths.has(filePath)).sort();
  const removed = [...beforePaths].filter((filePath) => !afterPaths.has(filePath)).sort();
  const shared = [...beforePaths].filter((filePath) => afterPaths.has(filePath)).sort();
  const changed = shared.filter((filePath) => options.before[filePath] !== options.after[filePath]);
  const unchanged = shared.filter(
    (filePath) => options.before[filePath] === options.after[filePath],
  );

  for (const filePath of removed) {
    diagnostics.push(errorDiagnostic("DIFF-FILE-001", filePath, "Repair removed a .sophia file."));
  }

  const beforeCombined = Object.values(options.before).join("\n");
  const afterCombined = Object.values(options.after).join("\n");
  for (const action of extractNamedBlocks(beforeCombined, "action")) {
    if (!extractNamedBlocks(afterCombined, "action").has(action)) {
      diagnostics.push(
        errorDiagnostic("DIFF-ACTION-001", action, `Repair removed action declaration: ${action}.`),
      );
    }
  }
  for (const capability of extractNamedBlocks(beforeCombined, "capability")) {
    if (!extractNamedBlocks(afterCombined, "capability").has(capability)) {
      diagnostics.push(
        errorDiagnostic(
          "DIFF-CAPABILITY-001",
          capability,
          `Repair removed capability declaration: ${capability}.`,
        ),
      );
    }
  }
  for (const effect of extractEffects(beforeCombined)) {
    if (!extractEffects(afterCombined).has(effect)) {
      diagnostics.push(
        errorDiagnostic("DIFF-EFFECT-001", effect, `Repair removed effect reference: ${effect}.`),
      );
    }
  }

  const lineStats = changed.reduce(
    (stats, filePath) => {
      const beforeLines = normalizeLines(options.before[filePath] ?? "");
      const afterLines = normalizeLines(options.after[filePath] ?? "");
      return {
        removed_lines: stats.removed_lines + countMissingLines(beforeLines, afterLines),
        added_lines: stats.added_lines + countMissingLines(afterLines, beforeLines),
      };
    },
    { removed_lines: 0, added_lines: 0 },
  );

  if (lineStats.removed_lines + lineStats.added_lines > 40) {
    diagnostics.push(
      warningDiagnostic(
        "DIFF-SIZE-001",
        "<files>",
        "Repair made a large textual change.",
        "Review whether the repair preserved the original task constraints.",
      ),
    );
  }

  return {
    ok: diagnostics.every((diagnostic) => diagnostic.severity !== "error"),
    diagnostics,
    files: { added, removed, changed, unchanged },
    stats: {
      before_files: beforePaths.size,
      after_files: afterPaths.size,
      changed_files: changed.length,
      removed_lines: lineStats.removed_lines,
      added_lines: lineStats.added_lines,
    },
  };
}

function extractNamedBlocks(content: string, kind: "action" | "capability"): Set<string> {
  return new Set(
    [...content.matchAll(new RegExp(`\\b${kind}\\s+([A-Za-z_]\\w*)\\s*\\{`, "g"))]
      .map((match) => match[1])
      .filter((value): value is string => Boolean(value)),
  );
}

function extractEffects(content: string): Set<string> {
  const effects = new Set<string>();
  for (const declaration of parseSophiaTopLevelDeclarations(content)) {
    if (declaration.kind !== "action") continue;
    for (const block of parseSophiaImmediateNamedBlocks(declaration.body)) {
      if (block.name !== "effects") continue;
      for (const effect of parseSophiaEffectNames(block.body)) {
        effects.add(effect);
      }
    }
  }
  return effects;
}

function normalizeLines(content: string): string[] {
  return content
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function countMissingLines(source: string[], target: string[]): number {
  const remaining = new Map<string, number>();
  for (const line of target) {
    remaining.set(line, (remaining.get(line) ?? 0) + 1);
  }
  let count = 0;
  for (const line of source) {
    const available = remaining.get(line) ?? 0;
    if (available > 0) {
      remaining.set(line, available - 1);
    } else {
      count += 1;
    }
  }
  return count;
}
