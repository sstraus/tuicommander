export interface ResolvedPath {
  repoPath: string;
  relPath: string;
}

/** Normalize a POSIX path: resolve . and .. segments, collapse multiple slashes */
function normalizePath(p: string): string {
  const parts = p.split("/");
  const out: string[] = [];
  for (const seg of parts) {
    if (seg === "." || seg === "") continue;
    if (seg === "..") { out.pop(); continue; }
    out.push(seg);
  }
  return (p.startsWith("/") ? "/" : "") + out.join("/");
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

  // Absolute path — backward-compatible behavior
  if (path.startsWith("/")) {
    // Sort repos longest-first so nested repos match before parents
    const sorted = [...repoPaths].sort((a, b) => b.length - a.length);
    const repo = sorted.find((rp) => path.startsWith(rp + "/") || path === rp);
    if (!repo) return null;
    const relPath = path === repo ? "" : path.slice(repo.length + 1);
    return { repoPath: repo, relPath };
  }

  // Relative path — needs an active repo
  if (!activeRepoPath) return null;

  const absoluteResolved = normalizePath(activeRepoPath + "/" + path);

  // Guard: resolved path must stay within the repo root
  if (
    !absoluteResolved.startsWith(activeRepoPath + "/") &&
    absoluteResolved !== activeRepoPath
  ) {
    return null;
  }

  const relPath = absoluteResolved === activeRepoPath
    ? ""
    : absoluteResolved.slice(activeRepoPath.length + 1).replace(/\/+$/, "");

  return { repoPath: activeRepoPath, relPath };
}
