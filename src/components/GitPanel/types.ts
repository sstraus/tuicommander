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
