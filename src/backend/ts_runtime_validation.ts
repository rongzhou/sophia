import { isRecord } from "../util/json.js";
import { tsBuildDiagnostic, type TypeScriptBuildDiagnostic } from "./ts_codegen.js";
import {
  type GeneratedActionMetadata,
  type GeneratedFieldMetadata,
} from "./ts_generated_module.js";
import { matchesSophiaRuntimeType, sampleSophiaRuntimeValue } from "../lang/types.js";

export function buildSampleInput(
  fields: GeneratedFieldMetadata[],
  entityTypes: Map<string, GeneratedFieldMetadata[]>,
  stateTypes: Map<string, string[]> = new Map(),
): Record<string, unknown> | null {
  const input: Record<string, unknown> = {};
  for (const field of fields) {
    const value = sampleSophiaRuntimeValue(field.type, entityTypes, stateTypes);
    if (value === undefined) return null;
    input[field.name] = value;
  }
  return input;
}

export function validateInput(
  metadata: GeneratedActionMetadata,
  input: unknown,
  sourcePath: string,
  entityTypes: Map<string, GeneratedFieldMetadata[]>,
  stateTypes: Map<string, string[]> = new Map(),
): TypeScriptBuildDiagnostic | null {
  if (!isRecord(input)) {
    return tsBuildDiagnostic(
      "RUN-INPUT-001",
      sourcePath,
      `Input for ${metadata.name} must be a JSON object.`,
    );
  }
  const allowedKeys = new Set(metadata.input.map((field) => field.name));
  for (const key of Object.keys(input)) {
    if (!allowedKeys.has(key)) {
      return tsBuildDiagnostic(
        "RUN-INPUT-002",
        sourcePath,
        `Input for ${metadata.name} contains unknown field ${key}.`,
      );
    }
  }
  for (const field of metadata.input) {
    if (!(field.name in input)) {
      return tsBuildDiagnostic(
        "RUN-INPUT-003",
        sourcePath,
        `Input for ${metadata.name} is missing required field ${field.name}.`,
      );
    }
    if (!matchesSophiaRuntimeType(input[field.name], field.type, entityTypes, stateTypes)) {
      return tsBuildDiagnostic(
        "RUN-INPUT-004",
        sourcePath,
        `Input field ${field.name} for ${metadata.name} must be ${field.type}.`,
      );
    }
  }
  return null;
}

export function validateOutput(
  metadata: GeneratedActionMetadata,
  result: unknown,
  sourcePath: string,
  entityTypes: Map<string, GeneratedFieldMetadata[]>,
  stateTypes: Map<string, string[]> = new Map(),
): TypeScriptBuildDiagnostic | null {
  const outputType = metadata.output[0]?.type ?? "Unit";
  if (!matchesSophiaRuntimeType(result, outputType, entityTypes, stateTypes)) {
    return tsBuildDiagnostic(
      "RUN-OUTPUT-001",
      sourcePath,
      `Result for ${metadata.name} must be ${outputType}.`,
    );
  }
  return null;
}
