import { stripQuotedText } from "../lang/ast/braces.js";
import {
  parseActions,
  parseCapabilities,
  parseEntities,
  parseErrors,
  parseStates,
  parseStorages,
  type ParsedAction,
  type SophiaFileSet,
} from "../lang/ast/check_model.js";
import { parseSophiaTopLevelDeclarations } from "../lang/ast/parser.js";
import { parseSophiaStorageEffect } from "../lang/ast/signature.js";
import { errorDiagnostic, type Diagnostic } from "../lang/ast/diagnostics.js";
import { unwrapSophiaContextType } from "../lang/ast/types.js";
import { isSophiaNodeDirectory } from "../workspace/sophia_paths.js";

export interface SophiaContextNode {
  name: string;
  kind: "Domain" | "Entity" | "Action" | "Capability" | "Storage" | "Error" | "State";
  domain: string;
  path: string;
}

export interface SophiaContextEdge {
  from: string;
  relation:
    | "allows_effect"
    | "binds_capability"
    | "calls"
    | "declares_effect"
    | "denies_effect"
    | "raises"
    | "reads"
    | "uses_type"
    | "writes";
  to: string;
  to_kind: "Action" | "Capability" | "Effect" | "Entity" | "Error" | "State" | "Storage";
  detail?: string;
}

export interface SophiaContextSource {
  path: string;
  content: string;
}

export interface SophiaActionContextResult {
  ok: boolean;
  root: {
    kind: "Action";
    name: string;
  };
  files: string[];
  nodes: SophiaContextNode[];
  edges: SophiaContextEdge[];
  sources: SophiaContextSource[];
  summary: SophiaContextSummary;
  diagnostics: Diagnostic[];
}

export interface SophiaContextSummary {
  domains: string[];
  actions: string[];
  capabilities: string[];
  entities: string[];
  states: string[];
  errors: string[];
  storages: string[];
}

interface NodeIndexEntry {
  kind: SophiaContextNode["kind"];
  domain: string;
  path: string;
}

interface ActionEntry {
  path: string;
  action: ParsedAction;
}

export function buildActionContext(
  files: SophiaFileSet,
  actionName: string,
): SophiaActionContextResult {
  const diagnostics: Diagnostic[] = [];
  const nodeIndex = buildNodeIndex(files, diagnostics);
  const actions = buildActionMap(files, diagnostics);
  const capabilities = parseCapabilities(files);
  const entityTypes = parseEntities(files);
  const stateTypes = parseStates(files);
  const storageTypes = parseStorages(files);
  const errorVariants = parseErrors(files);
  const included = new Set<string>();
  const visitedActions = new Set<string>();
  const edges = new Map<string, SophiaContextEdge>();

  includeAction(actionName);

  for (const path of [...included]) {
    const domainPath = domainFilePath(path);
    if (domainPath && files[domainPath]) included.add(domainPath);
  }

  const sortedFiles = [...included].sort();
  return {
    ok: diagnostics.every((diagnostic) => diagnostic.severity !== "error"),
    root: {
      kind: "Action",
      name: actionName,
    },
    files: sortedFiles,
    nodes: sortedFiles
      .map((path) => nodeFromPath(path, nodeIndex))
      .filter((node): node is SophiaContextNode => node !== null)
      .sort(
        (left, right) => left.kind.localeCompare(right.kind) || left.name.localeCompare(right.name),
      ),
    edges: [...edges.values()].sort(
      (left, right) =>
        left.from.localeCompare(right.from) ||
        left.relation.localeCompare(right.relation) ||
        left.to_kind.localeCompare(right.to_kind) ||
        left.to.localeCompare(right.to) ||
        (left.detail ?? "").localeCompare(right.detail ?? ""),
    ),
    sources: sortedFiles.map((path) => ({
      path,
      content: files[path] ?? "",
    })),
    summary: buildSummary(sortedFiles, nodeIndex),
    diagnostics,
  };

  function includeAction(name: string): void {
    const entry = actions.get(name);
    if (!entry) {
      diagnostics.push(errorDiagnostic("CONTEXT-ACTION-001", "<context>", `Unknown action: ${name}.`));
      return;
    }
    if (visitedActions.has(name)) return;
    visitedActions.add(name);
    included.add(entry.path);

    if (entry.action.capability) {
      includeNode(entry.action.capability, "Capability", entry.path);
      addEdge({
        from: name,
        relation: "binds_capability",
        to: entry.action.capability,
        to_kind: "Capability",
      });
      includeCapabilityPolicy(entry.action.capability, entry.path);
    }
    for (const field of [...entry.action.inputFields, ...entry.action.outputFields]) {
      includeType(name, field.type, entry.path, field.name);
    }
    for (const variantName of entry.action.errors) {
      const variant = errorVariants.get(variantName);
      if (!variant) {
        diagnostics.push(
          errorDiagnostic(
            "CONTEXT-ERROR-001",
            entry.path,
            `Action ${name} declares unknown error variant: ${variantName}.`,
          ),
        );
        continue;
      }
      includeNode(variant.errorName, "Error", entry.path);
      addEdge({
        from: name,
        relation: "raises",
        to: variant.errorName,
        to_kind: "Error",
        detail: variantName,
      });
      for (const field of variant.fields)
        includeType(variant.errorName, field.type, entry.path, field.name);
    }
    for (const effect of entry.action.effects) {
      addEdge({
        from: name,
        relation: "declares_effect",
        to: effect,
        to_kind: "Effect",
      });
      const storageEffect = parseSophiaStorageEffect(effect);
      if (storageEffect) {
        includeNode(storageEffect.storage, "Storage", entry.path);
        addEdge({
          from: name,
          relation: storageEffect.mode === "Read" ? "reads" : "writes",
          to: storageEffect.storage,
          to_kind: "Storage",
          detail: effect,
        });
        const storage = storageTypes.get(storageEffect.storage);
        if (storage) {
          includeType(storageEffect.storage, storage.keyType, entry.path, "key");
          includeType(storageEffect.storage, storage.valueType, entry.path, "value");
        }
        continue;
      }
    }
    for (const typeName of collectStateValueTypes(entry.action.body))
      includeType(name, typeName, entry.path, "body");
    for (const typeName of collectConstructedTypes(entry.action.body))
      includeType(name, typeName, entry.path, "body");
    for (const calledAction of collectActionCalls(entry.action.body)) {
      addEdge({
        from: name,
        relation: "calls",
        to: calledAction,
        to_kind: "Action",
      });
      includeAction(calledAction);
    }
  }

  function includeType(
    sourceName: string,
    typeName: string,
    sourcePath: string,
    detail: string,
  ): void {
    const unwrapped = unwrapSophiaContextType(typeName);
    if (entityTypes.has(unwrapped)) {
      includeNode(unwrapped, "Entity", sourcePath);
      addEdge({
        from: sourceName,
        relation: "uses_type",
        to: unwrapped,
        to_kind: "Entity",
        detail,
      });
      for (const field of entityTypes.get(unwrapped) ?? [])
        includeType(unwrapped, field.type, sourcePath, field.name);
      return;
    }
    if (stateTypes.has(unwrapped)) {
      includeNode(unwrapped, "State", sourcePath);
      addEdge({
        from: sourceName,
        relation: "uses_type",
        to: unwrapped,
        to_kind: "State",
        detail,
      });
    }
  }

  function includeNode(
    name: string,
    expectedKind: SophiaContextNode["kind"],
    sourcePath: string,
  ): void {
    const node = nodeIndex.get(name);
    if (!node) {
      diagnostics.push(
        errorDiagnostic(
          "CONTEXT-NODE-001",
          sourcePath,
          `Referenced ${expectedKind.toLowerCase()} node does not exist: ${name}.`,
        ),
      );
      return;
    }
    if (node.kind !== expectedKind) {
      diagnostics.push(
        errorDiagnostic(
          "CONTEXT-NODE-002",
          sourcePath,
          `Referenced node ${name} is ${node.kind}, expected ${expectedKind}.`,
        ),
      );
      return;
    }
    included.add(node.path);
  }

  function includeCapabilityPolicy(capabilityName: string, sourcePath: string): void {
    const policy = capabilities.get(capabilityName);
    if (!policy) return;
    for (const effect of [...policy.allow].sort()) {
      includeCapabilityEffect(capabilityName, effect, "allows_effect", sourcePath);
    }
    for (const effect of [...policy.deny].sort()) {
      includeCapabilityEffect(capabilityName, effect, "denies_effect", sourcePath);
    }
  }

  function includeCapabilityEffect(
    capabilityName: string,
    effect: string,
    relation: "allows_effect" | "denies_effect",
    sourcePath: string,
  ): void {
    addEdge({
      from: capabilityName,
      relation,
      to: effect,
      to_kind: "Effect",
    });
    const storageEffect = parseSophiaStorageEffect(effect);
    if (!storageEffect) return;
    includeNode(storageEffect.storage, "Storage", sourcePath);
    addEdge({
      from: capabilityName,
      relation,
      to: storageEffect.storage,
      to_kind: "Storage",
      detail: effect,
    });
    const storage = storageTypes.get(storageEffect.storage);
    if (!storage) return;
    includeType(storageEffect.storage, storage.keyType, sourcePath, "key");
    includeType(storageEffect.storage, storage.valueType, sourcePath, "value");
  }

  function addEdge(edge: SophiaContextEdge): void {
    const key = [edge.from, edge.relation, edge.to_kind, edge.to, edge.detail ?? ""].join("\0");
    edges.set(key, edge);
  }

  function collectActionCalls(body: string): string[] {
    const calls: string[] = [];
    for (const match of stripQuotedText(body).matchAll(/\b([A-Z][A-Za-z0-9]*)\s*\{/g)) {
      const name = match[1];
      if (!name || entityTypes.has(name) || !actions.has(name)) continue;
      calls.push(name);
    }
    return calls;
  }
}

function buildActionMap(
  files: SophiaFileSet,
  diagnostics: Diagnostic[],
): Map<string, ActionEntry> {
  const actions = new Map<string, ActionEntry>();
  for (const [path, content] of Object.entries(files).sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    if (!path.endsWith(".sophia")) continue;
    for (const action of parseActions(content)) {
      if (actions.has(action.name)) {
        diagnostics.push(
          errorDiagnostic("CONTEXT-ACTION-002", path, `Duplicate action declaration: ${action.name}.`),
        );
        continue;
      }
      actions.set(action.name, { path, action });
    }
  }
  return actions;
}

function buildNodeIndex(
  files: SophiaFileSet,
  diagnostics: Diagnostic[],
): Map<string, NodeIndexEntry> {
  const nodes = new Map<string, NodeIndexEntry>();
  for (const [path, content] of Object.entries(files).sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    if (!path.endsWith(".sophia")) continue;
    const declaration = parseSophiaTopLevelDeclarations(content)[0];
    if (!declaration) continue;
    const kind = contextKindFromSophiaKind(declaration.kind);
    if (!kind) continue;
    if (nodes.has(declaration.name)) {
      diagnostics.push(
        errorDiagnostic("CONTEXT-NODE-003", path, `Duplicate top-level node name: ${declaration.name}.`),
      );
      continue;
    }
    nodes.set(declaration.name, {
      kind,
      domain: domainFromPath(path),
      path,
    });
  }
  return nodes;
}

function nodeFromPath(
  path: string,
  nodeIndex: Map<string, NodeIndexEntry>,
): SophiaContextNode | null {
  for (const [name, node] of nodeIndex) {
    if (node.path === path) return { name, ...node };
  }
  return null;
}

function buildSummary(
  paths: string[],
  nodeIndex: Map<string, NodeIndexEntry>,
): SophiaContextSummary {
  const summary: SophiaContextSummary = {
    domains: [],
    actions: [],
    capabilities: [],
    entities: [],
    states: [],
    errors: [],
    storages: [],
  };
  for (const path of paths) {
    const node = nodeFromPath(path, nodeIndex);
    if (!node) continue;
    if (node.kind === "Domain") summary.domains.push(node.name);
    if (node.kind === "Action") summary.actions.push(node.name);
    if (node.kind === "Capability") summary.capabilities.push(node.name);
    if (node.kind === "Entity") summary.entities.push(node.name);
    if (node.kind === "State") summary.states.push(node.name);
    if (node.kind === "Error") summary.errors.push(node.name);
    if (node.kind === "Storage") summary.storages.push(node.name);
  }
  for (const values of Object.values(summary)) values.sort();
  return summary;
}

function contextKindFromSophiaKind(kind: string): SophiaContextNode["kind"] | null {
  if (kind === "domain") return "Domain";
  if (kind === "entity") return "Entity";
  if (kind === "action") return "Action";
  if (kind === "capability") return "Capability";
  if (kind === "storage") return "Storage";
  if (kind === "error") return "Error";
  if (kind === "state") return "State";
  return null;
}

function collectStateValueTypes(body: string): string[] {
  return [...stripQuotedText(body).matchAll(/\b([A-Z][A-Za-z0-9]*)\.[A-Z][A-Za-z0-9]*\b/g)]
    .map((match) => match[1])
    .filter((name): name is string => Boolean(name));
}

function collectConstructedTypes(body: string): string[] {
  return [...stripQuotedText(body).matchAll(/\b([A-Z][A-Za-z0-9]*)\s*\{/g)]
    .map((match) => match[1])
    .filter((name): name is string => Boolean(name));
}

function domainFromPath(filePath: string): string {
  const parts = filePath.split("/");
  const kindIndex = parts.findIndex((part) => isSophiaNodeDirectory(part));
  if (kindIndex > 0) return parts[kindIndex - 1] ?? "";
  const domainFileIndex = parts.findIndex((part) => part === "domain.sophia");
  if (domainFileIndex > 0) return parts[domainFileIndex - 1] ?? "";
  return "";
}

function domainFilePath(filePath: string): string | null {
  const parts = filePath.split("/");
  const kindIndex = parts.findIndex((part) => isSophiaNodeDirectory(part));
  if (kindIndex <= 0) return null;
  return `${parts.slice(0, kindIndex).join("/")}/domain.sophia`;
}
