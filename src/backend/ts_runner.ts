import { errorDiagnostic, type Diagnostic } from "../lang/ast/diagnostics.js";
import {
  findGeneratedActionMetadata,
  loadGeneratedTypeScriptModule,
  readGeneratedActionMetadataList,
  readGeneratedEntityTypes,
  readGeneratedStateTypes,
  type GeneratedActionMetadata,
  type GeneratedFieldMetadata,
} from "./ts_generated_module.js";
import { buildSampleInput, validateInput, validateOutput } from "./ts_runtime_validation.js";

export interface TypeScriptRunResult {
  ok: boolean;
  action: string;
  input: unknown;
  result: unknown;
  effects: string[];
  diagnostics: Diagnostic[];
}

export interface TypeScriptSmokeActionResult {
  ok: boolean;
  action: string;
  skipped: boolean;
  reason: string | null;
  result: unknown;
  effects: string[];
  diagnostics: Diagnostic[];
}

export interface TypeScriptSmokeResult {
  ok: boolean;
  actions_run: number;
  actions_skipped: number;
  actions: TypeScriptSmokeActionResult[];
  diagnostics: Diagnostic[];
}

export interface TypeScriptSmokeOptions {
  inputs?: Record<string, unknown>;
  autoInputs?: boolean;
}

export async function runTypeScriptAction(
  root: string,
  action: string,
  input: unknown,
): Promise<TypeScriptRunResult> {
  const loaded = await loadGeneratedTypeScriptModule(root);
  if (!loaded.ok) {
    return {
      ok: false,
      action,
      input,
      result: null,
      effects: [],
      diagnostics: loaded.diagnostics,
    };
  }

  const entityTypes = readGeneratedEntityTypes(loaded.module.entities);
  const stateTypes = readGeneratedStateTypes(loaded.module.states);
  const actionMetadata = findGeneratedActionMetadata(loaded.module.actions, action);
  if (!actionMetadata) {
    return failedRun(
      action,
      input,
      "RUN-ACTION-001",
      loaded.sourcePath,
      `Generated build does not export action metadata for ${action}.`,
    );
  }
  const exportedAction = loaded.module[action];
  if (typeof exportedAction !== "function") {
    return failedRun(
      action,
      input,
      "RUN-ACTION-001",
      loaded.sourcePath,
      `Generated build does not export action ${action}.`,
    );
  }
  const inputDiagnostic = validateInput(
    actionMetadata,
    input,
    loaded.sourcePath,
    entityTypes,
    stateTypes,
  );
  if (inputDiagnostic) {
    return {
      ok: false,
      action,
      input,
      result: null,
      effects: [],
      diagnostics: [inputDiagnostic],
    };
  }

  return executeGeneratedAction({
    metadata: actionMetadata,
    exportedAction: exportedAction as GeneratedAction,
    input,
    sourcePath: loaded.sourcePath,
    entityTypes,
    stateTypes,
  });
}

export async function smokeTypeScriptActions(
  root: string,
  options: TypeScriptSmokeOptions = {},
): Promise<TypeScriptSmokeResult> {
  const loaded = await loadGeneratedTypeScriptModule(root);
  if (!loaded.ok) {
    return {
      ok: false,
      actions_run: 0,
      actions_skipped: 0,
      actions: [],
      diagnostics: loaded.diagnostics,
    };
  }

  const actions = readGeneratedActionMetadataList(loaded.module.actions);
  const entityTypes = readGeneratedEntityTypes(loaded.module.entities);
  const stateTypes = readGeneratedStateTypes(loaded.module.states);
  const results: TypeScriptSmokeActionResult[] = [];
  for (const action of actions) {
    const exportedAction = loaded.module[action.name];
    const hasSampleInput = Object.hasOwn(options.inputs ?? {}, action.name);
    const generatedInput = options.autoInputs
      ? buildSampleInput(action.input, entityTypes, stateTypes)
      : null;
    if (action.input.length > 0 && !hasSampleInput && !generatedInput) {
      results.push({
        ok: true,
        action: action.name,
        skipped: true,
        reason: "requires_input",
        result: null,
        effects: [],
        diagnostics: [],
      });
      continue;
    }
    if (typeof exportedAction !== "function") {
      results.push({
        ok: false,
        action: action.name,
        skipped: false,
        reason: null,
        result: null,
        effects: [],
        diagnostics: [
          errorDiagnostic(
            "RUN-ACTION-001",
            loaded.sourcePath,
            `Generated build does not export action ${action.name}.`,
          ),
        ],
      });
      continue;
    }
    const input = hasSampleInput ? options.inputs?.[action.name] : (generatedInput ?? {});
    const inputDiagnostic = validateInput(
      action,
      input,
      loaded.sourcePath,
      entityTypes,
      stateTypes,
    );
    if (inputDiagnostic) {
      results.push({
        ok: false,
        action: action.name,
        skipped: false,
        reason: null,
        result: null,
        effects: [],
        diagnostics: [inputDiagnostic],
      });
      continue;
    }
    results.push(
      toSmokeActionResult(
        executeGeneratedAction({
          metadata: action,
          exportedAction: exportedAction as GeneratedAction,
          input,
          sourcePath: loaded.sourcePath,
          entityTypes,
          stateTypes,
        }),
      ),
    );
  }

  const diagnostics = results.flatMap((result) => result.diagnostics);
  return {
    ok: diagnostics.length === 0,
    actions_run: results.filter((result) => !result.skipped).length,
    actions_skipped: results.filter((result) => result.skipped).length,
    actions: results,
    diagnostics,
  };
}

type GeneratedAction = (
  input: unknown,
  effects: {
    write(value: string): void;
  },
) => unknown;

interface ExecuteGeneratedActionOptions {
  metadata: GeneratedActionMetadata;
  exportedAction: GeneratedAction;
  input: unknown;
  sourcePath: string;
  entityTypes: Map<string, GeneratedFieldMetadata[]>;
  stateTypes: Map<string, string[]>;
}

function executeGeneratedAction(options: ExecuteGeneratedActionOptions): TypeScriptRunResult {
  const effects: string[] = [];
  try {
    const rawResult = options.exportedAction(options.input, {
      write(value: string): void {
        effects.push(value);
      },
    });
    const result = rawResult === undefined ? null : rawResult;
    const outputDiagnostic = validateOutput(
      options.metadata,
      result,
      options.sourcePath,
      options.entityTypes,
      options.stateTypes,
    );
    return {
      ok: !outputDiagnostic,
      action: options.metadata.name,
      input: options.input,
      result,
      effects,
      diagnostics: outputDiagnostic ? [outputDiagnostic] : [],
    };
  } catch (error) {
    return failedRun(
      options.metadata.name,
      options.input,
      "RUN-EXEC-001",
      options.sourcePath,
      error instanceof Error ? error.message : String(error),
      effects,
    );
  }
}

function toSmokeActionResult(result: TypeScriptRunResult): TypeScriptSmokeActionResult {
  return {
    ok: result.ok,
    action: result.action,
    skipped: false,
    reason: null,
    result: result.result,
    effects: result.effects,
    diagnostics: result.diagnostics,
  };
}

function failedRun(
  action: string,
  input: unknown,
  code: string,
  diagnosticLocation: string,
  problem: string,
  effects: string[] = [],
): TypeScriptRunResult {
  return {
    ok: false,
    action,
    input,
    result: null,
    effects,
    diagnostics: [errorDiagnostic(code, diagnosticLocation, problem)],
  };
}
