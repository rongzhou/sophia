import type { Diagnostic } from "../ast/diagnostics.js";
import {
  parseActionSignatures,
  parseCapabilities,
  parseEntities,
  parseErrors,
  parseStates,
  parseStorages,
  type SophiaFileSet,
} from "../ast/check_model.js";
import type { SophiaEntityTypes } from "../ast/types.js";

export interface CheckerContext {
  diagnostics: Diagnostic[];
  capabilities: ReturnType<typeof parseCapabilities>;
  entityTypes: SophiaEntityTypes;
  stateTypes: ReturnType<typeof parseStates>;
  storageTypes: ReturnType<typeof parseStorages>;
  errorVariants: ReturnType<typeof parseErrors>;
  actionTypes: ReturnType<typeof parseActionSignatures>;
  actionNames: Set<string>;
  capabilityNames: Set<string>;
  entityNames: Set<string>;
  stateNames: Set<string>;
  errorNames: Set<string>;
  errorVariantNames: Set<string>;
  topLevelNames: Map<string, { kind: string; path: string }>;
}

export function createCheckerContext(files: SophiaFileSet): CheckerContext {
  return {
    diagnostics: [],
    capabilities: parseCapabilities(files),
    entityTypes: parseEntities(files),
    stateTypes: parseStates(files),
    storageTypes: parseStorages(files),
    errorVariants: parseErrors(files),
    actionTypes: parseActionSignatures(files),
    actionNames: new Set(),
    capabilityNames: new Set(),
    entityNames: new Set(),
    stateNames: new Set(),
    errorNames: new Set(),
    errorVariantNames: new Set(),
    topLevelNames: new Map(),
  };
}
