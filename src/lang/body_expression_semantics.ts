import type { Diagnostic } from "./diagnostics.js";
import {
  collectSophiaExpressionIdentifiers,
  inferSophiaExpressionType,
  parseEntityAssignments,
  type SophiaActionSignature,
} from "./expression.js";
import { error } from "./diagnostics.js";
import type { ParsedAction } from "./check_model.js";
import {
  isSophiaTypeAssignable,
  type SophiaEntityTypes,
  type SophiaField,
  type SophiaStateTypes,
} from "./types.js";

export function checkActionCallExpression(
  filePath: string,
  line: number,
  expression: string,
  caller: ParsedAction,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  actionTypes: Map<string, SophiaActionSignature>,
  stateTypes: SophiaStateTypes,
): Diagnostic[] {
  const match = /^([A-Z][A-Za-z0-9]*)\s*\{\s*(.*)\s*\}$/.exec(expression.trim());
  if (!match?.[1] || match[2] === undefined || entityTypes.has(match[1])) return [];
  const actionName = match[1];
  const signature = actionTypes.get(actionName);
  if (!signature) {
    return [
      error(
        "CHECK-ACTION-CALL-001",
        `${filePath}:${line}`,
        `Action call references unknown action: ${actionName}.`,
      ),
    ];
  }
  if (actionName === caller.name) {
    return [
      error(
        "CHECK-ACTION-CALL-005",
        `${filePath}:${line}`,
        `Action ${caller.name} cannot recursively call itself in v0.`,
      ),
    ];
  }

  const assignments = parseEntityAssignments(match[2]);
  if (!assignments) {
    return [
      error(
        "CHECK-ACTION-CALL-002",
        `${filePath}:${line}`,
        `Action call ${actionName} must use input = expression assignments.`,
      ),
    ];
  }

  const diagnostics: Diagnostic[] = [];
  const expectedNames = new Set(signature.input.map((field) => field.name));
  for (const actualName of assignments.keys()) {
    if (!expectedNames.has(actualName)) {
      diagnostics.push(
        error(
          "CHECK-ACTION-CALL-003",
          `${filePath}:${line}`,
          `Action call ${actionName} uses unknown input ${actualName}.`,
        ),
      );
    }
  }
  for (const field of signature.input) {
    const actualExpression = assignments.get(field.name);
    if (actualExpression === undefined) {
      diagnostics.push(
        error(
          "CHECK-ACTION-CALL-004",
          `${filePath}:${line}`,
          `Action call ${actionName} is missing input ${field.name}.`,
        ),
      );
      continue;
    }
    const actualType = inferSophiaExpressionType(
      actualExpression,
      types,
      entityTypes,
      actionTypes,
      stateTypes,
    );
    if (actualType && !isSophiaTypeAssignable(actualType, field.type)) {
      diagnostics.push(
        error(
          "CHECK-TYPE-002",
          `${filePath}:${line}`,
          `Action call ${actionName}.${field.name} expects ${field.type}, got ${actualType}.`,
        ),
      );
    }
  }
  for (const effect of signature.effects) {
    if (effect === "Pure") continue;
    if (!caller.effects.has(effect)) {
      diagnostics.push(
        error(
          "CHECK-ACTION-CALL-006",
          `${filePath}:${line}`,
          `Action ${caller.name} calls ${actionName}, but does not declare called effect ${effect}.`,
          "Add the called action effect to the caller effects block and capability allow policy, unless it is denied.",
        ),
      );
    }
  }
  for (const variantName of signature.errors) {
    if (!caller.errors.has(variantName)) {
      diagnostics.push(
        error(
          "CHECK-ACTION-CALL-008",
          `${filePath}:${line}`,
          `Action ${caller.name} calls ${actionName}, but does not declare called error ${variantName}.`,
          "Add the called action error variant to the caller errors block until match/handle support exists.",
        ),
      );
    }
  }
  return diagnostics;
}

export function checkEntityConstructionExpression(
  filePath: string,
  line: number,
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  stateTypes: SophiaStateTypes,
): Diagnostic[] {
  const match = /^([A-Z][A-Za-z0-9]*)\s*\{\s*(.*)\s*\}$/.exec(expression.trim());
  if (!match?.[1] || match[2] === undefined || !entityTypes.has(match[1])) return [];
  const entityName = match[1];
  const expectedFields = entityTypes.get(entityName) ?? [];
  const assignments = parseEntityAssignments(match[2]);
  if (!assignments) {
    return [
      error(
        "CHECK-ENTITY-003",
        `${filePath}:${line}`,
        `Entity construction for ${entityName} must use field = expression assignments.`,
      ),
    ];
  }
  const diagnostics: Diagnostic[] = [];
  const expectedNames = new Set(expectedFields.map((field) => field.name));
  for (const actualName of assignments.keys()) {
    if (!expectedNames.has(actualName)) {
      diagnostics.push(
        error(
          "CHECK-ENTITY-004",
          `${filePath}:${line}`,
          `Entity construction for ${entityName} uses unknown field ${actualName}.`,
        ),
      );
    }
  }
  for (const field of expectedFields) {
    const actualExpression = assignments.get(field.name);
    if (actualExpression === undefined) {
      diagnostics.push(
        error(
          "CHECK-ENTITY-005",
          `${filePath}:${line}`,
          `Entity construction for ${entityName} is missing field ${field.name}.`,
        ),
      );
      continue;
    }
    const actualType = inferSophiaExpressionType(
      actualExpression,
      types,
      entityTypes,
      new Map(),
      stateTypes,
    );
    if (actualType && !isSophiaTypeAssignable(actualType, field.type)) {
      diagnostics.push(
        error(
          "CHECK-TYPE-002",
          `${filePath}:${line}`,
          `Entity field ${entityName}.${field.name} expects ${field.type}, got ${actualType}.`,
        ),
      );
    }
  }
  return diagnostics;
}

export function checkRaiseStatement(
  filePath: string,
  line: number,
  variantName: string,
  expression: string,
  action: ParsedAction,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  stateTypes: SophiaStateTypes,
  errorVariants: Map<string, { errorName: string; name: string; fields: SophiaField[] }>,
): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  const variant = errorVariants.get(variantName);
  if (!variant) {
    diagnostics.push(
      error(
        "CHECK-ERROR-004",
        `${filePath}:${line}`,
        `Raise references unknown error variant: ${variantName}.`,
      ),
    );
    return diagnostics;
  }
  if (!action.errors.has(variantName)) {
    diagnostics.push(
      error(
        "CHECK-ERROR-005",
        `${filePath}:${line}`,
        `Action ${action.name} raises ${variantName}, but does not declare it in errors.`,
        `Add ${variantName} to the action errors block or remove the raise path.`,
      ),
    );
  }

  const match = new RegExp(`^${variantName}\\s*\\{\\s*(.*)\\s*\\}$`).exec(expression.trim());
  const assignments = match?.[1] !== undefined ? parseEntityAssignments(match[1]) : null;
  if (!assignments) {
    diagnostics.push(
      error(
        "CHECK-ERROR-006",
        `${filePath}:${line}`,
        `Raise ${variantName} must use field = expression assignments.`,
      ),
    );
    return diagnostics;
  }

  const expectedNames = new Set(variant.fields.map((field) => field.name));
  for (const actualName of assignments.keys()) {
    if (!expectedNames.has(actualName)) {
      diagnostics.push(
        error(
          "CHECK-ERROR-007",
          `${filePath}:${line}`,
          `Raise ${variantName} uses unknown field ${actualName}.`,
        ),
      );
    }
  }
  for (const field of variant.fields) {
    const actualExpression = assignments.get(field.name);
    if (actualExpression === undefined) {
      diagnostics.push(
        error(
          "CHECK-ERROR-008",
          `${filePath}:${line}`,
          `Raise ${variantName} is missing field ${field.name}.`,
        ),
      );
      continue;
    }
    const actualType = inferSophiaExpressionType(
      actualExpression,
      types,
      entityTypes,
      new Map(),
      stateTypes,
    );
    if (actualType && !isSophiaTypeAssignable(actualType, field.type)) {
      diagnostics.push(
        error(
          "CHECK-TYPE-002",
          `${filePath}:${line}`,
          `Error field ${variantName}.${field.name} expects ${field.type}, got ${actualType}.`,
        ),
      );
    }
  }
  return diagnostics;
}

export function checkExpressionIdentifiers(
  filePath: string,
  line: number,
  expression: string,
  declared: Set<string>,
): Diagnostic[] {
  if (/^\s*call\s+[A-Z][A-Za-z0-9]*\s*\{/.test(expression)) {
    return [
      error(
        "CHECK-SYNTAX-009",
        `${filePath}:${line}`,
        "Sophia v0 action expressions do not use a call keyword.",
        "Write ActionName { input = value } instead of call ActionName { input = value }.",
      ),
    ];
  }
  if (/^\s*empty\s+List\s*<\s*(?:Int|Text)\s*>\s*$/.test(expression)) {
    return [
      error(
        "CHECK-SYNTAX-012",
        `${filePath}:${line}`,
        "Sophia v0 empty list expressions use [] rather than empty List<T>.",
        "Write [] and let the declared output, local use, or later list updates determine the list type.",
      ),
    ];
  }
  if (expression.trim() === "Unit") {
    return [
      error(
        "CHECK-SYNTAX-014",
        `${filePath}:${line}`,
        "Unit is a type name, not the Unit value.",
        "Use lowercase unit as the Unit value: return unit.",
      ),
    ];
  }
  if (/\b[A-Z][A-Za-z0-9]*\.[A-Za-z_]\w*\s*\(/.test(expression)) {
    return [
      error(
        "CHECK-SYNTAX-016",
        `${filePath}:${line}`,
        `Unsupported conversion or static helper expression: ${expression}.`,
        "Use supported Sophia expressions directly. For printing numbers, use print number.",
      ),
    ];
  }
  const diagnostics: Diagnostic[] = [];
  const expressionsToCheck = entityConstructorAssignmentExpressions(expression) ?? [expression];
  for (const expressionToCheck of expressionsToCheck) {
    for (const identifier of collectSophiaExpressionIdentifiers(expressionToCheck)) {
      if (!declared.has(identifier)) {
        diagnostics.push(
          error(
            "CHECK-VAR-001",
            `${filePath}:${line}`,
            `Identifier is not declared: ${identifier}.`,
          ),
        );
      }
    }
  }
  return diagnostics;
}

function entityConstructorAssignmentExpressions(expression: string): string[] | null {
  const match = /^[A-Z][A-Za-z0-9]*\s*\{\s*(.*)\s*\}$/.exec(expression.trim());
  if (!match || match[1] === undefined) return null;
  const assignments = parseEntityAssignments(match[1]);
  return assignments ? [...assignments.values()] : null;
}
