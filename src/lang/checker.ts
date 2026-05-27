import { checkActionDeclarations } from "./checker_action_rules.js";
import { checkActionCallGraph } from "./checker_action_graph.js";
import {
  checkCapabilityDeclarations,
  checkEntityDeclarations,
  checkErrorDeclarations,
  checkStateDeclarations,
  checkStorageDeclarations,
} from "./checker_declaration_rules.js";
import { createCheckerContext, type CheckerContext } from "./checker_context.js";
import {
  checkFileLayout,
  checkTopLevelNames,
  checkUnsupportedSyntax,
} from "./checker_file_rules.js";
import type { SophiaFileSet } from "./check_model.js";
import type { CheckResult } from "./diagnostics.js";
import { error } from "./diagnostics.js";
import { parseSophiaTopLevelDeclarations } from "./parser.js";

export function checkSophiaFiles(files: SophiaFileSet): CheckResult {
  const context = createCheckerContext(files);

  if (Object.keys(files).filter((filePath) => filePath.endsWith(".sophia")).length === 0) {
    context.diagnostics.push(
      error("CHECK-FILE-002", "<files>", "No .sophia files were provided to checker."),
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
