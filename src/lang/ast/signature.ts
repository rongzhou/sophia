import type { SophiaField } from "./types.js";

export function parseSophiaFieldDeclarations(block: string): SophiaField[] {
  const fields: SophiaField[] = [];
  let index = 0;

  while (index < block.length) {
    const nameMatch = /^[\s,]*([a-z_]\w*)\s*:/.exec(block.slice(index));
    if (!nameMatch?.[1]) {
      index += 1;
      continue;
    }

    const name = nameMatch[1];
    let cursor = index + nameMatch[0].length;
    while (/\s/.test(block[cursor] ?? "")) cursor += 1;

    const typeStart = cursor;
    let depth = 0;
    while (cursor < block.length) {
      const char = block[cursor] ?? "";
      if (char === "<") {
        depth += 1;
      } else if (char === ">") {
        if (depth === 0) break;
        depth -= 1;
      } else if (depth === 0 && /[\s,]/.test(char)) {
        break;
      }
      cursor += 1;
    }

    const type = block.slice(typeStart, cursor).trim();
    if (/^[A-Za-z]\w*(?:<.+>)?$/.test(type)) {
      fields.push({ name, type });
    }
    index = cursor;
  }

  return fields;
}

export function parseSophiaEffectNames(block: string): string[] {
  const effects: string[] = [];
  const sourceWithoutParameterizedEffects = block.replace(
    /\b([A-Z][\w]*(?:\.[A-Z][\w]*)?)\("([A-Z][A-Za-z0-9]*)"\)/g,
    (_match, effect: string, target: string) => {
      effects.push(`${effect}("${target}")`);
      return " ".repeat(String(_match).length);
    },
  );
  effects.push(
    ...[...sourceWithoutParameterizedEffects.matchAll(/\b[A-Z][\w]*(?:\.[A-Z][\w]*)?\b/g)].map(
      (match) => match[0],
    ),
  );
  return effects;
}

const STORAGE_EFFECT_PATTERN = /^DB\.(Read|Write)\("([A-Z][A-Za-z0-9]*)"\)$/;

export interface SophiaStorageEffect {
  mode: "Read" | "Write";
  storage: string;
}

export function parseSophiaStorageEffect(effect: string): SophiaStorageEffect | null {
  const match = STORAGE_EFFECT_PATTERN.exec(effect);
  if (!match?.[1] || !match[2]) return null;
  return { mode: match[1] as "Read" | "Write", storage: match[2] };
}

export function isSophiaStorageEffect(effect: string): boolean {
  return STORAGE_EFFECT_PATTERN.test(effect);
}
