/** Branch sorting for sidebar display.
 *
 * Sorting rules:
 * 1. Main/primary branches first (main, master, develop, etc.)
 * 2. Merged/closed PR branches at the bottom
 * 3. Alphabetical within each group
 *
 * The "main first + alphabetical" part mirrors Rust's sort_branches in git.rs.
 * The "merged/closed to bottom" part requires GitHub PR state which is only
 * available on the frontend.
 */

export interface SortableBranch {
  name: string;
  isMain: boolean;
}

/**
 * Compare two branches for sorting.
 * Main branches first, then merged/closed PRs to bottom, then alphabetical.
 */
export function compareBranches(
  a: SortableBranch,
  b: SortableBranch,
  aPr: { state?: string } | null | undefined,
  bPr: { state?: string } | null | undefined,
): number {
  // Main branches first
  if (a.isMain && !b.isMain) return -1;
  if (!a.isMain && b.isMain) return 1;

  // Merged/closed PRs sort to bottom
  const aMerged = aPr?.state === "MERGED" || aPr?.state === "CLOSED";
  const bMerged = bPr?.state === "MERGED" || bPr?.state === "CLOSED";
  if (aMerged && !bMerged) return 1;
  if (!aMerged && bMerged) return -1;

  return a.name.localeCompare(b.name);
}
