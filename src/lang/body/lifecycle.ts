import type { SophiaBodyStatement } from "./ast.js";
import type { ParsedAction } from "../ast/check_model.js";
import type { Diagnostic } from "../ast/diagnostics.js";
import { errorDiagnostic } from "../ast/diagnostics.js";
import {
  checkActionCallExpression,
  checkEntityConstructionExpression,
  checkExpressionIdentifiers,
  checkRaiseStatement,
} from "./expression_semantics.js";
import { inferSophiaExpressionType, type SophiaActionSignature } from "../ast/expression.js";
import {
  isSophiaTypeAssignable,
  parseSophiaOptionalType,
  type SophiaEntityTypes,
  type SophiaField,
} from "../ast/types.js";

interface LifecycleCheckContext {
  diagnostics: Diagnostic[];
  filePath: string;
  scope: Scope;
  action: ParsedAction;
  entityTypes: SophiaEntityTypes;
  stateTypes: Map<string, string[]>;
  errorVariants: Map<string, { errorName: string; name: string; fields: SophiaField[] }>;
  actionTypes: Map<string, SophiaActionSignature>;
}

interface Scope {
  parent: Scope | null;
  declared: Set<string>;
  types: Map<string, string>;
  mutable: Set<string>;
}

export function checkVariableLifecycle(
  filePath: string,
  action: ParsedAction,
  statements: SophiaBodyStatement[],
  entityTypes: SophiaEntityTypes,
  stateTypes: Map<string, string[]>,
  errorVariants: Map<string, { errorName: string; name: string; fields: SophiaField[] }>,
  actionTypes: Map<string, SophiaActionSignature>,
): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  const rootScope: Scope = {
    parent: null,
    declared: new Set(action.inputFields.map((field) => field.name)),
    types: new Map(action.inputFields.map((field) => [field.name, field.type])),
    mutable: new Set(),
  };
  checkStatements(statements, rootScope);

  return diagnostics;

  function checkStatements(items: SophiaBodyStatement[], scope: Scope): void {
    for (const statement of items) {
      if (statement.kind === "let") {
        checkLetStatement({
          diagnostics,
          filePath,
          statement,
          scope,
          action,
          entityTypes,
          stateTypes,
          errorVariants,
          actionTypes,
        });
        continue;
      }

      if (statement.kind === "set") {
        checkSetStatement({
          diagnostics,
          filePath,
          statement,
          scope,
          action,
          entityTypes,
          stateTypes,
          errorVariants,
          actionTypes,
        });
        continue;
      }

      if (statement.kind === "print") {
        diagnostics.push(
          ...checkExpressionIdentifiers(
            filePath,
            statement.line,
            statement.expression,
            visibleDeclared(scope),
          ),
        );
        diagnostics.push(
          ...checkActionCallExpression(
            filePath,
            statement.line,
            statement.expression,
            action,
            visibleTypes(scope),
            entityTypes,
            actionTypes,
            stateTypes,
          ),
        );
        continue;
      }

      if (statement.kind === "return" && statement.expression !== "unit") {
        diagnostics.push(
          ...checkExpressionIdentifiers(
            filePath,
            statement.line,
            statement.expression,
            visibleDeclared(scope),
          ),
        );
        diagnostics.push(
          ...checkEntityConstructionExpression(
            filePath,
            statement.line,
            statement.expression,
            visibleTypes(scope),
            entityTypes,
            stateTypes,
          ),
        );
        diagnostics.push(
          ...checkActionCallExpression(
            filePath,
            statement.line,
            statement.expression,
            action,
            visibleTypes(scope),
            entityTypes,
            actionTypes,
            stateTypes,
          ),
        );
        continue;
      }

      if (statement.kind === "raise") {
        diagnostics.push(
          ...checkExpressionIdentifiers(
            filePath,
            statement.line,
            statement.expression,
            visibleDeclared(scope),
          ),
        );
        diagnostics.push(
          ...checkRaiseStatement(
            filePath,
            statement.line,
            statement.variant,
            statement.expression,
            action,
            visibleTypes(scope),
            entityTypes,
            stateTypes,
            errorVariants,
          ),
        );
        continue;
      }

      if (statement.kind === "if") {
        checkIfStatement({
          diagnostics,
          filePath,
          statement,
          scope,
          action,
          entityTypes,
          stateTypes,
          errorVariants,
          actionTypes,
        });
        checkStatements(statement.thenBody, childScope(scope));
        checkStatements(statement.elseBody, childScope(scope));
        continue;
      }

      if (statement.kind === "match") {
        checkMatchStatement({
          diagnostics,
          filePath,
          statement,
          scope,
          action,
          entityTypes,
          stateTypes,
          errorVariants,
          actionTypes,
        });
        for (const matchCase of statement.cases) {
          const branchScope = childScope(scope);
          const matchedType = inferSophiaExpressionType(
            statement.expression,
            visibleTypes(scope),
            entityTypes,
            actionTypes,
            stateTypes,
          );
          const optionalType = matchedType ? parseSophiaOptionalType(matchedType) : null;
          if (matchCase.pattern === "Some" && matchCase.binding && optionalType) {
            branchScope.declared.add(matchCase.binding);
            branchScope.types.set(matchCase.binding, optionalType.innerType);
          }
          checkStatements(matchCase.body, branchScope);
        }
        continue;
      }

      if (statement.kind === "repeat") {
        checkStatements(statement.body, childScope(scope));
      }
    }
  }
}

function checkLetStatement(
  options: LifecycleCheckContext & {
    statement: Extract<SophiaBodyStatement, { kind: "let" }>;
  },
): void {
  const name = options.statement.name;
  if (lookupDeclared(options.scope, name)) {
    options.diagnostics.push(
      errorDiagnostic(
        "CHECK-VAR-002",
        `${options.filePath}:${options.statement.line}`,
        `Variable shadows or redeclares a visible name: ${name}.`,
      ),
    );
  }
  checkExpression(options, options.statement.expression);
  options.scope.declared.add(name);
  const inferredType = inferSophiaExpressionType(
    options.statement.expression,
    visibleTypes(options.scope),
    options.entityTypes,
    options.actionTypes,
    options.stateTypes,
  );
  if (inferredType) {
    options.scope.types.set(name, inferredType);
  }
  if (options.statement.mutable) options.scope.mutable.add(name);
}

function checkSetStatement(
  options: LifecycleCheckContext & {
    statement: Extract<SophiaBodyStatement, { kind: "set" }>;
  },
): void {
  const name = options.statement.name;
  const targetScope = lookupScope(options.scope, name);
  if (!targetScope) {
    options.diagnostics.push(
      errorDiagnostic(
        "CHECK-VAR-001",
        `${options.filePath}:${options.statement.line}`,
        `Assignment target is not declared: ${name}.`,
      ),
    );
  } else if (!targetScope.mutable.has(name)) {
    options.diagnostics.push(
      errorDiagnostic(
        "CHECK-VAR-003",
        `${options.filePath}:${options.statement.line}`,
        `Assignment target is not mutable: ${name}.`,
        "Declare it with let mutable before using set.",
      ),
    );
  }
  checkExpression(options, options.statement.expression);
  const targetType = targetScope?.types.get(name);
  const expressionType = inferSophiaExpressionType(
    options.statement.expression,
    visibleTypes(options.scope),
    options.entityTypes,
    options.actionTypes,
    options.stateTypes,
  );
  if (targetType && expressionType && !isSophiaTypeAssignable(expressionType, targetType)) {
    options.diagnostics.push(
      errorDiagnostic(
        "CHECK-TYPE-002",
        `${options.filePath}:${options.statement.line}`,
        `Assignment type mismatch for ${name}: expected ${targetType}, got ${expressionType}.`,
      ),
    );
  }
}

function checkIfStatement(
  options: LifecycleCheckContext & {
    statement: Extract<SophiaBodyStatement, { kind: "if" }>;
  },
): void {
  if (/\b[a-z_]\w*\s*(?:==|!=)\s*\[\s*\]/.test(options.statement.condition)) {
    options.diagnostics.push(
      errorDiagnostic(
        "CHECK-BODY-005",
        `${options.filePath}:${options.statement.line}`,
        "List equality is not a valid v0 emptiness check.",
        "Track appended item count with an Int variable and compare that counter instead.",
      ),
    );
  }
  options.diagnostics.push(
    ...checkExpressionIdentifiers(
      options.filePath,
      options.statement.line,
      options.statement.condition,
      visibleDeclared(options.scope),
    ),
  );
  options.diagnostics.push(
    ...checkActionCallExpression(
      options.filePath,
      options.statement.line,
      options.statement.condition,
      options.action,
      visibleTypes(options.scope),
      options.entityTypes,
      options.actionTypes,
      options.stateTypes,
    ),
  );
  const conditionType = inferSophiaExpressionType(
    options.statement.condition,
    visibleTypes(options.scope),
    options.entityTypes,
    options.actionTypes,
    options.stateTypes,
  );
  if (conditionType !== "Bool") {
    options.diagnostics.push(
      errorDiagnostic(
        "CHECK-TYPE-003",
        `${options.filePath}:${options.statement.line}`,
        `If condition must be Bool, got ${conditionType ?? "unknown"}: ${options.statement.condition}`,
      ),
    );
  }
}

function checkMatchStatement(
  options: LifecycleCheckContext & {
    statement: Extract<SophiaBodyStatement, { kind: "match" }>;
  },
): void {
  options.diagnostics.push(
    ...checkExpressionIdentifiers(
      options.filePath,
      options.statement.line,
      options.statement.expression,
      visibleDeclared(options.scope),
    ),
  );
  options.diagnostics.push(
    ...checkActionCallExpression(
      options.filePath,
      options.statement.line,
      options.statement.expression,
      options.action,
      visibleTypes(options.scope),
      options.entityTypes,
      options.actionTypes,
      options.stateTypes,
    ),
  );
  const matchedType = inferSophiaExpressionType(
    options.statement.expression,
    visibleTypes(options.scope),
    options.entityTypes,
    options.actionTypes,
    options.stateTypes,
  );
  if (!matchedType) {
    options.diagnostics.push(
      errorDiagnostic(
        "CHECK-MATCH-001",
        `${options.filePath}:${options.statement.line}`,
        `Match expression type is unknown: ${options.statement.expression}`,
      ),
    );
    return;
  }

  const optionalType = parseSophiaOptionalType(matchedType);
  const stateValues = options.stateTypes.get(matchedType);
  const validPatterns =
    matchedType === "Bool"
      ? new Set(["true", "false"])
      : optionalType
        ? new Set(["Some", "None"])
        : stateValues
          ? new Set(stateValues.map((value) => `${matchedType}.${value}`))
          : null;
  if (!validPatterns) {
    options.diagnostics.push(
      errorDiagnostic(
        "CHECK-MATCH-002",
        `${options.filePath}:${options.statement.line}`,
        `Match expression must be Bool, state, or Optional<T>, got ${matchedType}.`,
      ),
    );
    return;
  }

  const seen = new Set<string>();
  for (const matchCase of options.statement.cases) {
    if (!validPatterns.has(matchCase.pattern)) {
      options.diagnostics.push(
        errorDiagnostic(
          "CHECK-MATCH-003",
          `${options.filePath}:${matchCase.line}`,
          `Match case ${formatMatchPattern(matchCase)} is not valid for ${matchedType}.`,
        ),
      );
      continue;
    }
    if (seen.has(matchCase.pattern)) {
      options.diagnostics.push(
        errorDiagnostic(
          "CHECK-MATCH-004",
          `${options.filePath}:${matchCase.line}`,
          `Duplicate match case: ${formatMatchPattern(matchCase)}.`,
        ),
      );
    }
    seen.add(matchCase.pattern);
    if (matchCase.pattern === "Some" && matchCase.binding) {
      if (lookupDeclared(options.scope, matchCase.binding)) {
        options.diagnostics.push(
          errorDiagnostic(
            "CHECK-MATCH-006",
            `${options.filePath}:${matchCase.line}`,
            `Some binding shadows or redeclares a visible name: ${matchCase.binding}.`,
          ),
        );
      }
    }
  }

  const missing = [...validPatterns].filter((pattern) => !seen.has(pattern));
  if (missing.length > 0) {
    options.diagnostics.push(
      errorDiagnostic(
        "CHECK-MATCH-005",
        `${options.filePath}:${options.statement.line}`,
        `Match over ${matchedType} is not exhaustive; missing ${missing.join(", ")}.`,
      ),
    );
  }
}

function formatMatchPattern(matchCase: { pattern: string; binding: string | null }): string {
  return matchCase.pattern === "Some" && matchCase.binding
    ? `Some(${matchCase.binding})`
    : matchCase.pattern;
}

function checkExpression(
  options: LifecycleCheckContext & {
    statement: Extract<SophiaBodyStatement, { kind: "let" | "set" }>;
  },
  expression: string,
): void {
  options.diagnostics.push(
    ...checkExpressionIdentifiers(
      options.filePath,
      options.statement.line,
      expression,
      visibleDeclared(options.scope),
    ),
  );
  options.diagnostics.push(
    ...checkEntityConstructionExpression(
      options.filePath,
      options.statement.line,
      expression,
      visibleTypes(options.scope),
      options.entityTypes,
      options.stateTypes,
    ),
  );
  options.diagnostics.push(
    ...checkActionCallExpression(
      options.filePath,
      options.statement.line,
      expression,
      options.action,
      visibleTypes(options.scope),
      options.entityTypes,
      options.actionTypes,
      options.stateTypes,
    ),
  );
}

function childScope(parent: Scope): Scope {
  return { parent, declared: new Set(), types: new Map(), mutable: new Set() };
}

function lookupScope(scope: Scope, name: string): Scope | null {
  let current: Scope | null = scope;
  while (current) {
    if (current.declared.has(name)) return current;
    current = current.parent;
  }
  return null;
}

function lookupDeclared(scope: Scope, name: string): boolean {
  return lookupScope(scope, name) !== null;
}

function visibleDeclared(scope: Scope): Set<string> {
  const declared = new Set<string>();
  const chain: Scope[] = [];
  let current: Scope | null = scope;
  while (current) {
    chain.unshift(current);
    current = current.parent;
  }
  for (const item of chain) {
    for (const name of item.declared) declared.add(name);
  }
  return declared;
}

function visibleTypes(scope: Scope): Map<string, string> {
  const types = new Map<string, string>();
  const chain: Scope[] = [];
  let current: Scope | null = scope;
  while (current) {
    chain.unshift(current);
    current = current.parent;
  }
  for (const item of chain) {
    for (const [name, type] of item.types) types.set(name, type);
  }
  return types;
}
