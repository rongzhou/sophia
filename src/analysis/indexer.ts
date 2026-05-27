import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  expectedTopLevelKindForPath,
  isSupportedSophiaFilePath,
} from "../workspace/sophia_paths.js";
import { error, type Diagnostic } from "../lang/diagnostics.js";
import { parseSophiaSource } from "../lang/parser.js";
import { loadWorkspaceConfig } from "../workspace/workspace.js";
import { collectSophiaFiles } from "../util/fs.js";
import { stableJson } from "../util/json.js";
import { normalizeRelativePath } from "../util/strings.js";

export interface AsgIndexNode {
  kind: "Domain" | "Entity" | "Action" | "Capability" | "Storage" | "Error" | "State";
  domain: string;
  path: string;
}

export interface AsgIndex {
  version: 1;
  nodes: Record<string, AsgIndexNode>;
}

export type AsgIndexDiagnostic = Diagnostic;

export interface BuildAsgIndexResult {
  ok: boolean;
  index: AsgIndex;
  diagnostics: AsgIndexDiagnostic[];
  output_path: string;
}

export async function buildAsgIndex(root: string): Promise<BuildAsgIndexResult> {
  const config = await loadWorkspaceConfig(root);
  const domainRoot = normalizeRelativePath(config.source.domain_root);
  const generatedDir = normalizeRelativePath(config.source.generated_dir);
  const files = await readDomainSophiaFiles(root, domainRoot);
  const diagnostics: AsgIndexDiagnostic[] = [];
  const nodes: Record<string, AsgIndexNode> = {};

  for (const [filePath, content] of Object.entries(files).sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    if (!isSupportedSophiaFilePath(filePath, domainRoot)) {
      diagnostics.push(
        error(
          "INDEX-FILE-001",
          filePath,
          "Sophia file path is outside the v0 domain entity action/capability layout.",
        ),
      );
      continue;
    }

    const parsed = parseSophiaSource(content, filePath);
    if (!parsed.ok || !parsed.ast) {
      diagnostics.push(
        ...parsed.diagnostics
          .filter((diagnostic) => diagnostic.severity === "error")
          .map((diagnostic) => ({
            code: "INDEX-PARSE-001",
            severity: "error" as const,
            location: diagnostic.location ?? filePath,
            problem: `${diagnostic.code}: ${diagnostic.problem}`,
          })),
      );
      continue;
    }

    const declaration = parsed.ast;
    const expectedKind = expectedTopLevelKindForPath(filePath, domainRoot);
    if (expectedKind && declaration.kind !== expectedKind) {
      diagnostics.push(
        error(
          "INDEX-FILE-003",
          filePath,
          `File path expects ${expectedKind}, found ${declaration.kind}.`,
        ),
      );
      continue;
    }

    if (nodes[declaration.name]) {
      diagnostics.push(
        error("INDEX-NODE-001", filePath, `Duplicate top-level node name: ${declaration.name}.`),
      );
      continue;
    }

    const declarationKind = toIndexKind(declaration.kind);
    if (!declarationKind) {
      diagnostics.push(
        error("INDEX-PARSE-001", filePath, `Unsupported parsed node kind: ${declaration.kind}.`),
      );
      continue;
    }

    nodes[declaration.name] = {
      kind: declarationKind,
      domain: domainFromPath(filePath, domainRoot),
      path: filePath,
    };
  }

  const index: AsgIndex = {
    version: 1,
    nodes: Object.fromEntries(
      Object.entries(nodes).sort(([left], [right]) => left.localeCompare(right)),
    ),
  };
  const outputPath = path.join(root, generatedDir, "asg_index.json");
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${stableJson(index)}\n`, "utf8");

  return {
    ok: diagnostics.every((diagnostic) => diagnostic.severity !== "error"),
    index,
    diagnostics,
    output_path: path.relative(root, outputPath).split(path.sep).join("/"),
  };
}

async function readDomainSophiaFiles(
  root: string,
  domainRoot: string,
): Promise<Record<string, string>> {
  return collectSophiaFiles(path.join(root, domainRoot), domainRoot);
}

function toIndexKind(kind: string): AsgIndexNode["kind"] | null {
  if (kind === "domain") return "Domain";
  if (kind === "entity") return "Entity";
  if (kind === "action") return "Action";
  if (kind === "capability") return "Capability";
  if (kind === "error") return "Error";
  if (kind === "state") return "State";
  if (kind === "storage") return "Storage";
  return null;
}

function domainFromPath(filePath: string, domainRoot: string): string {
  const prefix = `${domainRoot}/`;
  return filePath.startsWith(prefix) ? (filePath.slice(prefix.length).split("/")[0] ?? "") : "";
}
