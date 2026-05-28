import { parseSophiaBody, type SophiaBodyStatement } from "../lang/body/ast.js";
import { inferEmptyListTypeForVariable } from "../lang/body/empty_list.js";
import {
  inferSophiaExpressionType,
  parseEntityAssignments,
  type SophiaActionSignature,
} from "../lang/ast/expression.js";
import {
  sophiaTypeToTypeScript,
  parseSophiaOptionalType,
  type SophiaEntityTypes,
  type SophiaStateTypes,
} from "../lang/ast/types.js";
import { indent } from "../util/strings.js";

export function emitBody(options: {
  body: string;
  types: Map<string, string>;
  outputType: string;
  entityTypes: SophiaEntityTypes;
  stateTypes: SophiaStateTypes;
  actionTypes: Map<string, SophiaActionSignature>;
}): string {
  const ast = parseSophiaBody(options.body, "<codegen>");
  if (ast.diagnostics.length > 0) {
    throw new Error(ast.diagnostics.map((diagnostic) => diagnostic.problem).join("; "));
  }
  return emitStatements(
    ast.statements,
    options.types,
    options.outputType,
    options.entityTypes,
    options.stateTypes,
    options.actionTypes,
  );
}

function matchTempVar(line: number): string {
  return `__match_${line}`;
}

function emitStatements(
  statements: SophiaBodyStatement[],
  types: Map<string, string>,
  outputType: string,
  entityTypes: SophiaEntityTypes,
  stateTypes: SophiaStateTypes,
  actionTypes: Map<string, SophiaActionSignature>,
): string {
  const output: string[] = [];
  for (let index = 0; index < statements.length; index += 1) {
    const statement = statements[index];
    if (!statement) continue;
    if (statement.kind === "repeat") {
      output.push(
        `for (let __i${statement.line} = 0; __i${statement.line} < ${statement.count}; __i${statement.line} += 1) {`,
      );
      output.push(
        indent(
          emitStatements(statement.body, types, outputType, entityTypes, stateTypes, actionTypes),
          2,
        ),
      );
      output.push("}");
      continue;
    }
    if (statement.kind === "if") {
      output.push(`if (${emitExpression(statement.condition, actionTypes)}) {`);
      output.push(
        indent(
          emitStatements(
            statement.thenBody,
            new Map(types),
            outputType,
            entityTypes,
            stateTypes,
            actionTypes,
          ),
          2,
        ),
      );
      if (statement.elseBody.length > 0) {
        output.push("} else {");
        output.push(
          indent(
            emitStatements(
              statement.elseBody,
              new Map(types),
              outputType,
              entityTypes,
              stateTypes,
              actionTypes,
            ),
            2,
          ),
        );
        output.push("}");
      } else {
        output.push("}");
      }
      continue;
    }
    if (statement.kind === "match") {
      output.push(
        emitMatchStatement(statement, types, outputType, entityTypes, stateTypes, actionTypes),
      );
      continue;
    }
    if (statement.kind === "let") {
      const expression = emitExpression(statement.expression, actionTypes);
      const sophiaType =
        expression === "[]"
          ? inferEmptyListTypeForVariable({
              name: statement.name,
              statements: statements.slice(index + 1),
              types,
              outputType,
              entityTypes,
              actionTypes,
              stateTypes,
            })
          : inferSophiaExpressionType(
              statement.expression,
              types,
              entityTypes,
              actionTypes,
              stateTypes,
            );
      if (sophiaType) types.set(statement.name, sophiaType);
      output.push(
        expression === "[]"
          ? `let ${statement.name}: ${sophiaTypeToTypeScript(sophiaType ?? "UnknownList")} = [];`
          : `let ${statement.name} = ${expression};`,
      );
      continue;
    }
    if (statement.kind === "set") {
      const inferredType = inferSophiaExpressionType(
        statement.expression,
        types,
        entityTypes,
        actionTypes,
        stateTypes,
      );
      if (inferredType) types.set(statement.name, inferredType);
      output.push(`${statement.name} = ${emitExpression(statement.expression, actionTypes)};`);
      continue;
    }
    if (statement.kind === "print") {
      output.push(`effects.write(String(${emitExpression(statement.expression, actionTypes)}));`);
      continue;
    }
    if (statement.kind === "return") {
      output.push(`return ${emitExpression(statement.expression, actionTypes)};`);
      continue;
    }
    if (statement.kind === "raise") {
      output.push(`throw ${emitRaiseExpression(statement.expression, actionTypes)};`);
      continue;
    }
  }
  return output.join("\n");
}

function emitMatchStatement(
  statement: Extract<SophiaBodyStatement, { kind: "match" }>,
  types: Map<string, string>,
  outputType: string,
  entityTypes: SophiaEntityTypes,
  stateTypes: SophiaStateTypes,
  actionTypes: Map<string, SophiaActionSignature>,
): string {
  const output: string[] = [];
  const temporary = matchTempVar(statement.line);
  const matchedType = inferSophiaExpressionType(
    statement.expression,
    types,
    entityTypes,
    actionTypes,
    stateTypes,
  );
  const optionalType = matchedType ? parseSophiaOptionalType(matchedType) : null;
  output.push(`const ${temporary} = ${emitExpression(statement.expression, actionTypes)};`);
  for (let index = 0; index < statement.cases.length; index += 1) {
    const matchCase = statement.cases[index];
    if (!matchCase) continue;
    const isFirst = index === 0;
    const isLast = index === statement.cases.length - 1;
    const prefix = matchPrefix(isFirst, isLast, temporary, matchCase);
    output.push(prefix);
    const branchTypes = new Map(types);
    const branchLines: string[] = [];
    if (matchCase.pattern === "Some" && matchCase.binding && optionalType) {
      branchTypes.set(matchCase.binding, optionalType.innerType);
      branchLines.push(`const ${matchCase.binding} = ${temporary};`);
    }
    branchLines.push(
      emitStatements(matchCase.body, branchTypes, outputType, entityTypes, stateTypes, actionTypes),
    );
    output.push(indent(branchLines.filter(Boolean).join("\n"), 2));
  }
  output.push("}");
  return output.join("\n");
}

function matchPrefix(
  isFirst: boolean,
  isLast: boolean,
  temporary: string,
  matchCase: { pattern: string; binding: string | null },
): string {
  if (isFirst && isLast) return "{";
  if (isFirst) return `if (${emitMatchCondition(temporary, matchCase)}) {`;
  if (isLast) return "} else {";
  return `} else if (${emitMatchCondition(temporary, matchCase)}) {`;
}

function emitMatchCondition(
  temporary: string,
  matchCase: { pattern: string; binding: string | null },
): string {
  if (matchCase.pattern === "Some") return `${temporary} !== null`;
  if (matchCase.pattern === "None") return `${temporary} === null`;
  if (matchCase.pattern === "true" || matchCase.pattern === "false") {
    return `${temporary} === ${matchCase.pattern}`;
  }
  return `${temporary} === ${matchCase.pattern}`;
}

function emitRaiseExpression(
  expression: string,
  actionTypes: Map<string, SophiaActionSignature>,
): string {
  const match = /^([A-Z][A-Za-z0-9]*)\s*\{\s*(.*)\s*\}$/.exec(expression.trim());
  if (!match?.[1] || match[2] === undefined) return emitExpression(expression, actionTypes);
  const assignments = parseEntityAssignments(match[2]);
  const fields = assignments
    ? [...assignments.entries()].map(
        ([name, value]) => `${name}: ${emitExpression(value, actionTypes)}`,
      )
    : [];
  return `{ kind: "${match[1]}"${fields.length > 0 ? `, ${fields.join(", ")}` : ""} }`;
}

function emitExpression(
  expression: string,
  actionTypes: Map<string, SophiaActionSignature> = new Map(),
): string {
  const trimmed = expression.trim();
  if (trimmed === "unit") return "undefined";
  if (trimmed === "None") return "null";
  const someMatch = /^Some\((.+)\)$/.exec(trimmed);
  if (someMatch?.[1]) return emitExpression(someMatch[1], actionTypes);
  const toTextMatch = /^to_text\((.+)\)$/.exec(trimmed);
  if (toTextMatch?.[1]) return `String(${emitExpression(toTextMatch[1], actionTypes)})`;
  const actionMatch = /^([A-Z][A-Za-z0-9]*)\s*\{\s*(.*)\s*\}$/.exec(trimmed);
  if (actionMatch?.[1] && actionMatch[2] !== undefined && actionTypes.has(actionMatch[1])) {
    const assignments = parseEntityAssignments(actionMatch[2]);
    if (assignments) {
      return `${actionMatch[1]}({ ${[...assignments.entries()]
        .map(([name, value]) => `${name}: ${emitExpression(value, actionTypes)}`)
        .join(", ")} }, effects)`;
    }
  }
  const entityMatch = /^([A-Z][A-Za-z0-9]*)\s*\{\s*(.*)\s*\}$/.exec(trimmed);
  if (entityMatch?.[2] !== undefined) {
    const assignments = parseEntityAssignments(entityMatch[2]);
    if (assignments) {
      return `{ ${[...assignments.entries()]
        .map(([name, value]) => `${name}: ${emitExpression(value, actionTypes)}`)
        .join(", ")} }`;
    }
  }
  const appendMatch = /^([a-z_]\w*)\.append\((.+)\)$/.exec(trimmed);
  if (appendMatch?.[1] && appendMatch[2]) {
    return `${appendMatch[1]}.concat([${emitExpression(appendMatch[2], actionTypes)}])`;
  }
  const concatMatch = /^([a-z_]\w*)\s*\+\s*(\[.+\])$/.exec(trimmed);
  if (concatMatch?.[1] && concatMatch[2]) {
    return `${concatMatch[1]}.concat(${concatMatch[2]})`;
  }
  return emitBooleanOperators(emitStrictEquality(trimmed));
}

function emitBooleanOperators(expression: string): string {
  let output = "";
  let quote: '"' | "'" | null = null;
  let escaped = false;
  let index = 0;
  while (index < expression.length) {
    const char = expression[index] ?? "";
    if (quote) {
      output += char;
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === quote) {
        quote = null;
      }
      index += 1;
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
      output += char;
      index += 1;
      continue;
    }
    const rest = expression.slice(index);
    const operatorMatch = /^(and|or|not)\b/.exec(rest);
    const previous = expression[index - 1] ?? " ";
    const operator = operatorMatch?.[1];
    if (operator && !/\w/.test(previous)) {
      output += operator === "and" ? "&&" : operator === "or" ? "||" : "!";
      index += operator.length;
      if (operator === "not") {
        while (expression[index] === " ") index += 1;
      }
      continue;
    }
    output += char;
    index += 1;
  }
  return output;
}

function emitStrictEquality(expression: string): string {
  let output = "";
  let quote: '"' | "'" | null = null;
  let escaped = false;

  for (let index = 0; index < expression.length; index += 1) {
    const char = expression[index] ?? "";
    const next = expression[index + 1] ?? "";
    const previous = expression[index - 1] ?? "";

    if (quote) {
      output += char;
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
      output += char;
      continue;
    }

    if (char === "=" && next === "=" && previous !== "=" && expression[index + 2] !== "=") {
      output += "===";
      index += 1;
      continue;
    }
    if (char === "!" && next === "=" && expression[index + 2] !== "=") {
      output += "!==";
      index += 1;
      continue;
    }

    output += char;
  }

  return output;
}
