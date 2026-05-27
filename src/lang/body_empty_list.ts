import type { SophiaBodyStatement } from "./body_ast.js";
import { inferSophiaExpressionType, type SophiaActionSignature } from "./expression.js";
import type { SophiaEntityTypes, SophiaStateTypes } from "./types.js";

export interface InferEmptyListTypeOptions {
  name: string;
  statements: SophiaBodyStatement[];
  types: Map<string, string>;
  outputType: string;
  entityTypes: SophiaEntityTypes;
  stateTypes: SophiaStateTypes;
  actionTypes: Map<string, SophiaActionSignature>;
}

export function inferEmptyListTypeForVariable(options: InferEmptyListTypeOptions): string | null {
  return inferFromStatements({ ...options, types: new Map(options.types) });
}

function inferFromStatements(options: InferEmptyListTypeOptions): string | null {
  const { name, statements, types, outputType, entityTypes, stateTypes, actionTypes } = options;
  for (const statement of statements) {
    if (
      statement.kind === "return" &&
      statement.expression === name &&
      outputType.startsWith("List<")
    ) {
      return outputType;
    }
    if (statement.kind === "let") {
      if (statement.name !== name) {
        const inferredType = inferSophiaExpressionType(
          statement.expression,
          types,
          entityTypes,
          actionTypes,
          stateTypes,
        );
        if (inferredType) types.set(statement.name, inferredType);
      }
      continue;
    }
    if (statement.kind === "repeat") {
      const inferredType = inferFromStatements({
        ...options,
        statements: statement.body,
        types: new Map(types),
      });
      if (inferredType) return inferredType;
      continue;
    }
    if (statement.kind === "if") {
      const thenType = inferFromStatements({
        ...options,
        statements: statement.thenBody,
        types: new Map(types),
      });
      if (thenType) return thenType;
      const elseType = inferFromStatements({
        ...options,
        statements: statement.elseBody,
        types: new Map(types),
      });
      if (elseType) return elseType;
      continue;
    }
    if (statement.kind === "match") {
      for (const matchCase of statement.cases) {
        const inferredType = inferFromStatements({
          ...options,
          statements: matchCase.body,
          types: new Map(types),
        });
        if (inferredType) return inferredType;
      }
      continue;
    }
    if (statement.kind === "set" && statement.name === name) {
      const appendMatch = new RegExp(`^${name}\\.append\\((.+)\\)$`).exec(statement.expression);
      if (appendMatch?.[1]) {
        const itemType = inferSophiaExpressionType(
          appendMatch[1],
          types,
          entityTypes,
          actionTypes,
          stateTypes,
        );
        if (itemType === "Int") return "List<Int>";
        if (itemType === "Text") return "List<Text>";
      }
      const concatMatch = new RegExp(`^${name}\\s*\\+\\s*(\\[.+\\])$`).exec(statement.expression);
      if (concatMatch?.[1]) {
        return inferSophiaExpressionType(
          concatMatch[1],
          types,
          entityTypes,
          actionTypes,
          stateTypes,
        );
      }
      continue;
    }
    if (statement.kind === "set" && statement.name !== name) {
      const inferredType = inferSophiaExpressionType(
        statement.expression,
        types,
        entityTypes,
        actionTypes,
        stateTypes,
      );
      if (inferredType) types.set(statement.name, inferredType);
    }
  }
  return null;
}
