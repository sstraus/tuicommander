import type { DirEntry } from "../../types/fs";
import { formatRelativeTime } from "../../utils/time";
import g from "../shared/git-status.module.css";

/** Hover tooltip for a file/dir row: relative path plus last-modified time. */
export function fileTooltip(entry: DirEntry): string {
	// modified_at is seconds since the epoch (see DirEntry); formatRelativeTime wants ms.
	if (!entry.modified_at) return entry.path;
	return `${entry.path}\nModified ${formatRelativeTime(entry.modified_at * 1000)}`;
}

/** Map git_status string to CSS class for status dot */
export function getStatusClass(status: string): string {
	switch (status) {
		case "modified":
			return g.modified;
		case "staged":
			return g.staged;
		case "untracked":
			return g.untracked;
		default:
			return "";
	}
}

/** Format file size for display */
export function formatSize(bytes: number): string {
	if (bytes === 0) return "";
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
