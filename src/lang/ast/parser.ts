import { readFile } from "node:fs/promises";
import { braceDepth, readBraceBody, stripQuotedText } from "./braces.js";
import { errorDiagnostic, type Diagnostic } from "./diagnostics.js";

export interface SophiaRawAst {
  kind: string;
  name: string;
  blocks: Array<{
    name: string;
    body: string;
  }>;
  attributes: Array<{
    name: string;
    value: string;
  }>;
}

export interface SophiaTopLevelDeclaration {
  kind: string;
  name: string;
  body: string;
}

export interface SophiaNamedBlock {
  name: string;
  body: string;
}

export interface SophiaAttribute {
  name: string;
  value: string;
}

export interface ParseSophiaFileResult {
  ok: boolean;
  path: string;
  ast: SophiaRawAst | null;
  diagnostics: Diagnostic[];
}

const SUPPORTED_TOP_LEVEL_KINDS = new Set([
  "domain",
  "entity",
  "capability",
  "action",
  "storage",
  "state",
  "error",
]);
const SUPPORTED_BLOCKS_BY_KIND: Record<string, Set<string> | null> = {
  action: new Set(["input", "output", "effects", "errors", "body"]),
  capability: new Set(["allow", "deny"]),
  domain: new Set(),
  entity: new Set(["fields"]),
  error: null,
  state: null,
  storage: new Set(),
};

export async function parseSophiaFile(filePath: string): Promise<ParseSophiaFileResult> {
  const content = await readFile(filePath, "utf8");
  return parseSophiaSource(content, filePath);
}

export function parseSophiaSource(content: string, filePath = "<source>"): ParseSophiaFileResult {
  const diagnostics: Diagnostic[] = [];
  const topLevel = parseSophiaTopLevelDeclarations(content);

  if (!hasBalancedSophiaBraces(content)) {
    diagnostics.push(errorDiagnostic("PARSE-SYNTAX-001", filePath, "Unbalanced braces in Sophia source."));
  }

  if (topLevel.length === 0) {
    diagnostics.push(errorDiagnostic("PARSE-FILE-001", filePath, "No top-level Sophia node found."));
  }

  if (topLevel.length > 1) {
    diagnostics.push(
      errorDiagnostic("PARSE-FILE-002", filePath, `Expected one top-level Sophia node, found ${topLevel.length}.`),
    );
  }

  const declaration = topLevel[0] ?? null;
  if (declaration && !SUPPORTED_TOP_LEVEL_KINDS.has(declaration.kind)) {
    diagnostics.push(
      errorDiagnostic(
        "PARSE-FILE-003",
        filePath,
        `Unsupported top-level Sophia v0 node kind: ${declaration.kind}.`,
      ),
    );
  }

  const blocks = declaration
    ? declaration.kind === "error"
      ? parseSophiaErrorVariantBlocks(declaration.body)
      : declaration.kind === "state"
        ? parseSophiaStateValueBlocks(declaration.body)
        : parseSophiaImmediateNamedBlocks(declaration.body)
    : [];
  const diagnosticBlocks = declaration ? parseSophiaImmediateNamedBlocks(declaration.body) : [];
  const blockNames = new Set<string>();
  if (declaration && SUPPORTED_TOP_LEVEL_KINDS.has(declaration.kind)) {
    const supportedBlocks = SUPPORTED_BLOCKS_BY_KIND[declaration.kind];
    const variantNames =
      declaration.kind === "error" ? new Set(blocks.map((block) => block.name)) : null;
    const stateValueNames =
      declaration.kind === "state" ? new Set(blocks.map((block) => block.name)) : null;
    for (const block of diagnosticBlocks) {
      if (supportedBlocks !== null && !supportedBlocks?.has(block.name)) {
        diagnostics.push(
          errorDiagnostic(
            "PARSE-BLOCK-001",
            filePath,
            `Unsupported ${declaration.kind} block in Sophia v0: ${block.name}.`,
          ),
        );
      }
      if (declaration.kind === "error" && !variantNames?.has(block.name)) {
        diagnostics.push(
          errorDiagnostic(
            "PARSE-BLOCK-001",
            filePath,
            `Unsupported error block in Sophia v0: ${block.name}. Use variant ${block.name} { ... }.`,
          ),
        );
      }
      if (declaration.kind === "state" && !stateValueNames?.has(block.name)) {
        diagnostics.push(
          errorDiagnostic(
            "PARSE-BLOCK-001",
            filePath,
            `Unsupported state block in Sophia v0: ${block.name}. Use value ${block.name} { ... }.`,
          ),
        );
      }
      if (blockNames.has(block.name)) {
        diagnostics.push(
          errorDiagnostic(
            "PARSE-BLOCK-002",
            filePath,
            `Duplicate immediate block in ${declaration.kind}: ${block.name}.`,
          ),
        );
      }
      blockNames.add(block.name);
    }
  }

  const ast = declaration
    ? {
        kind: declaration.kind,
        name: declaration.name,
        blocks,
        attributes: parseSophiaImmediateAttributes(declaration.body),
      }
    : null;

  return {
    ok: diagnostics.every((diagnostic) => diagnostic.severity !== "error"),
    path: filePath,
    ast,
    diagnostics,
  };
}

export function parseSophiaTopLevelDeclarations(content: string): SophiaTopLevelDeclaration[] {
  const declarations: SophiaTopLevelDeclaration[] = [];
  let depth = 0;
  const source = stripQuotedText(content);
  for (const match of source.matchAll(/\b([A-Za-z_]\w*)\s+([A-Za-z_]\w*)\s*\{/g)) {
    const before = source.slice(0, match.index);
    depth = braceDepth(before);
    if (depth !== 0) continue;
    const kind = match[1];
    const name = match[2];
    if (!kind || !name) continue;
    const body = readBraceBody(content, match.index + match[0].length);
    declarations.push({ kind, name, body: body ?? "" });
  }
  return declarations;
}

export function parseSophiaImmediateNamedBlocks(content: string): SophiaNamedBlock[] {
  const blocks: SophiaNamedBlock[] = [];
  const source = stripQuotedText(content);
  for (const match of source.matchAll(/\b([A-Za-z_]\w*)\s*\{/g)) {
    if (braceDepth(source.slice(0, match.index)) !== 0) continue;
    const name = match[1];
    if (!name) continue;
    const body = readBraceBody(content, match.index + match[0].length);
    if (body !== null) {
      blocks.push({ name, body: body.trim() });
    }
  }
  return blocks.sort((left, right) => left.name.localeCompare(right.name));
}

export function parseSophiaErrorVariantBlocks(content: string): SophiaNamedBlock[] {
  return parseSophiaPrefixedNamedBlocks(content, "variant");
}

export function parseSophiaStateValueBlocks(content: string): SophiaNamedBlock[] {
  return parseSophiaPrefixedNamedBlocks(content, "value");
}

function parseSophiaPrefixedNamedBlocks(
  content: string,
  prefix: "variant" | "value",
): SophiaNamedBlock[] {
  const blocks: SophiaNamedBlock[] = [];
  const source = stripQuotedText(content);
  const pattern = new RegExp(`\\b${prefix}\\s+([A-Z][A-Za-z0-9]*)\\s*\\{`, "g");
  for (const match of source.matchAll(pattern)) {
    if (braceDepth(source.slice(0, match.index)) !== 0) continue;
    const name = match[1];
    if (!name) continue;
    const body = readBraceBody(content, match.index + match[0].length);
    if (body !== null) {
      blocks.push({ name, body: body.trim() });
    }
  }
  return blocks.sort((left, right) => left.name.localeCompare(right.name));
}

export function parseSophiaImmediateAttributes(content: string): SophiaAttribute[] {
  const attributes: SophiaAttribute[] = [];
  let depth = 0;
  for (const rawLine of content.split("\n")) {
    const line = rawLine.trim();
    if (depth === 0 && line && !line.includes("{") && !line.includes("}")) {
      const match = /^([A-Za-z_]\w*)\s*:\s*(.+)$/.exec(line);
      if (match?.[1] && match[2]) {
        attributes.push({ name: match[1], value: match[2].trim() });
      }
    }
    for (const char of stripQuotedText(rawLine)) {
      if (char === "{") depth += 1;
      if (char === "}") depth = Math.max(0, depth - 1);
    }
  }
  return attributes.sort((left, right) => left.name.localeCompare(right.name));
}

export function hasBalancedSophiaBraces(content: string): boolean {
  let depth = 0;
  for (const char of stripQuotedText(content)) {
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
    if (depth < 0) return false;
  }
  return depth === 0;
}
