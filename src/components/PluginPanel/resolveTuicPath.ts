import { isAbsolutePath, normalizeSep, pathStartsWith, pathStripPrefix, joinPath } from "../../utils/pathUtils";

export interface ResolvedPath {
  repoPath: string;
  relPath: string;
}

/** Normalize a path: resolve . and .. segments, collapse multiple slashes, handle both separators. */
function normalizePath(p: string): string {
  const n = normalizeSep(p);
  const parts = n.split("/");
  const out: string[] = [];
  for (const seg of parts) {
    if (seg === "." || seg === "") continue;
    if (seg === "..") { out.pop(); continue; }
    out.push(seg);
  }
  if (n.startsWith("/")) return "/" + out.join("/");
  return out.join("/");
}

/**
 * Resolve a path (absolute or relative) to a repo + relative-path pair.
 *
 * - Absolute paths are matched against the known repo list (longest match wins).
 * - Relative paths are resolved against `activeRepoPath`.
 * - Path traversal (../) that escapes the repo root returns null.
 */
export function resolveTuicPath(
  path: string,
  repoPaths: string[],
  activeRepoPath: string | null,
): ResolvedPath | null {
  if (!path) return null;

  if (isAbsolutePath(path)) {
    const sorted = [...repoPaths].sort((a, b) => b.length - a.length);
    const repo = sorted.find((rp) => pathStartsWith(path, rp));
    if (!repo) return null;
    return { repoPath: repo, relPath: pathStripPrefix(path, repo)! };
  }

  if (!activeRepoPath) return null;

  const absoluteResolved = normalizePath(joinPath(activeRepoPath, path));

  if (!pathStartsWith(absoluteResolved, activeRepoPath)) {
    return null;
  }

  const relPath = pathStripPrefix(absoluteResolved, activeRepoPath)!;
  return { repoPath: activeRepoPath, relPath };
}
