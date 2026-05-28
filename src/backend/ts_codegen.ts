import { mkdir, rename, unlink, writeFile } from "node:fs/promises";
import path from "node:path";
import { checkSophiaFiles } from "../lang/checker/index.js";
import { errorDiagnostic, type Diagnostic } from "../lang/ast/diagnostics.js";
import { parseSophiaSource } from "../lang/ast/parser.js";
import { loadWorkspaceConfig } from "../workspace/workspace.js";
import { collectSophiaFiles, withFileLock } from "../util/fs.js";
import { normalizeRelativePath } from "../util/strings.js";
import { checkStripAssistTypeScriptEquivalence } from "./strip_assist_equivalence.js";
import { emitTypeScript, type SophiaSourceFile } from "./ts_emit_module.js";

export interface TypeScriptBuildResult {
  ok: boolean;
  target: "typescript";
  output_dir: string;
  files: string[];
  diagnostics: Diagnostic[];
}

export async function buildTypeScript(root: string): Promise<TypeScriptBuildResult> {
  const config = await loadWorkspaceConfig(root);
  if (config.build.target !== "typescript") {
    return {
      ok: false,
      target: "typescript",
      output_dir: normalizeRelativePath(config.build.out_dir),
      files: [],
      diagnostics: [
        errorDiagnostic("BUILD-TARGET-001", "sophia.toml", `Unsupported build target: ${config.build.target}.`),
      ],
    };
  }

  const domainRoot = normalizeRelativePath(config.source.domain_root);
  const outputDir = normalizeRelativePath(config.build.out_dir);
  const sourceFiles = await readDomainSophiaFiles(root, domainRoot);
  const check = checkSophiaFiles(sourceFiles);
  if (!check.ok) {
    return {
      ok: false,
      target: "typescript",
      output_dir: outputDir,
      files: [],
      diagnostics: check.diagnostics.map((diagnostic) => ({
        code: "BUILD-CHECK-001",
        severity: diagnostic.severity === "error" ? "error" : "warning",
        location: diagnostic.location ?? "<unknown>",
        problem: `${diagnostic.code}: ${diagnostic.problem}`,
      })),
    };
  }

  const parsedFiles: SophiaSourceFile[] = [];
  const diagnostics: Diagnostic[] = [];
  for (const [filePath, content] of Object.entries(sourceFiles).sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    const parsed = parseSophiaSource(content, filePath);
    if (!parsed.ok || !parsed.ast) {
      diagnostics.push(
        ...parsed.diagnostics.map((diagnostic) => ({
          code: "BUILD-PARSE-001",
          severity: diagnostic.severity,
          ...(diagnostic.location ? { location: diagnostic.location } : {}),
          problem: `${diagnostic.code}: ${diagnostic.problem}`,
        })),
      );
      continue;
    }
    parsedFiles.push({ path: filePath, ast: parsed.ast });
  }
  if (diagnostics.some((diagnostic) => diagnostic.severity === "error")) {
    return {
      ok: false,
      target: "typescript",
      output_dir: outputDir,
      files: [],
      diagnostics,
    };
  }

  let emitted: string;
  try {
    emitted = emitTypeScript(parsedFiles);
    if (config.check.require_strip_assist_equivalence) {
      const stripAssist = checkStripAssistTypeScriptEquivalence(parsedFiles);
      if (!stripAssist.ok) {
        return {
          ok: false,
          target: "typescript",
          output_dir: outputDir,
          files: [],
          diagnostics: stripAssist.diagnostics,
        };
      }
    }
  } catch (error) {
    return {
      ok: false,
      target: "typescript",
      output_dir: outputDir,
      files: [],
      diagnostics: [
        {
          code: "BUILD-CODEGEN-001",
          severity: "error",
          location: "<codegen>",
          problem: error instanceof Error ? error.message : String(error),
        },
      ],
    };
  }
  await writeBuildOutputAtomically(root, outputDir, "index.ts", emitted);

  return {
    ok: true,
    target: "typescript",
    output_dir: outputDir,
    files: [`${outputDir}/index.ts`],
    diagnostics: [],
  };
}

async function writeBuildOutputAtomically(
  root: string,
  outputDir: string,
  fileName: string,
  content: string,
): Promise<void> {
  const absoluteOutputDir = path.join(root, outputDir);
  await mkdir(absoluteOutputDir, { recursive: true });
  await withBuildLock(absoluteOutputDir, async () => {
    const outputPath = path.join(absoluteOutputDir, fileName);
    const tempPath = path.join(
      absoluteOutputDir,
      `.${fileName}.${process.pid}.${Date.now()}.${Math.random().toString(16).slice(2)}.tmp`,
    );
    try {
      await writeFile(tempPath, content, "utf8");
      await rename(tempPath, outputPath);
    } finally {
      await unlink(tempPath).catch(() => undefined);
    }
  });
}

async function withBuildLock<T>(outputDir: string, operation: () => Promise<T>): Promise<T> {
  const lockPath = path.join(outputDir, ".build.lock");
  return withFileLock({
    lockPath,
    attempts: 100,
    retryMs: 20,
    operation,
    errorLabel: `build lock ${lockPath}`,
  });
}

async function readDomainSophiaFiles(
  root: string,
  domainRoot: string,
): Promise<Record<string, string>> {
  return collectSophiaFiles(path.join(root, domainRoot), domainRoot);
}
