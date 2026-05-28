import { isRecord } from "../../util/json.js";

export const SOPHIA_V0_TYPES = ["Unit", "Bool", "Int", "Text", "List<Int>", "List<Text>"] as const;
export const SOPHIA_INTENT_TYPES = [
  "Raw",
  "Parsed",
  "Validated",
  "Sanitized",
  "Verified",
  "Authorized",
  "Persisted",
  "Secret",
  "Redacted",
] as const;

export type SophiaV0Type = (typeof SOPHIA_V0_TYPES)[number];
export type SophiaIntentType = (typeof SOPHIA_INTENT_TYPES)[number];
export interface SophiaField {
  name: string;
  type: string;
}

export type SophiaEntityTypes = Map<string, SophiaField[]>;
export type SophiaStateTypes = Map<string, string[]>;
export interface SophiaIntentWrappedType {
  intent: SophiaIntentType;
  innerType: string;
}

export interface SophiaOptionalType {
  innerType: string;
}

export function isSophiaV0Type(typeName: string): typeName is SophiaV0Type {
  return (SOPHIA_V0_TYPES as readonly string[]).includes(typeName);
}

export function parseSophiaIntentType(typeName: string): SophiaIntentWrappedType | null {
  const trimmed = typeName.trim();
  const match = /^([A-Z][A-Za-z0-9]*)<(.+)>$/.exec(trimmed);
  if (!match?.[1] || !match[2]) return null;
  if (!(SOPHIA_INTENT_TYPES as readonly string[]).includes(match[1])) return null;
  return {
    intent: match[1] as SophiaIntentType,
    innerType: match[2].trim(),
  };
}

export function parseSophiaOptionalType(typeName: string): SophiaOptionalType | null {
  const trimmed = typeName.trim();
  const match = /^Optional<(.+)>$/.exec(trimmed);
  if (!match?.[1]) return null;
  return {
    innerType: match[1].trim(),
  };
}

export function unwrapSophiaIntentType(typeName: string): string {
  return parseSophiaIntentType(typeName)?.innerType ?? typeName;
}

export function unwrapSophiaContextType(typeName: string): string {
  let current = typeName.trim();
  while (true) {
    const intentType = parseSophiaIntentType(current);
    if (intentType) {
      current = intentType.innerType;
      continue;
    }
    const optionalType = parseSophiaOptionalType(current);
    if (optionalType) {
      current = optionalType.innerType;
      continue;
    }
    return current;
  }
}

export function wrapSophiaIntentType(intent: SophiaIntentType, innerType: string): string {
  return `${intent}<${innerType}>`;
}

export function isSupportedSophiaType(
  typeName: string,
  entityTypes: SophiaEntityTypes,
  stateTypes: SophiaStateTypes = new Map(),
): boolean {
  if (isSophiaV0Type(typeName) || entityTypes.has(typeName) || stateTypes.has(typeName))
    return true;
  const intentType = parseSophiaIntentType(typeName);
  if (intentType) return isSupportedSophiaType(intentType.innerType, entityTypes, stateTypes);
  const optionalType = parseSophiaOptionalType(typeName);
  if (optionalType) return isSupportedSophiaType(optionalType.innerType, entityTypes, stateTypes);
  return false;
}

export function isSophiaTypeAssignable(actualType: string, expectedType: string): boolean {
  if (actualType === "None" && parseSophiaOptionalType(expectedType)) return true;
  return actualType === expectedType;
}

export function sophiaTypeToTypeScript(typeName: string): string {
  const unwrappedType = unwrapSophiaIntentType(typeName);
  if (unwrappedType !== typeName) return sophiaTypeToTypeScript(unwrappedType);
  const optionalType = parseSophiaOptionalType(typeName);
  if (optionalType) return `${sophiaTypeToTypeScript(optionalType.innerType)} | null`;
  if (typeName === "Int") return "number";
  if (typeName === "Bool") return "boolean";
  if (typeName === "Text") return "string";
  if (typeName === "Unit") return "Unit";
  if (typeName === "List<Int>") return "number[]";
  if (typeName === "List<Text>") return "string[]";
  if (/^[A-Z][A-Za-z0-9]*$/.test(typeName)) return typeName;
  if (typeName === "UnknownList") return "unknown[]";
  return "unknown";
}

export function matchesSophiaRuntimeType(
  value: unknown,
  typeName: string,
  entityTypes: SophiaEntityTypes = new Map(),
  stateTypes: SophiaStateTypes = new Map(),
): boolean {
  const unwrappedType = unwrapSophiaIntentType(typeName);
  if (unwrappedType !== typeName)
    return matchesSophiaRuntimeType(value, unwrappedType, entityTypes, stateTypes);
  const optionalType = parseSophiaOptionalType(typeName);
  if (optionalType) {
    return (
      value === null ||
      matchesSophiaRuntimeType(value, optionalType.innerType, entityTypes, stateTypes)
    );
  }
  if (typeName === "Unit") return value === null;
  if (typeName === "Bool") return typeof value === "boolean";
  if (typeName === "Int") return typeof value === "number" && Number.isInteger(value);
  if (typeName === "Text") return typeof value === "string";
  if (typeName === "List<Int>") {
    return (
      Array.isArray(value) &&
      value.every((item) => typeof item === "number" && Number.isInteger(item))
    );
  }
  if (typeName === "List<Text>") {
    return Array.isArray(value) && value.every((item) => typeof item === "string");
  }
  const entityFields = entityTypes.get(typeName);
  if (entityFields) {
    if (!isRecord(value)) return false;
    const allowed = new Set(entityFields.map((field) => field.name));
    if (Object.keys(value).some((key) => !allowed.has(key))) return false;
    return entityFields.every(
      (field) =>
        field.name in value &&
        matchesSophiaRuntimeType(value[field.name], field.type, entityTypes, stateTypes),
    );
  }
  const stateValues = stateTypes.get(typeName);
  if (stateValues) return typeof value === "string" && stateValues.includes(value);
  return false;
}

export function sampleSophiaRuntimeValue(
  typeName: string,
  entityTypes: SophiaEntityTypes = new Map(),
  stateTypes: SophiaStateTypes = new Map(),
): unknown {
  const unwrappedType = unwrapSophiaIntentType(typeName);
  if (unwrappedType !== typeName)
    return sampleSophiaRuntimeValue(unwrappedType, entityTypes, stateTypes);
  const optionalType = parseSophiaOptionalType(typeName);
  if (optionalType) return null;
  if (typeName === "Unit") return null;
  if (typeName === "Bool") return false;
  if (typeName === "Int") return 0;
  if (typeName === "Text") return "";
  if (typeName === "List<Int>") return [];
  if (typeName === "List<Text>") return [];
  const entityFields = entityTypes.get(typeName);
  if (entityFields) {
    const sample: Record<string, unknown> = {};
    for (const field of entityFields) {
      const value = sampleSophiaRuntimeValue(field.type, entityTypes, stateTypes);
      if (value === undefined) return undefined;
      sample[field.name] = value;
    }
    return sample;
  }
  const stateValues = stateTypes.get(typeName);
  if (stateValues) return stateValues[0];
  return undefined;
}
