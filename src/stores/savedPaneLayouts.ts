import type { PaneLayoutState } from "./paneLayout";

/** In-memory pane layout cache per branch — survives branch switches within a session */
export const savedPaneLayouts = new Map<string, PaneLayoutState>();

/** Build a cache key for a repo+branch pair */
export function paneLayoutKey(repoPath: string, branchName: string): string {
  return `${repoPath}\0${branchName}`;
}
