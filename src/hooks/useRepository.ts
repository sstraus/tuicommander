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
      appLogger.debug("git", "Failed to get diff stats", { path, err });
      return { additions: 0, deletions: 0 };
    }
  }

  /** Remove a worktree by branch name */
  async function removeWorktree(repoPath: string, branchName: string, deleteBranch: boolean): Promise<void> {
    await invoke("remove_worktree", { repoPath, branchName, deleteBranch });
  }

  /** Create a new worktree with a branch */
  async function createWorktree(baseRepo: string, branchName: string, createBranch?: boolean, baseRef?: string): Promise<{
    name: string;
    path: string;
    branch: string;
    base_repo: string;
  }> {
    return await invoke("create_worktree", { baseRepo, branchName, createBranch, baseRef });
  }

  /** Get worktree paths: branch name → worktree directory */
  async function getWorktreePaths(repoPath: string): Promise<Record<string, string>> {
    try {
      return await invoke<Record<string, string>>("get_worktree_paths", { repoPath });
    } catch (err) {
      appLogger.warn("git", `Failed to get worktree paths for ${repoPath}`, err);
      return {};
    }
  }

  /** Get list of changed files with status and stats */
  async function getChangedFiles(path: string, scope?: string): Promise<ChangedFile[]> {
    try {
      return await invoke<ChangedFile[]>("get_changed_files", { path, scope });
    } catch (err) {
      appLogger.error("git", "Failed to get changed files", err);
      return [];
    }
  }

  /** Get diff for a single file */
  async function getFileDiff(path: string, file: string, scope?: string, untracked?: boolean): Promise<string> {
    try {
      return await invoke<string>("get_file_diff", { path, file, scope, untracked: untracked || undefined });
    } catch (err) {
      appLogger.error("git", "Failed to get file diff", err);
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
      appLogger.error("git", "Failed to list markdown files", err);
      return [];
    }
  }

  /** Read file content */
  async function readFile(path: string, file: string): Promise<string> {
    try {
      return await invoke<string>("read_file", { path, file });
    } catch (err) {
      appLogger.error("git", "Failed to read file", err);
      return "";
    }
  }

  /** Generate a unique worktree branch name, avoiding collisions with existing names */
  async function generateWorktreeName(existingNames: string[]): Promise<string> {
    return await invoke<string>("generate_worktree_name_cmd", { existingNames });
  }

  /** Generate a hybrid clone branch name: `{sanitized_source}--{random_name}` */
  async function generateCloneBranchName(sourceBranch: string, existingNames: string[]): Promise<string> {
    return await invoke<string>("generate_clone_branch_name_cmd", { sourceBranch, existingNames });
  }

  /** List base ref options for the create worktree dropdown (default branch first) */
  async function listBaseRefOptions(repoPath: string): Promise<string[]> {
    try {
      return await invoke<string[]>("list_base_ref_options", { repoPath });
    } catch (err) {
      appLogger.error("git", `Failed to list base ref options for ${repoPath}`, err);
      return [];
    }
  }

  /** Result of merge-and-archive operation */
  interface MergeArchiveResult {
    merged: boolean;
    action: string;
    archive_path: string | null;
  }

  /** Merge a worktree branch into target, then archive or delete */
  async function mergeAndArchiveWorktree(
    repoPath: string,
    branchName: string,
    targetBranch: string,
    afterMerge: string,
  ): Promise<MergeArchiveResult> {
    return await invoke<MergeArchiveResult>("merge_and_archive_worktree", {
      repoPath,
      branchName,
      targetBranch,
      afterMerge,
    });
  }

  /** Finalize a pending merge by archiving or deleting the worktree */
  async function finalizeMergedWorktree(
    repoPath: string,
    branchName: string,
    action: "archive" | "delete",
  ): Promise<MergeArchiveResult> {
    return await invoke<MergeArchiveResult>("finalize_merged_worktree", {
      repoPath,
      branchName,
      action,
    });
  }

  /** Recent commit entry */
  interface RecentCommit {
    hash: string;
    short_hash: string;
    subject: string;
  }

  /** Get branches fully merged into the repo's main branch */
  async function getMergedBranches(repoPath: string): Promise<string[]> {
    try {
      return await invoke<string[]>("get_merged_branches", { path: repoPath });
    } catch (err) {
      appLogger.warn("git", `Failed to get merged branches for ${repoPath}`, err);
      return [];
    }
  }

  /** Result of branch switch operation */
  interface SwitchBranchResult {
    success: boolean;
    stashed: boolean;
    previous_branch: string;
    new_branch: string;
  }

  /** Switch the main worktree to a different branch via git checkout.
   *  Runs in Rust (no PTY involvement) — safe even with editors open.
   *  Returns "dirty" error string when working tree has uncommitted changes. */
  async function switchBranch(
    repoPath: string,
    branchName: string,
    opts?: { force?: boolean; stash?: boolean },
  ): Promise<SwitchBranchResult> {
    return await invoke<SwitchBranchResult>("switch_branch", {
      repoPath,
      branchName,
      force: opts?.force ?? false,
      stash: opts?.stash ?? false,
    });
  }

  /** Check out a remote-only branch as a new local branch tracking origin. */
  async function checkoutRemoteBranch(repoPath: string, branchName: string): Promise<void> {
    await invoke("checkout_remote_branch", { repoPath, branchName });
  }

  /** Detect linked worktrees in detached HEAD state (branch was deleted). */
  async function detectOrphanWorktrees(repoPath: string): Promise<string[]> {
    try {
      return await invoke<string[]>("detect_orphan_worktrees", { repoPath });
    } catch (err) {
      appLogger.error("git", "Failed to detect orphan worktrees", err);
      return [];
    }
  }

  /** Remove a detached-HEAD worktree by path (no branch to look up). */
  async function removeOrphanWorktree(repoPath: string, worktreePath: string): Promise<void> {
    await invoke("remove_orphan_worktree", { repoPath, worktreePath });
  }

  /** Merge a PR via GitHub REST API. merge_method: "merge" | "squash" | "rebase" */
  async function mergePrViaGithub(repoPath: string, prNumber: number, mergeMethod: string): Promise<string> {
    return await invoke<string>("merge_pr_via_github", { repoPath, prNumber, mergeMethod });
  }

  /** List local branch names for a repository */
  async function listLocalBranches(repoPath: string): Promise<string[]> {
    try {
      return await invoke<string[]>("list_local_branches", { repoPath });
    } catch (err) {
      appLogger.error("git", "Failed to list local branches", err);
      return [];
    }
  }

  /** Get recent commits for a repository */
  async function getRecentCommits(path: string, count?: number): Promise<RecentCommit[]> {
    try {
      return await invoke<RecentCommit[]>("get_recent_commits", { path, count });
    } catch (err) {
      appLogger.error("git", "Failed to get recent commits", err);
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
    generateCloneBranchName,
    listBaseRefOptions,
    mergeAndArchiveWorktree,
    finalizeMergedWorktree,
    getMergedBranches,
    checkoutRemoteBranch,
    detectOrphanWorktrees,
    removeOrphanWorktree,
    mergePrViaGithub,
    listLocalBranches,
    switchBranch,
    getRecentCommits,
  };
}
