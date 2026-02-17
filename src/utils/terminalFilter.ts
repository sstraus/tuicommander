/** Filter branch terminal IDs to only those that exist in the terminal store */
export function filterValidTerminals(
  branchTerminals: string[] | undefined,
  existingTerminalIds: string[]
): string[] {
  if (!branchTerminals) return [];
  const existingSet = new Set(existingTerminalIds);
  return branchTerminals.filter((id) => existingSet.has(id));
}
