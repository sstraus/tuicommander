export interface EnvVarEntry {
  key: string;
  value: string;
}

/** Find keys that appear more than once after trimming. Empty/whitespace-only keys are ignored. */
export function findDuplicateEnvKeys(entries: readonly EnvVarEntry[]): string[] {
  const counts = new Map<string, number>();
  for (const { key } of entries) {
    const k = key.trim();
    if (!k) continue;
    counts.set(k, (counts.get(k) ?? 0) + 1);
  }
  const dupes: string[] = [];
  for (const [k, count] of counts) {
    if (count > 1) dupes.push(k);
  }
  return dupes;
}

/** Build an env Record from entries. Throws if duplicate keys detected. Empty/whitespace keys are filtered out. */
export function buildEnvFromEntries(entries: readonly EnvVarEntry[]): Record<string, string> {
  const dupes = findDuplicateEnvKeys(entries);
  if (dupes.length > 0) {
    throw new Error(`Duplicate env keys: ${dupes.join(", ")}`);
  }
  const env: Record<string, string> = {};
  for (const { key, value } of entries) {
    const k = key.trim();
    if (k) env[k] = value;
  }
  return env;
}
