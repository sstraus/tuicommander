/** Find terminal IDs not associated with any branch */
export function findOrphanTerminals(
  terminalIds: string[],
  branchTerminalMap: Record<string, string[]>
): string[] {
  const associatedIds = new Set(Object.values(branchTerminalMap).flat());
  return terminalIds.filter((id) => !associatedIds.has(id));
}
