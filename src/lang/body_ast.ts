import type { Diagnostic } from "./diagnostics.js";
import { error } from "./diagnostics.js";

export type SophiaBodyStatement =
  | {
      kind: "let";
      line: number;
      mutable: boolean;
      name: string;
      expression: string;
    }
  | {
      kind: "set";
      line: number;
      name: string;
      expression: string;
    }
  | {
      kind: "print";
      line: number;
      expression: string;
    }
  | {
      kind: "return";
      line: number;
      expression: string;
    }
  | {
      kind: "raise";
      line: number;
      variant: string;
      expression: string;
    }
  | {
      kind: "repeat";
      line: number;
      count: number;
      body: SophiaBodyStatement[];
    }
  | {
      kind: "if";
      line: number;
      condition: string;
      thenBody: SophiaBodyStatement[];
      elseBody: SophiaBodyStatement[];
    }
  | {
      kind: "match";
      line: number;
      expression: string;
      cases: SophiaMatchCase[];
    };

export interface SophiaMatchCase {
  line: number;
  pattern: string;
  binding: string | null;
  body: SophiaBodyStatement[];
}

export interface SophiaBodyAst {
  statements: SophiaBodyStatement[];
  diagnostics: Diagnostic[];
}

interface BodyLine {
  text: string;
  line: number;
}

type CloseToken = "block" | "else";

interface ParseListResult {
  statements: SophiaBodyStatement[];
  diagnostics: Diagnostic[];
  nextIndex: number;
  close: CloseToken | null;
}

export function parseSophiaBody(body: string, filePath: string): SophiaBodyAst {
  const lines = body
    .split("\n")
    .map((rawLine, index) => ({ text: rawLine.trim(), line: index + 1 }))
    .filter((line) => line.text.length > 0 && line.text !== "{");
  const parsed = parseStatementList(lines, 0, filePath);
  const diagnostics = [...parsed.diagnostics];
  if (parsed.close) {
    diagnostics.push(
      blockError(
        filePath,
        lines[parsed.nextIndex - 1]?.line ?? 1,
        "Body contains an unmatched closing brace.",
      ),
    );
  }
  return {
    statements: parsed.statements,
    diagnostics,
  };
}

export function flattenSophiaBodyStatements(
  statements: SophiaBodyStatement[],
): SophiaBodyStatement[] {
  const flattened: SophiaBodyStatement[] = [];
  for (const statement of statements) {
    flattened.push(statement);
    if (statement.kind === "repeat") {
      flattened.push(...flattenSophiaBodyStatements(statement.body));
    }
    if (statement.kind === "if") {
      flattened.push(...flattenSophiaBodyStatements(statement.thenBody));
      flattened.push(...flattenSophiaBodyStatements(statement.elseBody));
    }
    if (statement.kind === "match") {
      for (const matchCase of statement.cases) {
        flattened.push(...flattenSophiaBodyStatements(matchCase.body));
      }
    }
  }
  return flattened;
}

function parseStatementList(
  lines: BodyLine[],
  startIndex: number,
  filePath: string,
): ParseListResult {
  const statements: SophiaBodyStatement[] = [];
  const diagnostics: Diagnostic[] = [];
  let index = startIndex;

  while (index < lines.length) {
    const line = lines[index];
    if (!line) break;
    if (line.text === "}") {
      return { statements, diagnostics, nextIndex: index + 1, close: "block" };
    }
    if (/^\}\s*else\s*\{$/.test(line.text) || line.text === "else {") {
      return { statements, diagnostics, nextIndex: index + 1, close: "else" };
    }

    const matchMatch = /^match\s+(.+)\s*\{$/.exec(line.text);
    if (matchMatch?.[1]) {
      const parsed = parseMatchCases(lines, index + 1, filePath, line.line);
      diagnostics.push(...parsed.diagnostics);
      statements.push({
        kind: "match",
        line: line.line,
        expression: matchMatch[1].trim(),
        cases: parsed.cases,
      });
      index = parsed.nextIndex;
      continue;
    }

    const repeatMatch = /^repeat\s+(\d+)\s+times\s*\{$/.exec(line.text);
    if (repeatMatch?.[1]) {
      const child = parseStatementList(lines, index + 1, filePath);
      diagnostics.push(...child.diagnostics);
      if (child.close === "else") {
        diagnostics.push(
          blockError(
            filePath,
            lines[child.nextIndex - 1]?.line ?? line.line,
            "else block does not immediately close an if block.",
            "Use if condition { ... } else { ... } with no orphan else blocks.",
          ),
        );
      } else if (!child.close) {
        diagnostics.push(
          blockError(
            filePath,
            line.line,
            "Body contains an unclosed repeat or if block.",
            "Ensure every repeat and if/else block is closed inside body.",
          ),
        );
      }
      statements.push({
        kind: "repeat",
        line: line.line,
        count: Number.parseInt(repeatMatch[1], 10),
        body: child.statements,
      });
      index = child.nextIndex;
      continue;
    }

    const ifMatch = /^if\s+(.+)\s*\{$/.exec(line.text);
    if (ifMatch?.[1]) {
      const thenBlock = parseStatementList(lines, index + 1, filePath);
      diagnostics.push(...thenBlock.diagnostics);
      let elseBody: SophiaBodyStatement[] = [];
      let nextIndex = thenBlock.nextIndex;
      if (thenBlock.close === "else") {
        const elseBlock = parseStatementList(lines, thenBlock.nextIndex, filePath);
        diagnostics.push(...elseBlock.diagnostics);
        elseBody = elseBlock.statements;
        nextIndex = elseBlock.nextIndex;
        if (elseBlock.close === "else") {
          diagnostics.push(
            blockError(
              filePath,
              lines[elseBlock.nextIndex - 1]?.line ?? line.line,
              "else block does not immediately close an if block.",
              "Use if condition { ... } else { ... } with no orphan else blocks.",
            ),
          );
        } else if (!elseBlock.close) {
          diagnostics.push(
            blockError(
              filePath,
              line.line,
              "Body contains an unclosed repeat or if block.",
              "Ensure every repeat and if/else block is closed inside body.",
            ),
          );
        }
      } else if (!thenBlock.close) {
        diagnostics.push(
          blockError(
            filePath,
            line.line,
            "Body contains an unclosed repeat or if block.",
            "Ensure every repeat and if/else block is closed inside body.",
          ),
        );
      }
      statements.push({
        kind: "if",
        line: line.line,
        condition: ifMatch[1].trim(),
        thenBody: thenBlock.statements,
        elseBody,
      });
      index = nextIndex;
      continue;
    }

    const letMatch = /^let\s+(mutable\s+)?([a-z_]\w*)\s*=\s*(.+)$/.exec(line.text);
    if (letMatch?.[2] && letMatch[3]) {
      statements.push({
        kind: "let",
        line: line.line,
        mutable: Boolean(letMatch[1]),
        name: letMatch[2],
        expression: letMatch[3].trim(),
      });
      index += 1;
      continue;
    }

    const typedLetMatch =
      /^let\s+(mutable\s+)?([a-z_]\w*)\s*:\s*([A-Z][A-Za-z0-9]*(?:\s*<\s*[A-Z][A-Za-z0-9]*\s*>)?)(?:\s*=\s*(.+))?$/.exec(
        line.text,
      );
    if (typedLetMatch?.[2]) {
      diagnostics.push(
        error(
          typedLetMatch[4] ? "CHECK-SYNTAX-010" : "CHECK-SYNTAX-011",
          `${filePath}:${line.line}`,
          typedLetMatch[4]
            ? `Local variable declarations do not support type annotations: ${line.text}`
            : `Mutable local declarations must be initialized without a type annotation: ${line.text}`,
          typedLetMatch[4]
            ? "Use let name = expr or let mutable name = expr."
            : "Use let mutable name = initial_expr.",
        ),
      );
      index += 1;
      continue;
    }

    const setMatch = /^set\s+([a-z_]\w*)\s*=\s*(.+)$/.exec(line.text);
    if (setMatch?.[1] && setMatch[2]) {
      statements.push({
        kind: "set",
        line: line.line,
        name: setMatch[1],
        expression: setMatch[2].trim(),
      });
      index += 1;
      continue;
    }

    const printMatch = /^print\s+(.+)$/.exec(line.text);
    if (printMatch?.[1]) {
      statements.push({ kind: "print", line: line.line, expression: printMatch[1].trim() });
      index += 1;
      continue;
    }

    const returnMatch = /^return\s+(.+)$/.exec(line.text);
    if (returnMatch?.[1]) {
      statements.push({ kind: "return", line: line.line, expression: returnMatch[1].trim() });
      index += 1;
      continue;
    }

    const raiseMatch = /^raise\s+([A-Z][A-Za-z0-9]*)\s*(\{.*\})$/.exec(line.text);
    if (raiseMatch?.[1] && raiseMatch[2]) {
      statements.push({
        kind: "raise",
        line: line.line,
        variant: raiseMatch[1],
        expression: `${raiseMatch[1]} ${raiseMatch[2].trim()}`,
      });
      index += 1;
      continue;
    }

    if (/^return\s*$/.test(line.text)) {
      diagnostics.push(
        error(
          "CHECK-SYNTAX-013",
          `${filePath}:${line.line}`,
          "Bare return is not valid Sophia v0 syntax.",
          "Use return unit for Unit actions, or return an expression matching the action output type.",
        ),
      );
      index += 1;
      continue;
    }

    if (/^[A-Z][A-Za-z0-9]*\s*\{.*\}$/.test(line.text)) {
      diagnostics.push(
        error(
          "CHECK-SYNTAX-015",
          `${filePath}:${line.line}`,
          "Action calls are expressions, not standalone body statements.",
          "Use let ignored = ActionName { input = value } when calling a Unit action only for its effects.",
        ),
      );
      index += 1;
      continue;
    }

    diagnostics.push(
      bodyStatementError(
        filePath,
        line.line,
        `Unsupported Sophia v0 body statement: ${line.text}`,
        "Use only let, let mutable, set, print, repeat N times, if/else, match, raise, and return statements.",
      ),
    );
    index += 1;
  }

  return { statements, diagnostics, nextIndex: index, close: null };
}

function parseMatchCases(
  lines: BodyLine[],
  startIndex: number,
  filePath: string,
  matchLine: number,
): {
  cases: SophiaMatchCase[];
  diagnostics: Diagnostic[];
  nextIndex: number;
} {
  const cases: SophiaMatchCase[] = [];
  const diagnostics: Diagnostic[] = [];
  let index = startIndex;

  while (index < lines.length) {
    const line = lines[index];
    if (!line) break;
    if (line.text === "}") {
      return { cases, diagnostics, nextIndex: index + 1 };
    }

    const caseMatch = /^(.+?)\s*\{$/.exec(line.text);
    if (!caseMatch?.[1]) {
      diagnostics.push(
        blockError(
          filePath,
          line.line,
          `Invalid match case syntax: ${line.text}`,
          "Use Pattern { ... } inside a match block.",
        ),
      );
      index += 1;
      continue;
    }

    const rawPattern = caseMatch[1].trim();
    if (rawPattern === "_") {
      diagnostics.push(
        blockError(
          filePath,
          line.line,
          "Sophia match does not support catch-all _ cases.",
          "Write every Bool, state, or Optional case explicitly.",
        ),
      );
      index += 1;
      continue;
    }

    const parsedPattern = parseMatchPattern(rawPattern);
    if (!parsedPattern) {
      diagnostics.push(
        blockError(
          filePath,
          line.line,
          `Unsupported match case pattern: ${rawPattern}`,
          "Use true, false, None, Some(name), or StateName.Value patterns.",
        ),
      );
      index += 1;
      continue;
    }

    const child = parseStatementList(lines, index + 1, filePath);
    diagnostics.push(...child.diagnostics);
    if (child.close === "else") {
      diagnostics.push(
        blockError(
          filePath,
          lines[child.nextIndex - 1]?.line ?? line.line,
          "else block does not immediately close an if block.",
          "Use if condition { ... } else { ... } with no orphan else blocks.",
        ),
      );
    } else if (!child.close) {
      diagnostics.push(
        blockError(
          filePath,
          line.line,
          "Body contains an unclosed match case block.",
          "Ensure every match case and match block is closed.",
        ),
      );
      return { cases, diagnostics, nextIndex: child.nextIndex };
    }

    cases.push({
      line: line.line,
      pattern: parsedPattern.pattern,
      binding: parsedPattern.binding,
      body: child.statements,
    });
    index = child.nextIndex;
  }

  diagnostics.push(
    blockError(
      filePath,
      matchLine,
      "Body contains an unclosed match block.",
      "Close the match block after all case blocks.",
    ),
  );
  return { cases, diagnostics, nextIndex: index };
}

function parseMatchPattern(pattern: string): { pattern: string; binding: string | null } | null {
  if (pattern === "true" || pattern === "false" || pattern === "None") {
    return { pattern, binding: null };
  }
  const someMatch = /^Some\(([a-z_]\w*)\)$/.exec(pattern);
  if (someMatch?.[1]) {
    return { pattern: "Some", binding: someMatch[1] };
  }
  if (/^[A-Z][A-Za-z0-9]*\.[A-Z][A-Za-z0-9]*$/.test(pattern)) {
    return { pattern, binding: null };
  }
  return null;
}

function bodyStatementError(
  filePath: string,
  line: number,
  problem: string,
  repair?: string,
): Diagnostic {
  return error("CHECK-BODY-004", `${filePath}:${line}`, problem, repair);
}

function blockError(filePath: string, line: number, problem: string, repair?: string): Diagnostic {
  return error("CHECK-BLOCK-001", `${filePath}:${line}`, problem, repair);
}
