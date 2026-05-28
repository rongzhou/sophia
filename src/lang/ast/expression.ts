import {
  isSophiaTypeAssignable,
  parseSophiaIntentType,
  unwrapSophiaIntentType,
  wrapSophiaIntentType,
  type SophiaEntityTypes,
  type SophiaField,
  type SophiaIntentType,
  type SophiaStateTypes,
} from "./types.js";

export function inferSophiaExpressionType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes = new Map(),
  actionTypes: Map<string, SophiaActionSignature> = new Map(),
  stateTypes: SophiaStateTypes = new Map(),
): string | null {
  const trimmed = expression.trim();
  if (trimmed === "unit") return "Unit";
  if (trimmed === "None") return "None";
  if (trimmed === "true" || trimmed === "false") return "Bool";
  if (/^-?\d+$/.test(trimmed)) return "Int";
  if (/^(?:"[^"]*"|'[^']*')$/.test(trimmed)) return "Text";
  const toTextType = inferToTextExpressionType(
    trimmed,
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  if (toTextType) return toTextType;
  const optionalType = inferOptionalExpressionType(
    trimmed,
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  if (optionalType) return optionalType;
  const stateValueMatch = /^([A-Z][A-Za-z0-9]*)\.([A-Z][A-Za-z0-9]*)$/.exec(trimmed);
  if (stateValueMatch?.[1] && stateValueMatch[2]) {
    return stateTypes.get(stateValueMatch[1])?.includes(stateValueMatch[2])
      ? stateValueMatch[1]
      : null;
  }
  const listLiteralType = inferListLiteralType(
    trimmed,
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  if (listLiteralType !== undefined) return listLiteralType;
  const variableType = types.get(trimmed);
  if (variableType) return variableType;
  const fieldAccessType = inferFieldAccessType(trimmed, types, entityTypes);
  if (fieldAccessType) return fieldAccessType;
  const entityConstructionType = inferEntityConstructionType(
    trimmed,
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  if (entityConstructionType) return entityConstructionType;
  const actionCallType = inferActionCallType(trimmed, types, entityTypes, actionTypes, stateTypes);
  if (actionCallType) return actionCallType;
  const booleanType = inferBooleanExpressionType(
    trimmed,
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  if (booleanType) return booleanType;
  const textConcatType = inferTextConcatExpressionType(
    trimmed,
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  if (textConcatType) return textConcatType;
  if (
    /^(?:-?\d+|[a-z_]\w*(?:\.[a-z_]\w*)?)(?:\s*[-+*/]\s*(?:-?\d+|[a-z_]\w*(?:\.[a-z_]\w*)?))*$/.test(
      trimmed,
    )
  ) {
    const identifiers = collectSophiaExpressionIdentifiers(trimmed);
    if (identifiers.every((identifier) => types.has(identifier))) {
      const operandTypes = splitArithmeticOperands(trimmed).map((operand) =>
        /^-?\d+$/.test(operand)
          ? "Int"
          : inferSophiaExpressionType(operand, types, entityTypes, actionTypes, stateTypes),
      );
      const arithmeticType = combineIntentTypes(operandTypes, "Int");
      if (arithmeticType) {
        return arithmeticType;
      }
    }
  }
  const appendMatch = /^([a-z_]\w*)\.append\((.+)\)$/.exec(trimmed);
  if (appendMatch?.[1] && appendMatch[2]) {
    const listType = types.get(appendMatch[1]);
    const itemType = inferSophiaExpressionType(
      appendMatch[2],
      types,
      entityTypes,
      actionTypes,
      stateTypes,
    );
    if (listType === "List<Int>" && itemType === "Int") return "List<Int>";
    if (listType === "List<Text>" && itemType === "Text") return "List<Text>";
    return null;
  }
  const concatMatch = /^([a-z_]\w*)\s*\+\s*(\[.+\])$/.exec(trimmed);
  if (concatMatch?.[1] && concatMatch[2]) {
    const listType = types.get(concatMatch[1]);
    const itemListType = inferSophiaExpressionType(
      concatMatch[2],
      types,
      entityTypes,
      actionTypes,
      stateTypes,
    );
    if (listType && listType === itemListType) return listType;
  }
  return null;
}

export interface SophiaActionSignature {
  name: string;
  input: SophiaField[];
  outputType: string;
  effects: Set<string>;
  errors: Set<string>;
}

function inferToTextExpressionType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  actionTypes: Map<string, SophiaActionSignature>,
  stateTypes: SophiaStateTypes,
): string | null {
  const match = /^to_text\((.+)\)$/.exec(expression);
  if (!match?.[1]) return null;
  const innerType = inferSophiaExpressionType(
    match[1],
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  return unwrapSophiaIntentType(innerType ?? "") === "Int" ? "Text" : null;
}

function inferOptionalExpressionType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  actionTypes: Map<string, SophiaActionSignature>,
  stateTypes: SophiaStateTypes,
): string | null {
  const someMatch = /^Some\((.+)\)$/.exec(expression);
  if (!someMatch?.[1]) return null;
  const innerType = inferSophiaExpressionType(
    someMatch[1],
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  return innerType ? `Optional<${innerType}>` : null;
}

function inferTextConcatExpressionType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  actionTypes: Map<string, SophiaActionSignature>,
  stateTypes: SophiaStateTypes,
): string | null {
  const operands = splitTextConcatOperands(expression);
  if (!operands) return null;
  const operandTypes = operands.map((operand) =>
    inferSophiaExpressionType(operand, types, entityTypes, actionTypes, stateTypes),
  );
  return combineIntentTypes(operandTypes, "Text");
}

function splitTextConcatOperands(expression: string): string[] | null {
  const operands: string[] = [];
  let current = "";
  let quote: '"' | "'" | null = null;
  let escaped = false;

  for (let index = 0; index < expression.length; index += 1) {
    const char = expression[index] ?? "";
    if (quote) {
      current += char;
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === quote) {
        quote = null;
      }
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
      current += char;
      continue;
    }
    if (char === "+") {
      const operand = current.trim();
      if (!operand) return null;
      operands.push(operand);
      current = "";
      continue;
    }
    current += char;
  }

  const finalOperand = current.trim();
  if (!finalOperand) return null;
  operands.push(finalOperand);
  return operands.length > 1 ? operands : null;
}

function inferBooleanExpressionType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  actionTypes: Map<string, SophiaActionSignature>,
  stateTypes: SophiaStateTypes,
): "Bool" | null {
  const notMatch = /^not\s+(.+)$/.exec(expression);
  if (notMatch?.[1]) {
    return inferSophiaExpressionType(notMatch[1], types, entityTypes, actionTypes, stateTypes) ===
      "Bool"
      ? "Bool"
      : null;
  }

  for (const operator of ["and", "or"] as const) {
    const match = new RegExp(`^(.+)\\s+${operator}\\s+(.+)$`).exec(expression);
    if (match?.[1] && match[2]) {
      const leftType = inferSophiaExpressionType(
        match[1],
        types,
        entityTypes,
        actionTypes,
        stateTypes,
      );
      const rightType = inferSophiaExpressionType(
        match[2],
        types,
        entityTypes,
        actionTypes,
        stateTypes,
      );
      return leftType === "Bool" && rightType === "Bool" ? "Bool" : null;
    }
  }

  const comparisonMatch = /^(.+)\s*(==|!=|<=|>=|<|>)\s*(.+)$/.exec(expression);
  if (!comparisonMatch?.[1] || !comparisonMatch[2] || !comparisonMatch[3]) return null;
  const leftType = inferSophiaExpressionType(
    comparisonMatch[1],
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  const rightType = inferSophiaExpressionType(
    comparisonMatch[3],
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  if (
    !leftType ||
    !rightType ||
    unwrapSophiaIntentType(leftType) !== unwrapSophiaIntentType(rightType)
  ) {
    return null;
  }
  if (comparisonMatch[2] === "==" || comparisonMatch[2] === "!=") {
    return ["Bool", "Int", "Text"].includes(unwrapSophiaIntentType(leftType)) ||
      stateTypes.has(unwrapSophiaIntentType(leftType))
      ? "Bool"
      : null;
  }
  return unwrapSophiaIntentType(leftType) === "Int" ? "Bool" : null;
}

function combineIntentTypes(types: Array<string | null>, baseType: "Int" | "Text"): string | null {
  if (types.some((type) => type === null)) return null;
  const concreteTypes = types as string[];
  if (concreteTypes.some((type) => unwrapSophiaIntentType(type) !== baseType)) return null;
  const intentTypes = concreteTypes
    .map((type) => parseSophiaIntentType(type)?.intent ?? null)
    .filter((intent): intent is SophiaIntentType => intent !== null);
  if (intentTypes.length === 0) return baseType;
  if (intentTypes.includes("Raw")) return wrapSophiaIntentType("Raw", baseType);
  const firstIntent = intentTypes[0];
  if (firstIntent && intentTypes.every((intent) => intent === firstIntent)) {
    return wrapSophiaIntentType(firstIntent, baseType);
  }
  return null;
}

function inferFieldAccessType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
): string | null {
  const match = /^([a-z_]\w*)\.([a-z_]\w*)$/.exec(expression);
  if (!match?.[1] || !match[2]) return null;
  const entityName = types.get(match[1]);
  if (!entityName) return null;
  return entityTypes.get(entityName)?.find((field) => field.name === match[2])?.type ?? null;
}

function inferEntityConstructionType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  actionTypes: Map<string, SophiaActionSignature>,
  stateTypes: SophiaStateTypes,
): string | null {
  const match = /^([A-Z][A-Za-z0-9]*)\s*\{\s*(.*)\s*\}$/.exec(expression);
  if (!match?.[1] || match[2] === undefined) return null;
  const entityName = match[1];
  const expectedFields = entityTypes.get(entityName);
  if (!expectedFields) return null;
  const assignments = parseEntityAssignments(match[2]);
  if (!assignments) return null;
  if (assignments.size !== expectedFields.length) return null;
  return expectedFields.every((field) => {
    const expression = assignments.get(field.name);
    const actualType =
      expression === undefined
        ? null
        : inferSophiaExpressionType(expression, types, entityTypes, actionTypes, stateTypes);
    return actualType !== null && isSophiaTypeAssignable(actualType, field.type);
  })
    ? entityName
    : null;
}

function inferActionCallType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  actionTypes: Map<string, SophiaActionSignature>,
  stateTypes: SophiaStateTypes,
): string | null {
  const match = /^([A-Z][A-Za-z0-9]*)\s*\{\s*(.*)\s*\}$/.exec(expression);
  if (!match?.[1] || match[2] === undefined) return null;
  const signature = actionTypes.get(match[1]);
  if (!signature) return null;
  const assignments = parseEntityAssignments(match[2]);
  if (!assignments) return null;
  if (assignments.size !== signature.input.length) return null;
  return signature.input.every((field) => {
    const expression = assignments.get(field.name);
    const actualType =
      expression === undefined
        ? null
        : inferSophiaExpressionType(expression, types, entityTypes, actionTypes, stateTypes);
    return actualType !== null && isSophiaTypeAssignable(actualType, field.type);
  })
    ? signature.outputType
    : null;
}

export function parseEntityAssignments(body: string): Map<string, string> | null {
  const assignments = new Map<string, string>();
  const parts = splitTopLevelCommas(body)
    .map((part) => part.trim())
    .filter(Boolean);
  for (const part of parts) {
    const match = /^([a-z_]\w*)\s*=\s*(.+)$/.exec(part);
    if (!match?.[1] || !match[2] || assignments.has(match[1])) return null;
    assignments.set(match[1], match[2].trim());
  }
  return assignments;
}

function splitTopLevelCommas(body: string): string[] {
  const parts: string[] = [];
  let current = "";
  let quote: '"' | "'" | null = null;
  let escaped = false;
  let parenDepth = 0;
  let braceDepth = 0;
  let bracketDepth = 0;

  for (let index = 0; index < body.length; index += 1) {
    const char = body[index] ?? "";
    if (quote) {
      current += char;
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === quote) {
        quote = null;
      }
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
      current += char;
      continue;
    }
    if (char === "(") parenDepth += 1;
    if (char === ")") parenDepth -= 1;
    if (char === "{") braceDepth += 1;
    if (char === "}") braceDepth -= 1;
    if (char === "[") bracketDepth += 1;
    if (char === "]") bracketDepth -= 1;
    if (parenDepth < 0 || braceDepth < 0 || bracketDepth < 0) return [body];
    if (char === "," && parenDepth === 0 && braceDepth === 0 && bracketDepth === 0) {
      parts.push(current);
      current = "";
      continue;
    }
    current += char;
  }

  if (quote || parenDepth !== 0 || braceDepth !== 0 || bracketDepth !== 0) return [body];
  parts.push(current);
  return parts;
}

function splitArithmeticOperands(expression: string): string[] {
  return expression
    .split(/\s*[-+*/]\s*/)
    .map((operand) => operand.trim())
    .filter(Boolean);
}

function inferListLiteralType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes = new Map(),
  actionTypes: Map<string, SophiaActionSignature> = new Map(),
  stateTypes: SophiaStateTypes = new Map(),
): string | null | undefined {
  const match = /^\[\s*(.*)\s*\]$/.exec(expression);
  if (!match) return undefined;
  const content = match[1]?.trim() ?? "";
  if (!content) return null;
  const itemTypes = splitTopLevelCommas(content).map((item) =>
    inferListItemType(item.trim(), types, entityTypes, actionTypes, stateTypes),
  );
  if (itemTypes.every((type) => type === "Int")) return "List<Int>";
  if (itemTypes.every((type) => type === "Text")) return "List<Text>";
  return null;
}

function inferListItemType(
  expression: string,
  types: Map<string, string>,
  entityTypes: SophiaEntityTypes,
  actionTypes: Map<string, SophiaActionSignature>,
  stateTypes: SophiaStateTypes,
): "Int" | "Text" | null {
  const inferred = inferSophiaExpressionType(
    expression,
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  return inferred === "Int" || inferred === "Text" ? inferred : null;
}

export function collectSophiaExpressionIdentifiers(expression: string): string[] {
  const cleaned = expression
    .replace(/"[^"]*"/g, "")
    .replace(/'[^']*'/g, "")
    .replace(/\.[a-z_]\w*/g, "");
  const reserved = new Set(["unit", "true", "false", "and", "or", "not", "mod", "to_text"]);
  return [...cleaned.matchAll(/\b[a-z_]\w*\b/g)]
    .map((match) => match[0])
    .filter((identifier) => !reserved.has(identifier));
}
