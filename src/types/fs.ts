/** A directory entry returned by list_directory */
export interface DirEntry {
  name: string;
  /** Path relative to repo root, always using `/` as separator */
  path: string;
  is_dir: boolean;
  size: number;
  /** Git status: "modified", "staged", "untracked", or "" (clean) */
  git_status: string;
  /** Whether the file is listed in .gitignore */
  is_ignored: boolean;
}
