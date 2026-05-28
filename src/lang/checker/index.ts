import { checkActionDeclarations } from "./action_rules.js";
import { checkActionCallGraph } from "./action_graph.js";
import {
  checkCapabilityDeclarations,
  checkEntityDeclarations,
  checkErrorDeclarations,
  checkStateDeclarations,
  checkStorageDeclarations,
} from "./declaration_rules.js";
import { createCheckerContext, type CheckerContext } from "./context.js";
import {
  checkFileLayout,
  checkTopLevelNames,
  checkUnsupportedSyntax,
} from "./file_rules.js";
import type { SophiaFileSet } from "../ast/check_model.js";
import type { CheckResult } from "../ast/diagnostics.js";
import { errorDiagnostic } from "../ast/diagnostics.js";
import { parseSophiaTopLevelDeclarations } from "../ast/parser.js";

export function checkSophiaFiles(files: SophiaFileSet): CheckResult {
  const context = createCheckerContext(files);

  if (Object.keys(files).filter((filePath) => filePath.endsWith(".sophia")).length === 0) {
    context.diagnostics.push(
      errorDiagnostic("CHECK-FILE-002", "<files>", "No .sophia files were provided to checker."),
    );
  }

  for (const [filePath, content] of Object.entries(files)) {
    if (!filePath.endsWith(".sophia")) continue;
    checkSophiaFile(context, filePath, content);
  }
  checkActionCallGraph(context, files);

  return {
    ok: context.diagnostics.every((diagnostic) => diagnostic.severity !== "error"),
    diagnostics: context.diagnostics,
  };
}

function checkSophiaFile(context: CheckerContext, filePath: string, content: string): void {
  const topLevelBlocks = parseSophiaTopLevelDeclarations(content);
  checkFileLayout(context, filePath, content, topLevelBlocks);
  checkTopLevelNames(context, filePath, topLevelBlocks);
  checkUnsupportedSyntax(context, filePath, content);
  checkCapabilityDeclarations(context, filePath, content);
  checkEntityDeclarations(context, filePath, content);
  checkStateDeclarations(context, filePath, content);
  checkErrorDeclarations(context, filePath, content);
  checkStorageDeclarations(context, filePath, content);
  checkActionDeclarations(context, filePath, content);
}
