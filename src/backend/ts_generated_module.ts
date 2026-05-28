import { readFile } from "node:fs/promises";
import path from "node:path";
import ts from "typescript";
import { errorDiagnostic, type Diagnostic } from "../lang/ast/diagnostics.js";
import { isRecord } from "../util/json.js";
import { buildTypeScript } from "./ts_codegen.js";

export interface GeneratedFieldMetadata {
  name: string;
  type: string;
}

export interface GeneratedActionMetadata {
  name: string;
  path: string;
  input: GeneratedFieldMetadata[];
  output: GeneratedFieldMetadata[];
  effects: string[];
  errors?: string[];
}

export interface GeneratedEntityMetadata {
  name: string;
  path: string;
  fields: GeneratedFieldMetadata[];
}

export interface GeneratedStateMetadata {
  name: string;
  path: string;
  values: string[];
}

export type LoadedGeneratedModule =
  | {
      ok: true;
      sourcePath: string;
      module: Record<string, unknown>;
      diagnostics: [];
    }
  | {
      ok: false;
      sourcePath: string | null;
      module: null;
      diagnostics: Diagnostic[];
    };

export async function loadGeneratedTypeScriptModule(root: string): Promise<LoadedGeneratedModule> {
  const build = await buildTypeScript(root);
  if (!build.ok) {
    return { ok: false, sourcePath: null, module: null, diagnostics: build.diagnostics };
  }

  const sourcePath = build.files[0];
  if (!sourcePath) {
    return {
      ok: false,
      sourcePath: null,
      module: null,
      diagnostics: [errorDiagnostic("RUN-BUILD-001", "<build>", "Build produced no entry file.")],
    };
  }

  const source = await readFile(path.join(root, sourcePath), "utf8");
  const transpiled = ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ES2022,
      moduleResolution: ts.ModuleResolutionKind.Bundler,
      strict: true,
    },
  });
  const transpileErrors =
    transpiled.diagnostics?.filter(
      (diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error,
    ) ?? [];
  if (transpileErrors.length > 0) {
    return {
      ok: false,
      sourcePath,
      module: null,
      diagnostics: transpileErrors.map((diagnostic) =>
        errorDiagnostic(
          "RUN-TRANSPILE-001",
          sourcePath,
          ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
        ),
      ),
    };
  }
  const moduleUrl = `data:text/javascript;base64,${Buffer.from(transpiled.outputText, "utf8").toString("base64")}`;
  return {
    ok: true,
    sourcePath,
    module: (await import(moduleUrl)) as Record<string, unknown>,
    diagnostics: [],
  };
}

export function readGeneratedActionMetadataList(value: unknown): GeneratedActionMetadata[] {
  if (!Array.isArray(value)) return [];
  return value.filter(isGeneratedActionMetadata);
}

export function readGeneratedEntityTypes(value: unknown): Map<string, GeneratedFieldMetadata[]> {
  if (!Array.isArray(value)) return new Map();
  return new Map(
    value.filter(isGeneratedEntityMetadata).map((entity) => [entity.name, entity.fields] as const),
  );
}

export function readGeneratedStateTypes(value: unknown): Map<string, string[]> {
  if (!Array.isArray(value)) return new Map();
  return new Map(
    value.filter(isGeneratedStateMetadata).map((state) => [state.name, state.values] as const),
  );
}

export function findGeneratedActionMetadata(
  value: unknown,
  action: string,
): GeneratedActionMetadata | null {
  return readGeneratedActionMetadataList(value).find((item) => item.name === action) ?? null;
}

function isGeneratedActionMetadata(value: unknown): value is GeneratedActionMetadata {
  if (!isRecord(value)) return false;
  return (
    typeof value.name === "string" &&
    typeof value.path === "string" &&
    Array.isArray(value.input) &&
    value.input.every(isGeneratedFieldMetadata) &&
    Array.isArray(value.output) &&
    value.output.every(isGeneratedFieldMetadata) &&
    Array.isArray(value.effects) &&
    value.effects.every((effect) => typeof effect === "string")
  );
}

function isGeneratedEntityMetadata(value: unknown): value is GeneratedEntityMetadata {
  return (
    isRecord(value) &&
    typeof value.name === "string" &&
    typeof value.path === "string" &&
    Array.isArray(value.fields) &&
    value.fields.every(isGeneratedFieldMetadata)
  );
}

function isGeneratedStateMetadata(value: unknown): value is GeneratedStateMetadata {
  return (
    isRecord(value) &&
    typeof value.name === "string" &&
    typeof value.path === "string" &&
    Array.isArray(value.values) &&
    value.values.every((item) => typeof item === "string")
  );
}

function isGeneratedFieldMetadata(value: unknown): value is GeneratedFieldMetadata {
  return isRecord(value) && typeof value.name === "string" && typeof value.type === "string";
}
