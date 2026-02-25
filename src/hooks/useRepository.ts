import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import type { RepoInfo } from "../types";

/** Changed file information for diff browser */
export interface ChangedFile {
  path: string;
  status: string; // "M" | "A" | "D" | "R"
  additions: number;
  deletions: number;
}

/** Repository hook for git operations */
export function useRepository() {
  /** Get repository info */
  async function getInfo(path: string): Promise<RepoInfo> {
    return await invoke<RepoInfo>("get_repo_info", { path });
  }

  /** Get git diff for a repository */
  async function getDiff(path: string, scope?: string): Promise<string> {
    return await invoke<string>("get_git_diff", { path, scope });
  }

  /** Open a path in an application, optionally at a specific line/col */
  async function openInApp(path: string, app: string, line?: number, col?: number): Promise<void> {
    await invoke("open_in_app", { path, app, line, col });
  }

  /** Rename a git branch */
  async function renameBranch(repoPath: string, oldName: string, newName: string): Promise<void> {
    await invoke("rename_branch", { path: repoPath, oldName, newName });
  }

  /** Get diff stats (additions/deletions) for a repository */
  async function getDiffStats(path: string, scope?: string): Promise<{ additions: number; deletions: number }> {
    try {
      return await invoke<{ additions: number; deletions: number }>("get_diff_stats", { path, scope });
    } catch (err) {
      console.debug("Failed to get diff stats:", { path, err });
      return { additions: 0, deletions: 0 };
    }
  }

  /** Remove a worktree by branch name */
  async function removeWorktree(repoPath: string, branchName: string): Promise<void> {
    await invoke("remove_worktree", { repoPath, branchName });
  }

  /** Create a new worktree with a branch */
  async function createWorktree(baseRepo: string, branchName: string, createBranch?: boolean): Promise<{
    name: string;
    path: string;
    branch: string;
    base_repo: string;
  }> {
    return await invoke("create_worktree", { baseRepo, branchName, createBranch });
  }

  /** Get worktree paths: branch name → worktree directory */
  async function getWorktreePaths(repoPath: string): Promise<Record<string, string>> {
    try {
      return await invoke<Record<string, string>>("get_worktree_paths", { repoPath });
    } catch (err) {
      appLogger.error("git", `Failed to get worktree paths for ${repoPath}`, err);
      return {};
    }
  }

  /** Get list of changed files with status and stats */
  async function getChangedFiles(path: string, scope?: string): Promise<ChangedFile[]> {
    try {
      return await invoke<ChangedFile[]>("get_changed_files", { path, scope });
    } catch (err) {
      console.error("Failed to get changed files:", err);
      return [];
    }
  }

  /** Get diff for a single file */
  async function getFileDiff(path: string, file: string, scope?: string): Promise<string> {
    try {
      return await invoke<string>("get_file_diff", { path, file, scope });
    } catch (err) {
      console.error("Failed to get file diff:", err);
      return "";
    }
  }

  /** Markdown file entry with git status */
  interface MarkdownFileEntry {
    path: string;
    git_status: string; // "modified" | "staged" | "untracked" | ""
    is_ignored: boolean;
  }

  /** List all markdown files in repository with git status */
  async function listMarkdownFiles(path: string): Promise<MarkdownFileEntry[]> {
    try {
      return await invoke<MarkdownFileEntry[]>("list_markdown_files", { path });
    } catch (err) {
      console.error("Failed to list markdown files:", err);
      return [];
    }
  }

  /** Read file content */
  async function readFile(path: string, file: string): Promise<string> {
    try {
      return await invoke<string>("read_file", { path, file });
    } catch (err) {
      console.error("Failed to read file:", err);
      return "";
    }
  }

  /** Generate a unique worktree branch name, avoiding collisions with existing names */
  async function generateWorktreeName(existingNames: string[]): Promise<string> {
    return await invoke<string>("generate_worktree_name_cmd", { existingNames });
  }

  /** Recent commit entry */
  interface RecentCommit {
    hash: string;
    short_hash: string;
    subject: string;
  }

  /** List local branch names for a repository */
  async function listLocalBranches(repoPath: string): Promise<string[]> {
    try {
      return await invoke<string[]>("list_local_branches", { repoPath });
    } catch (err) {
      console.error("Failed to list local branches:", err);
      return [];
    }
  }

  /** Get recent commits for a repository */
  async function getRecentCommits(path: string, count?: number): Promise<RecentCommit[]> {
    try {
      return await invoke<RecentCommit[]>("get_recent_commits", { path, count });
    } catch (err) {
      console.error("Failed to get recent commits:", err);
      return [];
    }
  }

  return {
    getInfo,
    getDiff,
    getDiffStats,
    openInApp,
    renameBranch,
    removeWorktree,
    createWorktree,
    getWorktreePaths,
    getChangedFiles,
    getFileDiff,
    listMarkdownFiles,
    readFile,
    generateWorktreeName,
    listLocalBranches,
    getRecentCommits,
  };
}
