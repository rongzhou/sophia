import {
  braceDepth,
  extractNamedSection,
  readBraceBody,
  replaceNamedSection,
} from "../lang/braces.js";
import { pseudocodeSection } from "./document.js";

const REDACTED_SECTION_NAMES = ["expected"] as const;

export interface ImplementationStructurePlan {
  program_name: string;
  files: {
    domain: string;
    entities: string[];
    states: string[];
    capability: string;
    action: string;
  };
  symbols: {
    domain: string;
    capability: string;
    action: string;
  };
  action_contract_hints: {
    entities: Array<{
      name: string;
      fields: Array<{ name: string; type: string; source: string }>;
    }>;
    states: Array<{
      name: string;
      values: string[];
      source: string;
    }>;
    inputs: Array<{ name: string; type: string; source: string }>;
    output: { name: string; type: string; source: string } | null;
    effects: string[];
  };
  body_instruction: string;
}

export interface ImplementationStructureOverride {
  program?: string | undefined;
  domain?: string | undefined;
  action?: string | undefined;
  capability?: string | undefined;
  states?: Array<{ name: string; values: string[] }> | undefined;
  inputs?: Array<{ name: string; type: string }> | undefined;
  output?: { name: string; type: string } | undefined;
  effects?: string[] | undefined;
}

export function pseudocodeForImplementationPrompt(content: string): string {
  let sanitized = content;
  for (const sectionName of REDACTED_SECTION_NAMES) {
    sanitized = replaceNamedSection(
      sanitized,
      sectionName,
      `${sectionName} {\n  <redacted for implementation; deterministic audit uses the original .pseudo>\n}`,
    );
  }
  sanitized = replaceSectionIfPresent(
    sanitized,
    "constraints",
    (sectionBody) => `constraints {\n${redactConstraintBody(sectionBody)}\n}`,
  );
  return sanitized;
}

export function buildImplementationStructurePlan(
  content: string,
  override: ImplementationStructureOverride = {},
): ImplementationStructurePlan {
  const programName =
    normalizeOverrideName(override.program) ?? extractProgramName(content);
  const states = normalizeStateOverrides(override.states);
  const stateNames = new Set(states.map((state) => state.name));
  const entities = extractEntityHints(content).filter((entity) => !stateNames.has(entity.name));
  const domainName = normalizeOverrideName(override.domain) ?? `${programName}Domain`;
  const capabilityName =
    normalizeOverrideName(override.capability) ?? `${programName}Capability`;
  const actionName = normalizeOverrideName(override.action) ?? programName;
  const domainDir = `domains/${domainName}`;
  const effects = override.effects ? normalizeEffectOverrides(override.effects) : [];
  return {
    program_name: programName,
    files: {
      domain: `${domainDir}/domain.sophia`,
      entities: entities.map((entity) => `${domainDir}/entities/${entity.name}.sophia`),
      states: states.map((state) => `${domainDir}/states/${state.name}.sophia`),
      capability: `${domainDir}/capabilities/${capabilityName}.sophia`,
      action: `${domainDir}/actions/${actionName}.sophia`,
    },
    symbols: {
      domain: domainName,
      capability: capabilityName,
      action: actionName,
    },
    action_contract_hints: {
      entities,
      states,
      inputs: override.inputs
        ? normalizeFieldOverrides(override.inputs)
        : extractFieldHints(
            pseudocodeSection(content, "inputs") ?? extractNamedSection(content, "inputs") ?? "",
            entities,
          ),
      output: override.output
        ? normalizeFieldOverride(override.output)
        : (extractFieldHints(
            pseudocodeSection(content, "outputs") ?? extractNamedSection(content, "outputs") ?? "",
            entities,
          )[0] ?? null),
      effects,
    },
    body_instruction:
      "Use this only as a structural plan. Translate the algorithm into the action body; do not copy expected values or validation-only constraints.",
  };
}

function normalizeStateOverrides(
  states: Array<{ name: string; values: string[] }> | undefined,
): Array<{ name: string; values: string[]; source: string }> {
  return (states ?? []).map((state) => ({
    name: toPascalIdentifier(state.name),
    values: [...new Set(state.values.map(toPascalIdentifier))],
    source: `${state.name}: ${state.values.join(", ")}`,
  }));
}

function normalizeFieldOverrides(
  fields: Array<{ name: string; type: string }>,
): Array<{ name: string; type: string; source: string }> {
  return fields.map(normalizeFieldOverride);
}

function normalizeFieldOverride(field: { name: string; type: string }): {
  name: string;
  type: string;
  source: string;
} {
  return {
    name: toSnakeIdentifier(field.name),
    type: normalizeOverrideType(field.type),
    source: `${field.name}: ${field.type}`,
  };
}

function normalizeEffectOverrides(effects: string[]): string[] {
  return [...new Set(effects.map((effect) => effect.trim()).filter(Boolean))].sort();
}

function extractEntityHints(
  content: string,
): Array<{ name: string; fields: Array<{ name: string; type: string; source: string }> }> {
  const body = pseudocodeSection(content, "definitions") ?? extractNamedSection(content, "entities") ?? "";
  const entities: Array<{
    name: string;
    fields: Array<{ name: string; type: string; source: string }>;
  }> = [];
  const source = body.replace(/"[^"]*"/g, (match) => " ".repeat(match.length));
  for (const match of source.matchAll(/\b([A-Z][A-Za-z0-9]*)\s*\{/g)) {
    if (braceDepth(source.slice(0, match.index)) !== 0 || !match[1] || match.index === undefined) {
      continue;
    }
    const fieldsBody = readBraceBody(body, match.index + match[0].length);
    if (fieldsBody === null) continue;
    entities.push({ name: match[1], fields: extractFieldHints(fieldsBody) });
  }
  return entities;
}

function replaceSectionIfPresent(
  content: string,
  sectionName: string,
  replacement: (sectionBody: string) => string,
): string {
  const sectionBody = extractNamedSection(content, sectionName);
  if (sectionBody === null) return content;
  return replaceNamedSection(content, sectionName, replacement(sectionBody));
}

function redactConstraintBody(sectionBody: string): string {
  return sectionBody
    .split("\n")
    .map((line) => {
      if (/\bsequence\b|\bexpected\b|\bstdout\b|\bresult\b|\bexactly\b/i.test(line)) {
        return line.replace(/"[^"]*"/g, '"<redacted validation detail>"');
      }
      return line;
    })
    .join("\n")
    .trimEnd();
}

function extractProgramName(content: string): string {
  const match = /^\s*program\s+([A-Za-z_]\w*)\s*\{/m.exec(content);
  return toPascalIdentifier(match?.[1] ?? "Program");
}

function extractFieldHints(
  sectionBody: string,
  entityHints: Array<{ name: string; fields: Array<{ name: string }> }> = [],
): Array<{ name: string; type: string; source: string }> {
  return splitPseudoFieldSources(sectionBody)
    .map((source) => source.trim())
    .filter((source) => source && !/^none$/i.test(source))
    .map((source) => {
      const match = /^([a-z_]\w*)\s*(?::=|:)\s*(.+)$/.exec(source);
      if (!match?.[1] || !match[2]) return null;
      const type = normalizePseudoType(match[2], entityHints);
      if (!type) return null;
      return {
        name: match[1],
        type,
        source,
      };
    })
    .filter((field): field is { name: string; type: string; source: string } => field !== null);
}

function splitPseudoFieldSources(sectionBody: string): string[] {
  return sectionBody
    .split("\n")
    .map((line) => line.trim())
    .flatMap((line) => {
      if (!line || /^none$/i.test(line)) return [line];
      const parts = line.split(/\s*,\s*(?=[a-z_]\w*\s*(?::=|:))/);
      return parts.length > 0 ? parts : [line];
    });
}

function toPascalIdentifier(value: string): string {
  const words = value
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean);
  const identifier = words
    .map((word) => `${word.charAt(0).toUpperCase()}${word.slice(1)}`)
    .join("");
  return /^[A-Za-z_]\w*$/.test(identifier) ? identifier : "Program";
}

function toSnakeIdentifier(value: string): string {
  const words = value
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean);
  const identifier = words.map((word) => word.toLowerCase()).join("_");
  return /^[a-z_]\w*$/.test(identifier) ? identifier : "value";
}

function normalizeOverrideType(value: string): string {
  const trimmed = value.trim();
  if (/^Optional\s*</i.test(trimmed)) {
    return trimmed.replace(/\s+/g, "");
  }
  return trimmed;
}

function normalizeOverrideName(value: string | undefined): string | null {
  if (value === undefined) return null;
  return toPascalIdentifier(value);
}

function normalizePseudoType(
  value: string,
  entityHints: Array<{ name: string; fields: Array<{ name: string }> }> = [],
): string | null {
  const raw = value.trim();
  if (/^".*"$/.test(raw)) return null;
  const trimmed = raw;
  if (/^Unit$/i.test(trimmed)) return "Unit";
  if (/^Bool$/i.test(trimmed)) return "Bool";
  if (/^Int$/i.test(trimmed)) return "Int";
  if (/^Text$/i.test(trimmed)) return "Text";
  if (/^[A-Z][A-Za-z0-9]*$/.test(trimmed)) return trimmed;
  if (/^List\s*<\s*Int\s*>$/i.test(trimmed)) return "List<Int>";
  if (/^List\s*<\s*Text\s*>$/i.test(trimmed)) return "List<Text>";
  if (/^Optional\s*<\s*.+\s*>$/i.test(trimmed)) return trimmed.replace(/\s+/g, "");
  if (/^(Raw|Parsed|Validated|Sanitized|Verified|Authorized|Persisted|Secret|Redacted)\s*<\s*.+\s*>$/i.test(trimmed)) {
    return trimmed.replace(/\s+/g, "");
  }
  if (entityHints.some((entity) => entity.name === trimmed)) return trimmed;
  return null;
}
