import { error } from "./diagnostics.js";
import { stripQuotedText } from "./braces.js";
import { hasBalancedSophiaBraces, parseSophiaTopLevelDeclarations } from "./parser.js";
import {
  expectedTopLevelKindForPath,
  expectedTopLevelPathInfo,
  isPascalCaseSophiaName,
  isSupportedSophiaFilePath,
} from "../workspace/sophia_paths.js";
import type { CheckerContext } from "./checker_context.js";

export type TopLevelDeclaration = ReturnType<typeof parseSophiaTopLevelDeclarations>[number];

const SOPHIA_V0_TOP_LEVEL_KINDS = [
  "action",
  "capability",
  "domain",
  "entity",
  "error",
  "state",
  "storage",
] as const;

export function checkFileLayout(
  context: CheckerContext,
  filePath: string,
  content: string,
  topLevelBlocks: TopLevelDeclaration[],
): void {
  if (!isSupportedSophiaFilePath(filePath)) {
    context.diagnostics.push(
      error(
        "CHECK-FILE-001",
        filePath,
        "Generated file path is outside the v0 domain entity action/capability layout.",
        "Use domains/<Domain>/domain.sophia or a supported node directory such as entities, actions, capabilities, states, storages, or errors.",
      ),
    );
  }
  if (!hasBalancedSophiaBraces(content)) {
    context.diagnostics.push(
      error(
        "CHECK-SYNTAX-008",
        filePath,
        "Unbalanced braces in .sophia file.",
        "Ensure every opening brace has a matching closing brace.",
      ),
    );
  }
  if (topLevelBlocks.length === 0) {
    context.diagnostics.push(
      error("CHECK-FILE-003", filePath, "Sophia v0 file has no top-level node."),
    );
  }
  if (topLevelBlocks.length > 1) {
    context.diagnostics.push(
      error(
        "CHECK-FILE-004",
        filePath,
        "Sophia v0 file contains more than one top-level node.",
        "Write exactly one domain, capability, entity, state, error, storage, or action node per .sophia file.",
      ),
    );
  }
  const pathKind = expectedTopLevelKindForPath(filePath);
  if (pathKind && topLevelBlocks[0] && topLevelBlocks[0].kind !== pathKind) {
    context.diagnostics.push(
      error(
        "CHECK-FILE-005",
        filePath,
        `File path expects a ${pathKind} node, but found ${topLevelBlocks[0].kind}.`,
      ),
    );
  }
  const pathInfo = expectedTopLevelPathInfo(filePath);
  if (
    pathInfo &&
    topLevelBlocks[0] &&
    topLevelBlocks[0].kind === pathInfo.kind &&
    topLevelBlocks[0].name !== pathInfo.name
  ) {
    context.diagnostics.push(
      error(
        "CHECK-FILE-006",
        filePath,
        `File path expects ${pathInfo.kind} name ${pathInfo.name}, but found ${topLevelBlocks[0].name}.`,
        "Keep the declared top-level name identical to the v0 file path node name.",
      ),
    );
  }
}

export function checkTopLevelNames(
  context: CheckerContext,
  filePath: string,
  topLevelBlocks: TopLevelDeclaration[],
): void {
  for (const topLevel of topLevelBlocks) {
    if (!isSophiaV0TopLevelKind(topLevel.kind)) {
      context.diagnostics.push(
        error(
          "CHECK-SYNTAX-007",
          filePath,
          `Unsupported top-level Sophia v0 block: ${topLevel.kind}.`,
          "Use only domain, entity, state, error, storage, capability, and action top-level blocks.",
        ),
      );
    }
    if (isSophiaV0TopLevelKind(topLevel.kind) && !isPascalCaseSophiaName(topLevel.name)) {
      context.diagnostics.push(
        error(
          "CHECK-NAME-001",
          filePath,
          `Top-level ${topLevel.kind} name must be PascalCase: ${topLevel.name}.`,
          "Use PascalCase for domain, entity, state, error, storage, capability, and action names.",
        ),
      );
    }
    if (isSophiaV0TopLevelKind(topLevel.kind)) {
      const existing = context.topLevelNames.get(topLevel.name);
      if (existing) {
        context.diagnostics.push(
          error(
            "CHECK-NAME-002",
            filePath,
            `Top-level name ${topLevel.name} is already used by ${existing.kind} in ${existing.path}.`,
            "Use globally unique PascalCase names for Sophia v0 ASG nodes.",
          ),
        );
      } else {
        context.topLevelNames.set(topLevel.name, { kind: topLevel.kind, path: filePath });
      }
    }
  }
}

function isSophiaV0TopLevelKind(kind: string): boolean {
  return (SOPHIA_V0_TOP_LEVEL_KINDS as readonly string[]).includes(kind);
}

export function checkUnsupportedSyntax(
  context: CheckerContext,
  filePath: string,
  content: string,
): void {
  const structuralSource = stripQuotedText(content);
  const forLine = firstMatchingLine(structuralSource, /\bfor\s*\(|\bfor\s+\w+\s+in\b/);
  if (forLine !== null) {
    context.diagnostics.push(
      error(
        "CHECK-SYNTAX-004",
        `${filePath}:${forLine}`,
        "Unsupported loop syntax: for.",
        "Use repeat N times.",
      ),
    );
  }
  const whileLine = firstMatchingLine(structuralSource, /\bwhile\s*\(|\bwhile\s+/);
  if (whileLine !== null) {
    context.diagnostics.push(
      error(
        "CHECK-SYNTAX-004",
        `${filePath}:${whileLine}`,
        "Unsupported loop syntax: while.",
        "Use repeat N times.",
      ),
    );
  }
  const varLine = firstMatchingLine(structuralSource, /\bvar\s+\w+/);
  if (varLine !== null) {
    context.diagnostics.push(
      error(
        "CHECK-SYNTAX-006",
        `${filePath}:${varLine}`,
        "Unsupported mutable declaration syntax: var.",
        "Use let mutable name = expr and set name = expr for reassignment.",
      ),
    );
  }
}

function firstMatchingLine(content: string, pattern: RegExp): number | null {
  const lines = content.split("\n");
  for (let index = 0; index < lines.length; index += 1) {
    if (pattern.test(lines[index] ?? "")) return index + 1;
  }
  return null;
}
