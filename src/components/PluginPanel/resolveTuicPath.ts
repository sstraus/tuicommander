import {
	isAbsolutePath,
	joinPath,
	normalizePathSegments,
	normalizeSep,
	pathStartsWith,
	pathStripPrefix,
} from "../../utils/pathUtils";

export interface ResolvedPath {
	repoPath: string;
	/** Path inside `repoPath`; the full absolute path when `external` is set. */
	relPath: string;
	/** Absolute path outside every registered repo; `repoPath` is only the tab owner. */
	external?: true;
}

const DOT_SEGMENT_RE = /(^|[\\/])\.\.?([\\/]|$)/;

/** Normalize a path: resolve . and .. segments, collapse multiple slashes, handle both separators. */
function normalizePath(p: string): string {
	const n = normalizeSep(p);
	const parts = n.split("/");
	const out: string[] = [];
	for (const seg of parts) {
		if (seg === "." || seg === "") continue;
		if (seg === "..") {
			out.pop();
			continue;
		}
		out.push(seg);
	}
	if (n.startsWith("/")) return "/" + out.join("/");
	return out.join("/");
}

/**
 * Resolve a path (absolute or relative) to a repo + relative-path pair.
 *
 * - Absolute paths are matched against the known repo list (longest match wins).
 *   One that matches no repo is an external file: `external` is set, `relPath`
 *   is the absolute path, and `repoPath` is the asking panel's repo (or "" with
 *   none) so a foreign focused repo never owns the tab.
 * - Relative paths are resolved against `baseRepoPath` — the repo the ASKING
 *   panel belongs to, not whichever repo happens to have focus. A panel showing
 *   one repo's content must not resolve its own links into another repo.
 * - Path traversal (../) that escapes the repo root returns null.
 */
export function resolveTuicPath(path: string, repoPaths: string[], baseRepoPath: string | null): ResolvedPath | null {
	if (!path) return null;

	if (isAbsolutePath(path)) {
		// Resolve dot segments first: "/repo/../etc/x" must not pass the prefix check.
		const abs = DOT_SEGMENT_RE.test(path) ? normalizePathSegments(path) : path;
		const sorted = repoPaths.toSorted((a, b) => b.length - a.length);
		const repo = sorted.find((rp) => pathStartsWith(abs, rp));
		if (!repo) return { repoPath: baseRepoPath ?? "", relPath: abs, external: true };
		return { repoPath: repo, relPath: pathStripPrefix(abs, repo)! };
	}

	if (!baseRepoPath) return null;

	const absoluteResolved = normalizePath(joinPath(baseRepoPath, path));

	if (!pathStartsWith(absoluteResolved, baseRepoPath)) {
		return null;
	}

	const relPath = pathStripPrefix(absoluteResolved, baseRepoPath)!;
	return { repoPath: baseRepoPath, relPath };
}
