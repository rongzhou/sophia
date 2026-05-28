export function parsePseudocodeJson(content: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(content);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    return parsed as Record<string, unknown>;
  } catch {
    return null;
  }
}

export function pseudocodeSection(content: string, sectionName: string): string | null {
  const json = parsePseudocodeJson(content);
  if (json) {
    return stringifySection(json[sectionName]);
  }
  return null;
}

export function hasPseudocodeSection(content: string, sectionName: string): boolean {
  const json = parsePseudocodeJson(content);
  if (!json) return false;
  return Object.prototype.hasOwnProperty.call(json, sectionName);
}

export function readPseudoSection(content: string, sectionName: string): string {
  return pseudocodeSection(content, sectionName) ?? "";
}

export function hasPseudoSection(content: string, sectionName: string): boolean {
  return hasPseudocodeSection(content, sectionName);
}

export function pseudocodeAlgorithmLines(content: string): string[] | null {
  const json = parsePseudocodeJson(content);
  if (!json) return null;
  const algorithm = json.algorithm;
  if (Array.isArray(algorithm)) {
    return algorithm.map(stringifySection).filter((line): line is string => Boolean(line?.trim()));
  }
  const single = stringifySection(algorithm);
  return single ? single.split("\n").map((line) => line.trim()).filter(Boolean) : [];
}

function stringifySection(value: unknown): string | null {
  if (value === undefined || value === null) return null;
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) {
    return value
      .map((item) => stringifySection(item))
      .filter((item): item is string => Boolean(item?.trim()))
      .join("\n");
  }
  if (typeof value === "object") {
    const record = value as Record<string, unknown>;
    const name = stringifySection(record.name);
    const meaning = stringifySection(record.meaning);
    if (name && meaning) return `${name} := ${meaning}`;
    if (name) return name;
    if (meaning) return meaning;
    return Object.entries(record)
      .map(([key, item]) => {
        const rendered = stringifySection(item);
        return rendered ? `${key}: ${rendered}` : "";
      })
      .filter(Boolean)
      .join("\n");
  }
  return null;
}
