import { flattenSophiaBodyStatements, type SophiaBodyStatement } from "./ast.js";
import type { ParsedAction } from "../ast/check_model.js";
import type { Diagnostic } from "../ast/diagnostics.js";
import { errorDiagnostic } from "../ast/diagnostics.js";
import { inferSophiaExpressionType, type SophiaActionSignature } from "../ast/expression.js";
import {
  isSophiaTypeAssignable,
  parseSophiaIntentType,
  parseSophiaOptionalType,
  unwrapSophiaIntentType,
  type SophiaEntityTypes,
  type SophiaStateTypes,
} from "../ast/types.js";

export function checkReturnShape(
  filePath: string,
  action: ParsedAction,
  statements: SophiaBodyStatement[],
  entityTypes: SophiaEntityTypes,
  stateTypes: SophiaStateTypes,
  actionTypes: Map<string, SophiaActionSignature>,
): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  if (!action.outputType) return diagnostics;
  const typeEnvironments = buildBodyStatementTypeEnvironments(
    action,
    statements,
    entityTypes,
    stateTypes,
    actionTypes,
  );
  const returns = flattenSophiaBodyStatements(statements)
    .filter((statement) => statement.kind === "return")
    .map((statement) => ({ statement, expression: statement.expression, line: statement.line }));
  if (returns.length === 0) {
    diagnostics.push(
      errorDiagnostic(
        "CHECK-RETURN-001",
        filePath,
        `Action output declares ${action.outputType}, but body has no return statement.`,
      ),
    );
    return diagnostics;
  }
  for (const item of returns) {
    if (
      !returnExpressionMatchesType(
        action,
        item.expression,
        action.outputType,
        typeEnvironments.get(item.statement) ?? new Map(),
        entityTypes,
        stateTypes,
        actionTypes,
      )
    ) {
      diagnostics.push(
        errorDiagnostic(
          "CHECK-RETURN-001",
          `${filePath}:${item.line}`,
          `Return expression does not match declared output type ${action.outputType}: ${item.expression}`,
        ),
      );
    }
  }
  if (!statementListCompletes(statements)) {
    diagnostics.push(
      errorDiagnostic(
        "CHECK-RETURN-002",
        filePath,
        `Action output declares ${action.outputType}, but not every control-flow path returns or raises.`,
        "End the body with return/raise, or add else branches so every if path terminates.",
      ),
    );
  }
  return diagnostics;
}

export function buildBodyStatementTypeEnvironments(
  action: ParsedAction,
  statements: SophiaBodyStatement[],
  entityTypes: SophiaEntityTypes,
  stateTypes: SophiaStateTypes,
  actionTypes: Map<string, SophiaActionSignature>,
): Map<SophiaBodyStatement, Map<string, string>> {
  const environments = new Map<SophiaBodyStatement, Map<string, string>>();
  collectStatementEnvironments(
    statements,
    new Map(action.inputFields.map((field) => [field.name, field.type])),
    environments,
    entityTypes,
    stateTypes,
    actionTypes,
  );
  return environments;
}

function collectStatementEnvironments(
  statements: SophiaBodyStatement[],
  types: Map<string, string>,
  environments: Map<SophiaBodyStatement, Map<string, string>>,
  entityTypes: SophiaEntityTypes,
  stateTypes: SophiaStateTypes,
  actionTypes: Map<string, SophiaActionSignature>,
): void {
  for (const statement of statements) {
    environments.set(statement, new Map(types));
    if (statement.kind === "let") {
      const inferredType = inferSophiaExpressionType(
        statement.expression,
        types,
        entityTypes,
        actionTypes,
        stateTypes,
      );
      if (inferredType) types.set(statement.name, inferredType);
      continue;
    }
    if (statement.kind === "if") {
      collectStatementEnvironments(
        statement.thenBody,
        new Map(types),
        environments,
        entityTypes,
        stateTypes,
        actionTypes,
      );
      collectStatementEnvironments(
        statement.elseBody,
        new Map(types),
        environments,
        entityTypes,
        stateTypes,
        actionTypes,
      );
      continue;
    }
    if (statement.kind === "match") {
      const matchedType = inferSophiaExpressionType(
        statement.expression,
        types,
        entityTypes,
        actionTypes,
        stateTypes,
      );
      const optionalType = matchedType ? parseSophiaOptionalType(matchedType) : null;
      for (const matchCase of statement.cases) {
        const branchTypes = new Map(types);
        if (matchCase.pattern === "Some" && matchCase.binding && optionalType) {
          branchTypes.set(matchCase.binding, optionalType.innerType);
        }
        collectStatementEnvironments(
          matchCase.body,
          branchTypes,
          environments,
          entityTypes,
          stateTypes,
          actionTypes,
        );
      }
      continue;
    }
    if (statement.kind === "repeat") {
      collectStatementEnvironments(
        statement.body,
        new Map(types),
        environments,
        entityTypes,
        stateTypes,
        actionTypes,
      );
    }
  }
}

function statementListCompletes(statements: SophiaBodyStatement[]): boolean {
  for (const statement of statements) {
    if (statement.kind === "return" || statement.kind === "raise") return true;
    if (
      statement.kind === "if" &&
      statement.elseBody.length > 0 &&
      statementListCompletes(statement.thenBody) &&
      statementListCompletes(statement.elseBody)
    ) {
      return true;
    }
    if (
      statement.kind === "match" &&
      statement.cases.length > 0 &&
      statement.cases.every((matchCase) => statementListCompletes(matchCase.body))
    ) {
      return true;
    }
  }
  return false;
}

function returnExpressionMatchesType(
  action: ParsedAction,
  expression: string,
  typeName: string,
  typeEnvironment: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  stateTypes: SophiaStateTypes,
  actionTypes: Map<string, SophiaActionSignature>,
): boolean {
  const inferred = inferSophiaExpressionType(
    expression,
    typeEnvironment,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  if (
    action.intentConversion &&
    isExplicitIntentConversionReturn(expression, typeName, inferred, typeEnvironment)
  ) {
    return true;
  }
  if (inferred) return isSophiaTypeAssignable(inferred, typeName);
  if (typeName === "Unit") return expression === "unit";
  if (typeName === "Bool") return inferred === "Bool";
  if (typeName === "Int")
    return /^(?:-?\d+|[a-z_]\w*(?:\s*[-+*/]\s*(?:-?\d+|[a-z_]\w*))*)$/.test(expression);
  if (typeName === "Text") return /^(?:"[^"]*"|'[^']*'|[a-z_]\w*)$/.test(expression);
  if (typeName === "List<Int>")
    return /^(?:[a-z_]\w*|\[\s*(?:-?\d+\s*(?:,\s*-?\d+\s*)*)?\])$/.test(expression);
  if (typeName === "List<Text>")
    return /^(?:[a-z_]\w*|\[\s*(?:(?:"[^"]*"|'[^']*')\s*(?:,\s*(?:"[^"]*"|'[^']*')\s*)*)?\])$/.test(
      expression,
    );
  if (parseSophiaIntentType(typeName)) return false;
  return false;
}

function isExplicitIntentConversionReturn(
  expression: string,
  typeName: string,
  inferred: string | null,
  typeEnvironment: Map<string, string>,
): boolean {
  if (!parseSophiaIntentType(typeName) || !inferred || !parseSophiaIntentType(inferred)) {
    return false;
  }
  if (!typeEnvironment.has(expression.trim())) return false;
  return unwrapSophiaIntentType(inferred) === unwrapSophiaIntentType(typeName);
}
