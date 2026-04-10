import { createSignal, onCleanup } from "solid-js";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
import { rpc, isTauri } from "../transport";

/**
 * Global flag to suppress the file-drop overlay during internal drags
 * (tab reorder, sidebar repo drag, task queue drag, etc.).
 * Internal drag handlers set this to true on dragstart and false on dragend.
 */
let internalDragCount = 0;
export function markInternalDragStart(): void { internalDragCount++; }
export function markInternalDragEnd(): void { internalDragCount = Math.max(0, internalDragCount - 1); }

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
  return ["", absolutePath];
}

/** Tauri augments File with a non-standard `path` field containing the absolute OS path. */
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
      appLogger.warn("app", "Dropped file missing .path in Tauri context, using name", { name: file.name });
      paths.push(file.name);
    } else {
      paths.push(file.name);
    }
  }
  return paths;
}

/** Open a list of absolute file paths: write to active PTY if any, else open tabs. */
function openDroppedPaths(paths: string[]) {
  if (!paths.length) return;

  const active = terminalsStore.getActive();
  if (active?.sessionId && terminalsStore.state.activeId) {
    const joined = paths.join(" ");
    rpc("write_pty", { sessionId: active.sessionId, data: joined }).catch((err) => {
      appLogger.error("terminal", "Failed to write dropped file paths", err);
    });
    return;
  }

  for (const filePath of paths) {
    const [repoPath, relPath] = resolveRepoPaths(filePath);
    const fileType = classifyDroppedFile(filePath);
    if (fileType === "markdown") {
      mdTabsStore.add(repoPath, relPath);
    } else {
      editorTabsStore.add(repoPath, relPath);
    }
  }

  appLogger.info("app", `Opened ${paths.length} file(s) via drag & drop`);
}

/**
 * Hook for handling external file drag & drop onto the terminal area.
 *
 * Uses HTML5 drag events scoped to a container element, so internal drags
 * (sidebar repos, tab reorder) are never intercepted.
 *
 * Requires `dragDropEnabled: false` in tauri.conf.json — Tauri's native
 * drag handler intercepts all OS-level drags and breaks internal HTML5 DnD.
 * Absolute file paths are still available via Tauri's File.path extension
 * on the standard HTML5 File object.
 *
 * Call `attachTo(el)` with the container element to bind listeners.
 * Returns isDragging signal for overlay UI.
 */
export function useFileDrop() {
  const [isDragging, setIsDragging] = createSignal(false);

  /** Only react to drags that carry files (not internal tab/repo drags). */
  const hasFiles = (e: DragEvent) => e.dataTransfer?.types?.includes("Files") ?? false;

  const onDragOver = (e: DragEvent) => {
    if (!hasFiles(e) || internalDragCount > 0) return;
    e.preventDefault();
    if (!isDragging()) setIsDragging(true);
  };

  const onDragLeave = (e: DragEvent) => {
    if (!hasFiles(e)) return;
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
    openDroppedPaths(extractPaths(files));
  };

  let boundEl: HTMLElement | null = null;

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
