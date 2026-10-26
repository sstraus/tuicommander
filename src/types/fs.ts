/** A directory entry returned by list_directory */
export interface DirEntry {
	name: string;
	/** Path relative to repo root, always using `/` as separator */
	path: string;
	is_dir: boolean;
	size: number;
	/** Last modification time as seconds since UNIX epoch */
	modified_at: number;
	/** Git status: "modified", "staged", "untracked", or "" (clean) */
	git_status: string;
	/** Whether the file is listed in .gitignore */
	is_ignored: boolean;
	/** Link text and target metadata; absent for ordinary directory entries. */
	symlink_target?: string;
	symlink_target_kind?: "directory" | "file" | "unavailable";
}

/** A single content match from full-text search */
export interface ContentMatch {
	path: string;
	line_number: number;
	line_text: string;
	/** UTF-16 code-unit offsets, directly consumable by JavaScript String.slice. */
	match_start: number;
	match_end: number;
	/** Set only by cross-repo search (`/fs/search-content-all`); absent for single-repo results. */
	repo_path?: string;
}

/** The whole result of a content search, as returned by the HTTP routes
 *  (`/fs/search-content`, `/fs/search-content-all`). The Tauri commands answer
 *  with a stream of `ContentSearchBatch` events instead. */
export interface ContentSearchResult {
	matches: ContentMatch[];
	files_searched: number;
	files_skipped: number;
	truncated: boolean;
	repos_pending: number;
	repos_indexing: number;
	repos_searched: number;
}

/** A batch of content search results, emitted progressively via events */
export interface ContentSearchBatch {
	/** Echoed from the request. The event is global and several panels listen at
	 *  once, so a batch belongs only to the panel whose id it carries. */
	search_id: string;
	matches: ContentMatch[];
	is_final: boolean;
	files_searched: number;
	files_skipped: number;
	truncated: boolean;
	/** Cross-repo search: registered repos that could not be searched (0 for single-repo).
	 *  NOT the same as "indexing" — most are waiting on a scheduling event that
	 *  may never come. See `repos_indexing`. */
	repos_pending: number;
	/** Subset of `repos_pending` with a build in flight right now. The only count
	 *  that justifies telling the user a retry will help. */
	repos_indexing: number;
	/** Cross-repo search: registered repos actually searched (0 for single-repo). */
	repos_searched: number;
}

/** A content search that failed, correlated the same way its batches are. */
export interface ContentSearchError {
	search_id: string;
	message: string;
}
