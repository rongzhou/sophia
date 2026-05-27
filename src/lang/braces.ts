import { escapeRegExp } from "../util/strings.js";

export interface NamedSectionLocation {
  start: number;
  end: number;
  bodyStart: number;
  bodyEnd: number;
  indent: string;
}

export function readBraceBody(content: string, start: number): string | null {
  let depth = 1;
  let body = "";
  for (const char of content.slice(start)) {
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
    if (depth === 0) return body;
    body += char;
  }
  return null;
}

export function braceDepth(content: string): number {
  let depth = 0;
  for (const char of content) {
    if (char === "{") depth += 1;
    if (char === "}") depth = Math.max(0, depth - 1);
  }
  return depth;
}

export function extractNamedSection(content: string, sectionName: string): string | null {
  const location = findNamedSection(content, sectionName);
  return location ? content.slice(location.bodyStart, location.bodyEnd) : null;
}

export function findNamedSection(
  content: string,
  sectionName: string,
): NamedSectionLocation | null {
  const startPattern = new RegExp(`(^|\\n)(\\s*)${escapeRegExp(sectionName)}\\s*\\{`, "m");
  const match = startPattern.exec(content);
  if (!match || match.index === undefined) return null;
  const prefix = match[1] ?? "";
  const indent = match[2] ?? "";
  const start = match.index + prefix.length;
  const bodyStart = match.index + match[0].length;
  let depth = 1;
  for (let index = bodyStart; index < content.length; index += 1) {
    const char = content[index];
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
    if (depth === 0) {
      return { start, end: index + 1, bodyStart, bodyEnd: index, indent };
    }
  }
  return null;
}

export function replaceNamedSection(
  content: string,
  sectionName: string,
  replacement: string,
): string {
  const location = findNamedSection(content, sectionName);
  if (!location) return content;
  return `${content.slice(0, location.start)}${location.indent}${replacement}${content.slice(
    location.end,
  )}`;
}

export function stripQuotedText(content: string): string {
  return content.replace(/"[^"]*"|'[^']*'/g, (match) => " ".repeat(match.length));
}
