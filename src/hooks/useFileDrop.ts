import { createSignal, onCleanup } from "solid-js";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
import { rpc, isTauri } from "../transport";

/** Markdown extensions (case-insensitive) */
const MD_EXTENSIONS = new Set([".md", ".mdx"]);

/** Classify a file path as markdown or editor based on extension */
export function classifyDroppedFile(filePath: string): "markdown" | "editor" {
  const dotIndex = filePath.lastIndexOf(".");
  if (dotIndex === -1) return "editor";
  const ext = filePath.slice(dotIndex).toLowerCase();
  return MD_EXTENSIONS.has(ext) ? "markdown" : "editor";
}

/**
 * Resolve repoPath and relative filePath for a dropped absolute path.
 * If the file is inside the active repo, returns [repoPath, relativePath].
 * Otherwise returns ["", absolutePath] for standalone opening.
 */
function resolveRepoPaths(absolutePath: string): [repoPath: string, filePath: string] {
  const activeRepo = repositoriesStore.state.activeRepoPath;
  if (activeRepo) {
    const prefix = activeRepo.endsWith("/") ? activeRepo : activeRepo + "/";
    if (absolutePath.startsWith(prefix)) {
      return [activeRepo, absolutePath.slice(prefix.length)];
    }
  }
  // File outside any active repo — standalone tab with absolute path
  return ["", absolutePath];
}

/** Tauri augments File with a non-standard `path` field containing the absolute path. */
interface TauriFile extends File {
  readonly path?: string;
}

/** Extract file paths from a drop event's FileList.
 *  In Tauri webviews, File.path provides the absolute path.
 *  In browsers, only File.name (bare filename) is available. */
function extractPaths(files: FileList): string[] {
  const paths: string[] = [];
  for (const file of files) {
    const absPath = (file as TauriFile).path;
    if (absPath) {
      paths.push(absPath);
    } else if (isTauri()) {
      appLogger.warn("app", "Dropped file missing .path in Tauri context, skipping", { name: file.name });
    } else {
      paths.push(file.name);
    }
  }
  return paths;
}

/**
 * Hook for handling external file drag & drop onto the terminal area.
 * Uses HTML5 drag events scoped to a container element, so internal drags
 * (sidebar repos, tab reorder) are never intercepted.
 *
 * Requires `dragDropEnabled: false` in tauri.conf.json — Tauri's native
 * drag handler intercepts all OS-level drags and breaks internal HTML5 DnD.
 * File paths are still available via Tauri's File.path extension.
 *
 * Call `attachTo(el)` with the container element to bind listeners.
 * Returns isDragging signal for overlay UI.
 */
export function useFileDrop() {
  const [isDragging, setIsDragging] = createSignal(false);

  /** Only intercept drags that contain external files */
  const hasFiles = (e: DragEvent) => e.dataTransfer?.types?.includes("Files") ?? false;

  const onDragOver = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    if (!isDragging()) setIsDragging(true);
  };

  const onDragLeave = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    // Only reset when leaving the container itself, not its children
    const container = e.currentTarget as HTMLElement;
    const related = e.relatedTarget as Node | null;
    if (related && container.contains(related)) return;
    setIsDragging(false);
  };

  const onDrop = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    setIsDragging(false);

    const files = e.dataTransfer?.files;
    if (!files?.length) return;

    const paths = extractPaths(files);
    if (!paths.length) return;

    // If the active terminal has a PTY session, write paths there
    // so running processes (Claude Code, etc.) can reference them.
    const active = terminalsStore.getActive();
    if (active?.sessionId && terminalsStore.state.activeId) {
      const joined = paths.join(" ");
      rpc("write_pty", { sessionId: active.sessionId, data: joined }).catch((err) => {
        appLogger.error("terminal", "Failed to write dropped file paths", err);
      });
      return;
    }

    // No active PTY — open files in the appropriate tab.
    // In Tauri, paths are absolute so we resolve repo-relative paths.
    // In browser, only filenames are available — open as standalone tabs.
    const hasTauriPaths = isTauri();
    for (const filePath of paths) {
      const [repoPath, relPath] = hasTauriPaths ? resolveRepoPaths(filePath) : ["", filePath];
      const fileType = classifyDroppedFile(filePath);
      if (fileType === "markdown") {
        mdTabsStore.add(repoPath, relPath);
      } else {
        editorTabsStore.add(repoPath, relPath);
      }
    }

    appLogger.info("app", `Opened ${paths.length} file(s) via drag & drop`);
  };

  let boundEl: HTMLElement | null = null;

  /** Attach drag & drop listeners to a container element */
  function attachTo(el: HTMLElement) {
    if (boundEl) {
      boundEl.removeEventListener("dragover", onDragOver);
      boundEl.removeEventListener("dragleave", onDragLeave);
      boundEl.removeEventListener("drop", onDrop);
    }
    boundEl = el;
    el.addEventListener("dragover", onDragOver);
    el.addEventListener("dragleave", onDragLeave);
    el.addEventListener("drop", onDrop);
  }

  onCleanup(() => {
    if (boundEl) {
      boundEl.removeEventListener("dragover", onDragOver);
      boundEl.removeEventListener("dragleave", onDragLeave);
      boundEl.removeEventListener("drop", onDrop);
    }
  });

  return { isDragging, attachTo };
}
