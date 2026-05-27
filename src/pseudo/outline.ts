import { extractNamedSection } from "../lang/braces.js";
import { pseudocodeAlgorithmLines } from "./document.js";

export interface PseudoOutline {
  algorithm_steps: string[];
  repeats: string[];
  branches: string[];
  mutable_state_candidates: string[];
}

export function outlinePseudocode(content: string): PseudoOutline {
  const lines =
    pseudocodeAlgorithmLines(content) ??
    (extractNamedSection(content, "algorithm") ?? "")
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
  return {
    algorithm_steps: lines,
    repeats: lines.filter((line) => line.startsWith("repeat ")),
    branches: lines.filter((line) => line.startsWith("if ") || line.startsWith("} else")),
    mutable_state_candidates: extractMutableStateCandidates(lines),
  };
}

function extractMutableStateCandidates(lines: string[]): string[] {
  const assignmentCounts = new Map<string, number>();
  for (const line of lines) {
    const match = /^set\s+([a-z_]\w*)\s+to\b/i.exec(line);
    if (!match?.[1]) continue;
    assignmentCounts.set(match[1], (assignmentCounts.get(match[1]) ?? 0) + 1);
  }
  return [...assignmentCounts.entries()]
    .filter(([, count]) => count > 1)
    .map(([name]) => name)
    .sort();
}
