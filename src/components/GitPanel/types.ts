/** Mirrors the Rust CommitLogEntry struct from git.rs */
export interface CommitLogEntry {
  hash: string;
  parents: string[];
  refs: string[];
  author_name: string;
  author_date: string;
  subject: string;
}

/** A single entry from the Rust StatusEntry struct */
export interface StatusEntry {
  path: string;
  status: string;
  original_path?: string | null;
  additions?: number;
  deletions?: number;
}

/** Full working tree status from `get_working_tree_status` */
export interface WorkingTreeStatus {
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  stash_count: number;
  staged: StatusEntry[];
  unstaged: StatusEntry[];
  untracked: string[];
}

/** Mirrors the Rust ChangedFile struct from git.rs */
export interface ChangedFile {
  path: string;
  status: string;
  additions: number;
  deletions: number;
}

/** Mirrors the Rust BranchDetail struct from git.rs */
export interface BranchDetail {
  name: string;
  is_current: boolean;
  is_remote: boolean;
  is_main: boolean;
  is_merged: boolean;
  ahead: number | null;
  behind: number | null;
  upstream: string | null;
  last_commit_date: string | null;
  last_commit_message: string | null;
  last_commit_author: string | null;
}
