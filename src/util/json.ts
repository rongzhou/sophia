export function stableJson(value: unknown): string {
  return JSON.stringify(sortJson(value), null, 2);
}

export function sortJson(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortJson);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => [key, sortJson(item)]),
  );
}

export function deepEqualJson(left: unknown, right: unknown): boolean {
  return stableJson(left) === stableJson(right);
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
