import { invoke } from "../invoke";
import type { DirEntry } from "../types/fs";

/** Hook wrapping Rust fs commands for the file browser */
export function useFileBrowser() {
  async function listDirectory(repoPath: string, subdir: string): Promise<DirEntry[]> {
    return await invoke<DirEntry[]>("list_directory", { repoPath, subdir });
  }

  async function searchFiles(repoPath: string, query: string, limit?: number): Promise<DirEntry[]> {
    return await invoke<DirEntry[]>("search_files", { repoPath, query, limit: limit ?? 200 });
  }

  async function readFile(repoPath: string, file: string): Promise<string> {
    return await invoke<string>("fs_read_file", { repoPath, file });
  }

  async function writeFile(repoPath: string, file: string, content: string): Promise<void> {
    await invoke("write_file", { repoPath, file, content });
  }

  async function createDirectory(repoPath: string, dir: string): Promise<void> {
    await invoke("create_directory", { repoPath, dir });
  }

  async function deletePath(repoPath: string, path: string): Promise<void> {
    await invoke("delete_path", { repoPath, path });
  }

  async function renamePath(repoPath: string, from: string, to: string): Promise<void> {
    await invoke("rename_path", { repoPath, from, to });
  }

  async function copyPath(repoPath: string, from: string, to: string): Promise<void> {
    await invoke("copy_path", { repoPath, from, to });
  }

  async function addToGitignore(repoPath: string, pattern: string): Promise<void> {
    await invoke("add_to_gitignore", { repoPath, pattern });
  }

  return {
    listDirectory,
    searchFiles,
    readFile,
    writeFile,
    createDirectory,
    deletePath,
    renamePath,
    copyPath,
    addToGitignore,
  };
}
