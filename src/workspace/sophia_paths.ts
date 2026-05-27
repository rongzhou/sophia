import { escapeRegExp } from "../util/strings.js";

export type SophiaTopLevelKind =
  | "domain"
  | "entity"
  | "action"
  | "capability"
  | "storage"
  | "state"
  | "error";

export const SOPHIA_NODE_DIRECTORIES = [
  "actions",
  "capabilities",
  "entities",
  "errors",
  "states",
  "storages",
] as const;

export type SophiaNodeDirectory = (typeof SOPHIA_NODE_DIRECTORIES)[number];

const NODE_DIRECTORY_SET = new Set<string>(SOPHIA_NODE_DIRECTORIES);

export function isSophiaNodeDirectory(segment: string): segment is SophiaNodeDirectory {
  return NODE_DIRECTORY_SET.has(segment);
}

export function isSupportedSophiaFilePath(filePath: string, domainRoot = "domains"): boolean {
  return expectedTopLevelKindForPath(filePath, domainRoot) !== null;
}

export function expectedTopLevelKindForPath(
  filePath: string,
  domainRoot = "domains",
): SophiaTopLevelKind | null {
  return expectedTopLevelPathInfo(filePath, domainRoot)?.kind ?? null;
}

export interface SophiaTopLevelPathInfo {
  kind: SophiaTopLevelKind;
  name: string;
}

export function expectedTopLevelPathInfo(
  filePath: string,
  domainRoot = "domains",
): SophiaTopLevelPathInfo | null {
  const root = escapeRegExp(domainRoot.replace(/\\/g, "/").replace(/\/+$/, ""));
  const domainMatch = new RegExp(`^${root}/(${PASCAL_SEGMENT})/domain\\.sophia$`).exec(filePath);
  if (domainMatch?.[1]) return { kind: "domain", name: domainMatch[1] };

  const entityMatch = new RegExp(
    `^${root}/${PASCAL_SEGMENT}/entities/(${PASCAL_SEGMENT})\\.sophia$`,
  ).exec(filePath);
  if (entityMatch?.[1]) return { kind: "entity", name: entityMatch[1] };

  const actionMatch = new RegExp(
    `^${root}/${PASCAL_SEGMENT}/actions/(${PASCAL_SEGMENT})\\.sophia$`,
  ).exec(filePath);
  if (actionMatch?.[1]) return { kind: "action", name: actionMatch[1] };

  const capabilityMatch = new RegExp(
    `^${root}/${PASCAL_SEGMENT}/capabilities/(${PASCAL_SEGMENT})\\.sophia$`,
  ).exec(filePath);
  if (capabilityMatch?.[1]) return { kind: "capability", name: capabilityMatch[1] };

  const storageMatch = new RegExp(
    `^${root}/${PASCAL_SEGMENT}/storages/(${PASCAL_SEGMENT})\\.sophia$`,
  ).exec(filePath);
  if (storageMatch?.[1]) return { kind: "storage", name: storageMatch[1] };

  const errorMatch = new RegExp(
    `^${root}/${PASCAL_SEGMENT}/errors/(${PASCAL_SEGMENT})\\.sophia$`,
  ).exec(filePath);
  if (errorMatch?.[1]) return { kind: "error", name: errorMatch[1] };

  const stateMatch = new RegExp(
    `^${root}/${PASCAL_SEGMENT}/states/(${PASCAL_SEGMENT})\\.sophia$`,
  ).exec(filePath);
  if (stateMatch?.[1]) return { kind: "state", name: stateMatch[1] };

  return null;
}

export function isPascalCaseSophiaName(value: string): boolean {
  return new RegExp(`^${PASCAL_SEGMENT}$`).test(value);
}

export function isSafeRelativeArtifactPath(filePath: string): boolean {
  return !filePath.includes("..") && !filePath.startsWith("/") && !filePath.includes("\\");
}

const PASCAL_SEGMENT = "[A-Z][A-Za-z0-9]*";
