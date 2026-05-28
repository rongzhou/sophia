import type { SophiaRawAst } from "./parser.js";

export const SOPHIA_SEMANTIC_ASSIST_ATTRIBUTES = [
  "meaning",
  "purpose",
  "not",
  "because",
  "examples",
  "anti_patterns",
  "plan",
  "repair_notes",
] as const;

const SEMANTIC_ASSIST_ATTRIBUTE_SET = new Set<string>(SOPHIA_SEMANTIC_ASSIST_ATTRIBUTES);

export function stripSemanticAssistFromAst(ast: SophiaRawAst): SophiaRawAst {
  return {
    ...ast,
    attributes: ast.attributes.filter(
      (attribute) => !SEMANTIC_ASSIST_ATTRIBUTE_SET.has(attribute.name),
    ),
  };
}

export function stripSemanticAssistFromFiles<T extends { path: string; ast: SophiaRawAst }>(
  files: T[],
): T[] {
  return files.map((file) => ({
    ...file,
    ast: stripSemanticAssistFromAst(file.ast),
  }));
}
