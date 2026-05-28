import type { SophiaActionSignature } from "./expression.js";
import type { SophiaEntityTypes, SophiaField } from "./types.js";
import {
  parseSophiaImmediateAttributes,
  parseSophiaImmediateNamedBlocks,
  parseSophiaErrorVariantBlocks,
  parseSophiaTopLevelDeclarations,
  parseSophiaStateValueBlocks,
  type SophiaAttribute,
  type SophiaNamedBlock,
  type SophiaRawAst,
  type SophiaTopLevelDeclaration,
} from "./parser.js";
import { parseSophiaEffectNames, parseSophiaFieldDeclarations } from "./signature.js";

export interface ParsedAction {
  name: string;
  capability: string;
  hasCapability: boolean;
  hasBody: boolean;
  hasEffectsBlock: boolean;
  hasOutputBlock: boolean;
  inputBody: string;
  outputBody: string;
  inputFields: SophiaField[];
  outputFields: SophiaField[];
  effects: Set<string>;
  errors: Set<string>;
  outputType: string | null;
  intentConversion: boolean;
  body: string;
}

export interface ParsedEntity {
  name: string;
  fields: SophiaField[];
}

export interface ParsedCapability {
  name: string;
  allow: string[];
  deny: string[];
}

export interface SophiaCapabilityPolicy {
  allow: Set<string>;
  deny: Set<string>;
}

export interface ParsedStorage {
  name: string;
  keyType: string | null;
  valueType: string | null;
}

export interface ParsedErrorVariant {
  errorName: string;
  name: string;
  fields: SophiaField[];
}

export interface ParsedError {
  name: string;
  variants: ParsedErrorVariant[];
}

export interface ParsedState {
  name: string;
  values: string[];
}

export interface SophiaStorageTypes {
  [name: string]: {
    keyType: string;
    valueType: string;
  };
}

export interface SophiaFileSet {
  [path: string]: string;
}

export function parseActions(content: string): ParsedAction[] {
  const actions: ParsedAction[] = [];
  for (const declaration of actionDeclarations(content)) {
    actions.push(parseActionFromBody(declaration.name, declaration.body));
  }
  return actions;
}

export function parseCapabilities(files: SophiaFileSet): Map<string, SophiaCapabilityPolicy> {
  const capabilities = new Map<string, SophiaCapabilityPolicy>();
  for (const content of Object.values(files)) {
    for (const declaration of capabilityDeclarations(content)) {
      const blocks = parseSophiaImmediateNamedBlocks(declaration.body);
      capabilities.set(declaration.name, parseCapabilityPolicy(blocks));
    }
  }
  return capabilities;
}

export function parseEntities(files: SophiaFileSet): SophiaEntityTypes {
  const entities: SophiaEntityTypes = new Map();
  for (const content of Object.values(files)) {
    for (const declaration of parseEntityDeclarations(content)) {
      entities.set(declaration.name, declaration.fields);
    }
  }
  return entities;
}

export function parseStates(files: SophiaFileSet): Map<string, string[]> {
  const states = new Map<string, string[]>();
  for (const content of Object.values(files)) {
    for (const declaration of parseStateDeclarations(content)) {
      states.set(declaration.name, declaration.values);
    }
  }
  return states;
}

export function parseStorages(
  files: SophiaFileSet,
): Map<string, { keyType: string; valueType: string }> {
  const storages = new Map<string, { keyType: string; valueType: string }>();
  for (const content of Object.values(files)) {
    for (const declaration of storageDeclarations(content)) {
      const storage = parseStorageFromBody(declaration.name, declaration.body);
      if (storage.keyType && storage.valueType) {
        storages.set(storage.name, {
          keyType: storage.keyType,
          valueType: storage.valueType,
        });
      }
    }
  }
  return storages;
}

export function parseErrors(files: SophiaFileSet): Map<string, ParsedErrorVariant> {
  const variants = new Map<string, ParsedErrorVariant>();
  for (const content of Object.values(files)) {
    for (const declaration of errorDeclarations(content)) {
      for (const variant of parseErrorFromBody(declaration.name, declaration.body).variants) {
        variants.set(variant.name, variant);
      }
    }
  }
  return variants;
}

export function parseActionSignatures(files: SophiaFileSet): Map<string, SophiaActionSignature> {
  const actions = new Map<string, SophiaActionSignature>();
  for (const content of Object.values(files)) {
    for (const action of parseActions(content)) {
      actions.set(action.name, {
        name: action.name,
        input: action.inputFields,
        outputType: action.outputType ?? "Unit",
        effects: action.effects,
        errors: action.errors,
      });
    }
  }
  return actions;
}

export function parseEntityDeclarations(content: string): ParsedEntity[] {
  return parseSophiaTopLevelDeclarations(content)
    .filter((declaration) => declaration.kind === "entity")
    .map((declaration) => parseEntityFromBody(declaration.name, declaration.body));
}

export function parseStorageDeclarations(content: string): ParsedStorage[] {
  return storageDeclarations(content).map((declaration) =>
    parseStorageFromBody(declaration.name, declaration.body),
  );
}

export function parseStateDeclarations(content: string): ParsedState[] {
  return stateDeclarations(content).map((declaration) =>
    parseStateFromBody(declaration.name, declaration.body),
  );
}

export function parseErrorDeclarations(content: string): ParsedError[] {
  return errorDeclarations(content).map((declaration) =>
    parseErrorFromBody(declaration.name, declaration.body),
  );
}

export function parseActionAst(ast: SophiaRawAst): ParsedAction {
  assertAstKind(ast, "action");
  return parseActionParts(ast.name, ast.attributes, ast.blocks);
}

export function parseEntityAst(ast: SophiaRawAst): ParsedEntity {
  assertAstKind(ast, "entity");
  return parseEntityParts(ast.name, ast.blocks);
}

export function parseCapabilityAst(ast: SophiaRawAst): ParsedCapability {
  assertAstKind(ast, "capability");
  const policy = parseCapabilityPolicy(ast.blocks);
  return {
    name: ast.name,
    allow: [...policy.allow],
    deny: [...policy.deny],
  };
}

export function parseStorageAst(ast: SophiaRawAst): ParsedStorage {
  assertAstKind(ast, "storage");
  return parseStorageParts(ast.name, ast.attributes);
}

export function parseStateAst(ast: SophiaRawAst): ParsedState {
  assertAstKind(ast, "state");
  return parseStateParts(ast.name, ast.blocks);
}

export function parseErrorAst(ast: SophiaRawAst): ParsedError {
  assertAstKind(ast, "error");
  return parseErrorParts(ast.name, ast.blocks);
}

export function parseCapabilityNames(content: string): string[] {
  return capabilityDeclarations(content).map((declaration) => declaration.name);
}

function actionDeclarations(content: string): SophiaTopLevelDeclaration[] {
  return parseSophiaTopLevelDeclarations(content).filter(
    (declaration) => declaration.kind === "action",
  );
}

function capabilityDeclarations(content: string): SophiaTopLevelDeclaration[] {
  return parseSophiaTopLevelDeclarations(content).filter(
    (declaration) => declaration.kind === "capability",
  );
}

function storageDeclarations(content: string): SophiaTopLevelDeclaration[] {
  return parseSophiaTopLevelDeclarations(content).filter(
    (declaration) => declaration.kind === "storage",
  );
}

function stateDeclarations(content: string): SophiaTopLevelDeclaration[] {
  return parseSophiaTopLevelDeclarations(content).filter(
    (declaration) => declaration.kind === "state",
  );
}

function errorDeclarations(content: string): SophiaTopLevelDeclaration[] {
  return parseSophiaTopLevelDeclarations(content).filter(
    (declaration) => declaration.kind === "error",
  );
}

function parseActionFromBody(name: string, bodyText: string): ParsedAction {
  return parseActionParts(
    name,
    parseSophiaImmediateAttributes(bodyText),
    parseSophiaImmediateNamedBlocks(bodyText),
  );
}

function parseEntityFromBody(name: string, bodyText: string): ParsedEntity {
  return parseEntityParts(name, parseSophiaImmediateNamedBlocks(bodyText));
}

function parseStorageFromBody(name: string, bodyText: string): ParsedStorage {
  return parseStorageParts(name, parseSophiaImmediateAttributes(bodyText));
}

function parseStateFromBody(name: string, bodyText: string): ParsedState {
  return parseStateParts(name, parseSophiaStateValueBlocks(bodyText));
}

function parseErrorFromBody(name: string, bodyText: string): ParsedError {
  return parseErrorParts(name, parseSophiaErrorVariantBlocks(bodyText));
}

function parseCapabilityPolicy(blocks: SophiaNamedBlock[]): SophiaCapabilityPolicy {
  const allowBody = blocks.find((block) => block.name === "allow")?.body ?? "";
  const denyBody = blocks.find((block) => block.name === "deny")?.body ?? "";
  return {
    allow: new Set(parseSophiaEffectNames(allowBody)),
    deny: new Set(parseSophiaEffectNames(denyBody)),
  };
}

function parseActionParts(
  name: string,
  attributes: SophiaAttribute[],
  blocks: SophiaNamedBlock[],
): ParsedAction {
  const capability = attributes.find((attribute) => attribute.name === "capability")?.value ?? "";
  const intentConversion =
    attributes.find((attribute) => attribute.name === "intent_conversion")?.value === "true";
  const effectsBlock = blocks.find((block) => block.name === "effects");
  const errorsBlock = blocks.find((block) => block.name === "errors");
  const inputBlock = blocks.find((block) => block.name === "input");
  const outputBlock = blocks.find((block) => block.name === "output");
  const bodyBlock = blocks.find((block) => block.name === "body");
  const effectsBody = effectsBlock?.body ?? "";
  const errorsBody = errorsBlock?.body ?? "";
  const inputBody = inputBlock?.body ?? "";
  const outputBody = outputBlock?.body ?? "";
  const inputFields = parseSophiaFieldDeclarations(inputBody);
  const outputFields = parseSophiaFieldDeclarations(outputBody);
  const body = bodyBlock?.body ?? "";
  return {
    name,
    capability,
    hasCapability: Boolean(capability),
    hasBody: bodyBlock !== undefined,
    hasEffectsBlock: effectsBlock !== undefined,
    hasOutputBlock: outputBlock !== undefined,
    inputBody,
    outputBody,
    inputFields,
    outputFields,
    effects: new Set(parseSophiaEffectNames(effectsBody)),
    errors: new Set(parseSophiaErrorNames(errorsBody)),
    outputType: outputFields[0]?.type ?? null,
    intentConversion,
    body,
  };
}

function parseErrorParts(name: string, blocks: SophiaNamedBlock[]): ParsedError {
  return {
    name,
    variants: blocks.map((block) => ({
      errorName: name,
      name: block.name,
      fields: parseSophiaFieldDeclarations(block.body),
    })),
  };
}

function parseStateParts(name: string, blocks: SophiaNamedBlock[]): ParsedState {
  return {
    name,
    values: blocks.map((block) => block.name),
  };
}

function parseEntityParts(name: string, blocks: SophiaNamedBlock[]): ParsedEntity {
  return {
    name,
    fields: parseSophiaFieldDeclarations(
      blocks.find((block) => block.name === "fields")?.body ?? "",
    ),
  };
}

function parseStorageParts(name: string, attributes: SophiaAttribute[]): ParsedStorage {
  return {
    name,
    keyType: attributes.find((attribute) => attribute.name === "key")?.value ?? null,
    valueType: attributes.find((attribute) => attribute.name === "value")?.value ?? null,
  };
}

function parseSophiaErrorNames(block: string): string[] {
  return [...block.matchAll(/\b[A-Z][A-Za-z0-9]*\b/g)].map((match) => match[0]);
}

function assertAstKind(ast: SophiaRawAst, kind: string): void {
  if (ast.kind !== kind) {
    throw new Error(`Expected ${kind} AST, got ${ast.kind}.`);
  }
}
