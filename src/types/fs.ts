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
}

/** A single content match from full-text search */
export interface ContentMatch {
  path: string;
  line_number: number;
  line_text: string;
  match_start: number;
  match_end: number;
}

/** A batch of content search results, emitted progressively via events */
export interface ContentSearchBatch {
  matches: ContentMatch[];
  is_final: boolean;
  files_searched: number;
  files_skipped: number;
  truncated: boolean;
}
