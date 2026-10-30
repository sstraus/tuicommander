import { convertFileSrc } from "@tauri-apps/api/core";
import { locateFile } from "../stores/repositories";
import { isTauri } from "../transport";
import { getRemoteBaseUrl, getRepoConnection, withRemoteToken } from "../transportRuntime";
import { isAbsolutePath, joinPath, pathBasename, pathDirname } from "./pathUtils";

/** Browser URL of a repository image, served by the authenticated `/fs/markdown-image` route. */
export function repoImageUrl(repoPath: string, file: string, ownerRepoPath = repoPath): string {
	const connectionId = getRepoConnection(ownerRepoPath);
	const base = connectionId ? getRemoteBaseUrl(connectionId) : undefined;
	const url = new URL("/fs/markdown-image", base ?? window.location.origin);
	url.searchParams.set("repoPath", repoPath);
	url.searchParams.set("file", file);
	return withRemoteToken(url.toString(), connectionId);
}

/** Resolve browser assets against registered roots, including linked worktrees. */
export function browserLocalImageUrl(path: string, localFileTab = false): string | null {
	const location = locateFile(path);
	if (location.fsRoot) return repoImageUrl(location.fsRoot, location.filePath, location.repoPath);
	// An explicitly opened local image tab already names its source directory.
	// The existing HTTP image route still authenticates and confines the image to it.
	return localFileTab && isAbsolutePath(path) ? repoImageUrl(pathDirname(path), pathBasename(path)) : null;
}

/** The serialized `src` attribute back to the filesystem filename: HTML entities first, then URL escapes. */
function decodeImagePath(attribute: string): string {
	const text = document.createElement("textarea");
	text.innerHTML = attribute;
	return text.value.replace(/(?:%[0-9a-f]{2})+/gi, (escaped) => {
		try {
			return decodeURIComponent(escaped);
		} catch {
			return escaped; // malformed escape: keep as written
		}
	});
}

/** Keep unavailable local images visible without issuing a doomed browser request. */
export function rewriteLocalImages(html: string, baseDir?: string, imageSrc?: (path: string) => string): string {
	if (isTauri() && !baseDir && !imageSrc) return html;
	return html.replace(
		/<img\b[^>]*\ssrc=(?:"([^"]*)"|'([^']*)'|([^\s>]+))[^>]*>/gi,
		(tag, doubleQuoted: string | undefined, singleQuoted: string | undefined, unquoted: string | undefined) => {
			const path = decodeImagePath(doubleQuoted ?? singleQuoted ?? unquoted ?? "");
			if (/^(https?:\/\/|\/\/|data:|asset:\/\/)/i.test(path)) return tag;
			const local = isAbsolutePath(path) ? path : baseDir ? joinPath(baseDir, path) : path;
			const src = imageSrc ? imageSrc(path) : isTauri() ? convertFileSrc(local) : browserLocalImageUrl(local);
			if (!src || (!isTauri() && !/^(https?:\/\/|\/\/|data:)/i.test(src))) {
				return '<span role="img" aria-label="Image unavailable">Image unavailable: outside open repositories.</span>';
			}
			return tag.replace(/\ssrc=(?:"[^"]*"|'[^']*'|[^\s>]+)/i, () => ` src="${src}"`);
		},
	);
}
