export type TomlValue = string | boolean | number | string[];

export function parseMinimalToml(content: string): Map<string, TomlValue> {
  const values = new Map<string, TomlValue>();
  let section = "";
  for (const rawLine of content.split("\n")) {
    const line = stripTomlComment(rawLine).trim();
    if (!line) continue;
    const sectionMatch = /^\[([A-Za-z_][\w.]*)\]$/.exec(line);
    if (sectionMatch?.[1]) {
      section = sectionMatch[1];
      continue;
    }
    const assignmentMatch = /^([A-Za-z_]\w*)\s*=\s*(.+)$/.exec(line);
    if (assignmentMatch?.[1] && assignmentMatch[2] !== undefined) {
      values.set(`${section}.${assignmentMatch[1]}`, parseTomlValue(assignmentMatch[2].trim()));
    }
  }
  return values;
}

export function parseTomlValue(rawValue: string): TomlValue {
  const stringMatch = /^"((?:\\"|\\\\|[^"])*)"$/.exec(rawValue);
  if (stringMatch?.[1] !== undefined) {
    return unescapeTomlString(stringMatch[1]);
  }
  if (rawValue === "true") return true;
  if (rawValue === "false") return false;
  if (/^-?\d+(?:\.\d+)?$/.test(rawValue)) return Number(rawValue);
  const arrayMatch = /^\[(.*)\]$/.exec(rawValue);
  if (arrayMatch?.[1] !== undefined) {
    const body = arrayMatch[1].trim();
    if (!body) return [];
    return body.split(",").map((item) => {
      const parsed = parseTomlValue(item.trim());
      if (typeof parsed !== "string") {
        throw new Error(`Only string arrays are supported in sophia.toml: ${rawValue}`);
      }
      return parsed;
    });
  }
  throw new Error(`Unsupported sophia.toml value: ${rawValue}`);
}

export function stripTomlComment(line: string): string {
  let inString = false;
  let escaped = false;
  let result = "";
  for (const char of line) {
    if (escaped) {
      result += char;
      escaped = false;
      continue;
    }
    if (char === "\\") {
      result += char;
      escaped = true;
      continue;
    }
    if (char === '"') {
      inString = !inString;
      result += char;
      continue;
    }
    if (char === "#" && !inString) break;
    result += char;
  }
  return result;
}

export function unescapeTomlString(value: string): string {
  return value.replace(/\\"/g, '"').replace(/\\\\/g, "\\");
}

export function requireConfigValue(values: Map<string, TomlValue>, key: string): string {
  const value = values.get(key);
  if (typeof value !== "string" || !value) {
    throw new Error(`Missing required sophia.toml value: ${key}`);
  }
  return value;
}

export function requireBooleanConfigValue(values: Map<string, TomlValue>, key: string): boolean {
  const value = values.get(key);
  if (typeof value !== "boolean") {
    throw new Error(`Missing required sophia.toml boolean value: ${key}`);
  }
  return value;
}
