import { flattenSophiaBodyStatements, parseSophiaBody } from "./body_ast.js";
import { buildBodyStatementTypeEnvironments } from "./body_return.js";
import { checkVariableLifecycle } from "./body_lifecycle.js";
import { checkReturnShape } from "./body_return.js";
import { parseActions } from "./check_model.js";
import { error } from "./diagnostics.js";
import { parseSophiaFieldDeclarations, parseSophiaStorageEffect } from "./signature.js";
import type { CheckerContext } from "./checker_context.js";
import {
  isSupportedEffect,
  isSupportedType,
  unsupportedEffectDiagnostic,
  unsupportedTypeDiagnostic,
} from "./checker_declaration_rules.js";
import { inferSophiaExpressionType } from "./expression.js";
import { parseSophiaIntentType, unwrapSophiaIntentType } from "./types.js";

export function checkActionDeclarations(
  context: CheckerContext,
  filePath: string,
  content: string,
): void {
  for (const action of parseActions(content)) {
    if (context.actionNames.has(action.name)) {
      context.diagnostics.push(
        error("CHECK-ACTION-004", filePath, `Duplicate action declaration: ${action.name}.`),
      );
    }
    context.actionNames.add(action.name);
    checkActionContract(context, filePath, action);
    const bodyAst = checkActionBody(context, filePath, action);
    checkIntentConversionContract(context, filePath, action, bodyAst);
    context.diagnostics.push(
      ...checkReturnShape(
        filePath,
        action,
        bodyAst.statements,
        context.entityTypes,
        context.stateTypes,
        context.actionTypes,
      ),
    );
  }
}

function checkActionContract(
  context: CheckerContext,
  filePath: string,
  action: ReturnType<typeof parseActions>[number],
): void {
  if (!action.hasCapability) {
    context.diagnostics.push(
      error("CHECK-ACTION-001", filePath, `Action ${action.name} is missing capability binding.`),
    );
  }
  if (!action.hasEffectsBlock) {
    context.diagnostics.push(
      error("CHECK-ACTION-002", filePath, `Action ${action.name} is missing effects block.`),
    );
  }
  if (!action.hasBody) {
    context.diagnostics.push(
      error("CHECK-ACTION-003", filePath, `Action ${action.name} is missing body block.`),
    );
  }
  if (!action.hasOutputBlock) {
    context.diagnostics.push(
      error("CHECK-ACTION-005", filePath, `Action ${action.name} is missing output block.`),
    );
  }
  if (action.outputFields.length > 1) {
    context.diagnostics.push(
      error(
        "CHECK-OUTPUT-001",
        filePath,
        `Action ${action.name} declares multiple output fields.`,
        "Sophia v0 actions must declare exactly one output field.",
      ),
    );
  }
  for (const typeName of [
    ...extractFieldTypes(action.inputBody),
    ...extractFieldTypes(action.outputBody),
  ]) {
    if (!isSupportedType(typeName, context.entityTypes, context.stateTypes)) {
      context.diagnostics.push(unsupportedTypeDiagnostic(filePath, typeName));
    }
  }
  for (const variantName of action.errors) {
    if (!context.errorVariants.has(variantName)) {
      context.diagnostics.push(
        error(
          "CHECK-ERROR-009",
          filePath,
          `Action ${action.name} declares unknown error variant: ${variantName}.`,
        ),
      );
    }
  }
}

function checkActionBody(
  context: CheckerContext,
  filePath: string,
  action: ReturnType<typeof parseActions>[number],
): ReturnType<typeof parseSophiaBody> {
  const body = action.body;
  if (containsNaturalLanguageBodyLine(body)) {
    context.diagnostics.push(
      error(
        "CHECK-BODY-001",
        filePath,
        "Natural language text found inside Sophia-Core body.",
        "Replace prose with let/set/print/repeat/return statements.",
      ),
    );
  }
  const bodyAst = parseSophiaBody(body, filePath);
  context.diagnostics.push(...bodyAst.diagnostics);
  context.diagnostics.push(
    ...checkVariableLifecycle(
      filePath,
      action,
      bodyAst.statements,
      context.entityTypes,
      context.stateTypes,
      context.errorVariants,
      context.actionTypes,
    ),
  );

  const flattenedBody = flattenSophiaBodyStatements(bodyAst.statements);
  const usesPrint =
    flattenedBody.some((statement) => statement.kind === "print") ||
    /\bConsole\.Write\s*\(/.test(body);
  if (/\bConsole\.Write\s*\(/.test(body)) {
    context.diagnostics.push(
      error(
        "CHECK-BODY-002",
        filePath,
        "Body calls Console.Write directly; v0 body uses print statements.",
        "Use print expr and declare Console.Write in effects.",
      ),
    );
  }
  if (/(^|[^\w.])append\s*\(/m.test(body)) {
    context.diagnostics.push(
      error(
        "CHECK-BODY-003",
        filePath,
        "Body uses unsupported function-style append(...).",
        "Use set list = list + [item] or set list = list.append(item).",
      ),
    );
  }
  if (usesPrint && !action.effects.has("Console.Write")) {
    context.diagnostics.push(
      error(
        "CHECK-EFFECT-001",
        filePath,
        "Body uses print but effects does not include Console.Write.",
        "Add Console.Write to effects. Do not combine Pure with Console.Write.",
      ),
    );
  }
  checkConsoleWriteIntentPolicy(context, filePath, action, bodyAst.statements);
  checkStorageWriteIntentPolicy(context, filePath, action);
  if (action.effects.has("Pure") && action.effects.size > 1) {
    context.diagnostics.push(
      error(
        "CHECK-EFFECT-002",
        filePath,
        "Action effects combine Pure with observable effects.",
        "Use either Pure alone or list the concrete observable effects.",
      ),
    );
  }
  for (const effect of action.effects) {
    if (!isSupportedEffect(effect)) {
      context.diagnostics.push(unsupportedEffectDiagnostic(filePath, effect));
    }
  }
  checkCapabilityUse(context, filePath, action);
  return bodyAst;
}

function checkIntentConversionContract(
  context: CheckerContext,
  filePath: string,
  action: ReturnType<typeof parseActions>[number],
  bodyAst: ReturnType<typeof parseSophiaBody>,
): void {
  if (!action.intentConversion) return;

  if (action.inputFields.length !== 1 || action.outputFields.length !== 1 || !action.outputType) {
    context.diagnostics.push(
      error(
        "CHECK-INTENT-CONVERSION-001",
        filePath,
        `Intent conversion action ${action.name} must declare exactly one input and one output.`,
      ),
    );
    return;
  }

  const input = action.inputFields[0];
  const output = action.outputFields[0];
  const inputIntent = input ? parseSophiaIntentType(input.type) : null;
  const outputIntent = output ? parseSophiaIntentType(output.type) : null;
  if (
    !input ||
    !output ||
    !inputIntent ||
    !outputIntent ||
    inputIntent.intent === outputIntent.intent ||
    unwrapSophiaIntentType(input.type) !== unwrapSophiaIntentType(output.type)
  ) {
    context.diagnostics.push(
      error(
        "CHECK-INTENT-CONVERSION-002",
        filePath,
        `Intent conversion action ${action.name} must convert between different intents over the same inner type.`,
        "Use a shape such as input Raw<Text> and output Sanitized<Text>.",
      ),
    );
  }

  if (action.effects.size > 0) {
    context.diagnostics.push(
      error(
        "CHECK-INTENT-CONVERSION-003",
        filePath,
        `Intent conversion action ${action.name} must not declare effects.`,
      ),
    );
  }

  const flattenedBody = flattenSophiaBodyStatements(bodyAst.statements);
  const onlyStatement = flattenedBody.length === 1 ? flattenedBody[0] : null;
  if (
    !onlyStatement ||
    onlyStatement.kind !== "return" ||
    onlyStatement.expression !== input?.name
  ) {
    context.diagnostics.push(
      error(
        "CHECK-INTENT-CONVERSION-004",
        filePath,
        `Intent conversion action ${action.name} must return its single input directly.`,
      ),
    );
  }
}

function checkConsoleWriteIntentPolicy(
  context: CheckerContext,
  filePath: string,
  action: ReturnType<typeof parseActions>[number],
  statements: ReturnType<typeof parseSophiaBody>["statements"],
): void {
  const typeEnvironments = buildBodyStatementTypeEnvironments(
    action,
    statements,
    context.entityTypes,
    context.stateTypes,
    context.actionTypes,
  );
  for (const statement of flattenSophiaBodyStatements(statements)) {
    if (statement.kind !== "print") continue;
    const typeEnvironment = typeEnvironments.get(statement) ?? new Map();
    const expressionType = inferSophiaExpressionType(
      statement.expression,
      typeEnvironment,
      context.entityTypes,
      context.actionTypes,
      context.stateTypes,
    );
    const intentType = expressionType ? parseSophiaIntentType(expressionType) : null;
    if (intentType && (intentType.intent === "Sanitized" || intentType.intent === "Redacted")) {
      continue;
    }
    if (!intentType && isConsoleLiteralExpression(statement.expression)) continue;
    if (
      !intentType &&
      expressionType === "Text" &&
      isConsoleToTextExpression(statement.expression)
    ) {
      continue;
    }
    context.diagnostics.push(
      error(
        "CHECK-INTENT-BOUNDARY-001",
        `${filePath}:${statement.line}`,
        `Console.Write cannot output ${expressionType ?? "unknown"}; external output requires a literal, Sanitized<T>, or Redacted<T>.`,
      ),
    );
  }
}

function isConsoleToTextExpression(expression: string): boolean {
  return /^to_text\(.+\)$/.test(expression.trim());
}

function isConsoleLiteralExpression(expression: string): boolean {
  const trimmed = expression.trim();
  return /^(?:"[^"]*"|'[^']*'|-?\d+|true|false)$/.test(trimmed);
}

function checkStorageWriteIntentPolicy(
  context: CheckerContext,
  filePath: string,
  action: ReturnType<typeof parseActions>[number],
): void {
  for (const effect of action.effects) {
    if (effect === "DB.Write" || effect === "DB.Read") {
      context.diagnostics.push(
        error(
          "CHECK-EFFECT-003",
          filePath,
          `Effect ${effect} must name a storage target.`,
          'Use DB.Write("StorageName") or DB.Read("StorageName").',
        ),
      );
    }
    const storageEffect = parseSophiaStorageEffect(effect);
    if (!storageEffect) continue;
    if (storageEffect.mode === "Read") {
      if (!context.storageTypes.has(storageEffect.storage)) {
        context.diagnostics.push(
          error(
            "CHECK-STORAGE-READ-001",
            filePath,
            `Action ${action.name} reads unknown storage: ${storageEffect.storage}.`,
          ),
        );
      }
      continue;
    }
    const storage = context.storageTypes.get(storageEffect.storage);
    if (!storage) {
      context.diagnostics.push(
        error(
          "CHECK-STORAGE-WRITE-001",
          filePath,
          `Action ${action.name} writes unknown storage: ${storageEffect.storage}.`,
        ),
      );
      continue;
    }
    if (action.outputType !== storage.valueType) {
      context.diagnostics.push(
        error(
          "CHECK-STORAGE-WRITE-002",
          filePath,
          `Action ${action.name} declares ${effect}, but output type ${action.outputType ?? "Unit"} does not match storage ${storageEffect.storage} value type ${storage.valueType}.`,
          "Return the exact storage value type before declaring DB.Write for that storage.",
        ),
      );
    }
  }
}

function checkCapabilityUse(
  context: CheckerContext,
  filePath: string,
  action: ReturnType<typeof parseActions>[number],
): void {
  const policy = context.capabilities.get(action.capability);
  if (!policy) {
    context.diagnostics.push(
      error(
        "CHECK-CAPABILITY-001",
        filePath,
        `Action references unknown capability: ${action.capability}.`,
      ),
    );
    return;
  }
  for (const effect of action.effects) {
    if (policy.deny.has(effect)) {
      context.diagnostics.push(
        error(
          "CHECK-CAPABILITY-004",
          filePath,
          `Action effect ${effect} is denied by capability ${action.capability}.`,
        ),
      );
      continue;
    }
    if (!policy.allow.has(effect)) {
      context.diagnostics.push(
        error(
          "CHECK-CAPABILITY-002",
          filePath,
          `Action effect ${effect} is not allowed by capability ${action.capability}.`,
        ),
      );
    }
  }
}

function containsNaturalLanguageBodyLine(body: string): boolean {
  return body
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .some((line) => /^["']?[A-Z][a-z]+(?:\s+[a-z]+){3,}/.test(line));
}

function extractFieldTypes(block: string): string[] {
  return parseSophiaFieldDeclarations(block).map((field) => field.type);
}
