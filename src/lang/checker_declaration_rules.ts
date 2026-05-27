import {
  parseCapabilityNames,
  parseEntityDeclarations,
  parseErrorDeclarations,
  parseStateDeclarations,
  parseStorageDeclarations,
} from "./check_model.js";
import { error, type Diagnostic } from "./diagnostics.js";
import { parseSophiaImmediateNamedBlocks, parseSophiaTopLevelDeclarations } from "./parser.js";
import { isSophiaStorageEffect, parseSophiaEffectNames } from "./signature.js";
import { isSupportedSophiaType, type SophiaEntityTypes } from "./types.js";
import type { CheckerContext } from "./checker_context.js";

export function checkCapabilityDeclarations(
  context: CheckerContext,
  filePath: string,
  content: string,
): void {
  for (const capabilityName of parseCapabilityNames(content)) {
    if (context.capabilityNames.has(capabilityName)) {
      context.diagnostics.push(
        error(
          "CHECK-CAPABILITY-003",
          filePath,
          `Duplicate capability declaration: ${capabilityName}.`,
        ),
      );
    }
    context.capabilityNames.add(capabilityName);
  }
  for (const declaration of parseSophiaTopLevelDeclarations(content).filter(
    (declaration) => declaration.kind === "capability",
  )) {
    const blocks = parseSophiaImmediateNamedBlocks(declaration.body).filter((block) =>
      ["allow", "deny"].includes(block.name),
    );
    for (const effect of blocks.flatMap((block) => parseSophiaEffectNames(block.body))) {
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
      if (!isSupportedEffect(effect)) {
        context.diagnostics.push(unsupportedEffectDiagnostic(filePath, effect));
      }
    }
  }
}

export function checkEntityDeclarations(
  context: CheckerContext,
  filePath: string,
  content: string,
): void {
  for (const entity of parseEntityDeclarations(content)) {
    if (context.entityNames.has(entity.name)) {
      context.diagnostics.push(
        error("CHECK-ENTITY-001", filePath, `Duplicate entity declaration: ${entity.name}.`),
      );
    }
    context.entityNames.add(entity.name);
    if (entity.fields.length === 0) {
      context.diagnostics.push(
        error(
          "CHECK-ENTITY-002",
          filePath,
          `Entity ${entity.name} declares no fields.`,
          "Declare explicit fields in fields { name: Type }.",
        ),
      );
    }
    for (const field of entity.fields) {
      if (!isSupportedType(field.type, context.entityTypes, context.stateTypes)) {
        context.diagnostics.push(unsupportedTypeDiagnostic(filePath, field.type));
      }
    }
  }
}

export function checkStateDeclarations(
  context: CheckerContext,
  filePath: string,
  content: string,
): void {
  for (const state of parseStateDeclarations(content)) {
    if (context.stateNames.has(state.name)) {
      context.diagnostics.push(
        error("CHECK-STATE-001", filePath, `Duplicate state declaration: ${state.name}.`),
      );
    }
    context.stateNames.add(state.name);
    if (state.values.length === 0) {
      context.diagnostics.push(
        error(
          "CHECK-STATE-002",
          filePath,
          `State ${state.name} declares no values.`,
          "Declare at least one value with value ValueName { }.",
        ),
      );
    }
    const seenValues = new Set<string>();
    for (const value of state.values) {
      if (seenValues.has(value)) {
        context.diagnostics.push(
          error("CHECK-STATE-003", filePath, `Duplicate state value declaration: ${value}.`),
        );
      }
      seenValues.add(value);
    }
  }
}

export function checkStorageDeclarations(
  context: CheckerContext,
  filePath: string,
  content: string,
): void {
  for (const storage of parseStorageDeclarations(content)) {
    if (!storage.keyType) {
      context.diagnostics.push(
        error(
          "CHECK-STORAGE-001",
          filePath,
          `Storage ${storage.name} is missing key type.`,
          "Declare key: Type directly inside the storage node.",
        ),
      );
    }
    if (!storage.valueType) {
      context.diagnostics.push(
        error(
          "CHECK-STORAGE-002",
          filePath,
          `Storage ${storage.name} is missing value type.`,
          "Declare value: Type directly inside the storage node.",
        ),
      );
    }
    for (const typeName of [storage.keyType, storage.valueType]) {
      if (typeName && !isSupportedType(typeName, context.entityTypes, context.stateTypes)) {
        context.diagnostics.push(unsupportedTypeDiagnostic(filePath, typeName));
      }
    }
  }
}

export function checkErrorDeclarations(
  context: CheckerContext,
  filePath: string,
  content: string,
): void {
  for (const declaration of parseErrorDeclarations(content)) {
    if (context.errorNames.has(declaration.name)) {
      context.diagnostics.push(
        error("CHECK-ERROR-001", filePath, `Duplicate error declaration: ${declaration.name}.`),
      );
    }
    context.errorNames.add(declaration.name);
    if (declaration.variants.length === 0) {
      context.diagnostics.push(
        error(
          "CHECK-ERROR-002",
          filePath,
          `Error ${declaration.name} declares no variants.`,
          "Declare at least one variant with variant VariantName { field: Type }.",
        ),
      );
    }
    for (const variant of declaration.variants) {
      if (context.errorVariantNames.has(variant.name)) {
        context.diagnostics.push(
          error(
            "CHECK-ERROR-003",
            filePath,
            `Duplicate error variant declaration: ${variant.name}.`,
          ),
        );
      }
      context.errorVariantNames.add(variant.name);
      for (const field of variant.fields) {
        if (!isSupportedType(field.type, context.entityTypes, context.stateTypes)) {
          context.diagnostics.push(unsupportedTypeDiagnostic(filePath, field.type));
        }
      }
    }
  }
}

export function unsupportedTypeDiagnostic(filePath: string, typeName: string): Diagnostic {
  return error(
    "CHECK-TYPE-001",
    filePath,
    `Unsupported v0 type: ${typeName}.`,
    "Use Unit, Bool, Int, Text, List<Int>, List<Text>, a declared entity/state type, an intent wrapper such as Raw<Text>, or Optional<T>.",
  );
}

export function isSupportedType(
  typeName: string,
  entityTypes: SophiaEntityTypes,
  stateTypes = new Map<string, string[]>(),
): boolean {
  return isSupportedSophiaType(typeName, entityTypes, stateTypes);
}

export function isSupportedEffect(effect: string): boolean {
  return effect === "Pure" || effect === "Console.Write" || isSophiaStorageEffect(effect);
}

export function unsupportedEffectDiagnostic(filePath: string, effect: string): Diagnostic {
  return error(
    "CHECK-EFFECT-004",
    filePath,
    `Unsupported v0 effect: ${effect}.`,
    'Use Pure, Console.Write, DB.Read("StorageName"), or DB.Write("StorageName").',
  );
}
