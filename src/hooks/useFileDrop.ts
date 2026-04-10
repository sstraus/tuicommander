import { createSignal, onCleanup } from "solid-js";
import { getCurrentWebview } from "@tauri-apps/api/webview";
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

/** Open a list of absolute file paths: write to active PTY if any, else open tabs. */
function openDroppedPaths(paths: string[]) {
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
 * Hook for handling external file drag & drop.
 *
 * In Tauri mode: uses the native `onDragDropEvent` API which provides
 * absolute OS paths (requires `dragDropEnabled: true` in tauri.conf.json).
 *
 * In browser mode: falls back to HTML5 drag events with bare filenames.
 *
 * Call `attachTo(el)` with the container element — in browser mode this
 * scopes the HTML5 listeners; in Tauri mode it's a no-op target (overlay
 * still shows window-wide since Tauri intercepts OS-level drops).
 *
 * Returns isDragging signal for overlay UI.
 */
export function useFileDrop() {
  const [isDragging, setIsDragging] = createSignal(false);

  if (isTauri()) {
    // Tauri native API — provides absolute paths via onDragDropEvent
    const setup = getCurrentWebview().onDragDropEvent((event) => {
      const { type } = event.payload;
      if (type === "enter" || type === "over") {
        if (!isDragging()) setIsDragging(true);
      } else if (type === "leave") {
        setIsDragging(false);
      } else if (type === "drop") {
        setIsDragging(false);
        const paths = event.payload.paths;
        if (paths?.length) openDroppedPaths(paths);
      }
    });

    onCleanup(() => {
      setup.then((unlisten) => unlisten()).catch(() => {});
    });

    // In Tauri mode, attachTo is a no-op — Tauri captures OS drops window-wide
    return { isDragging, attachTo: (_el: HTMLElement) => {} };
  }

  // Browser mode: HTML5 drag events (filenames only, no absolute paths)
  const hasFiles = (e: DragEvent) => e.dataTransfer?.types?.includes("Files") ?? false;

  const onDragOver = (e: DragEvent) => {
    if (!hasFiles(e)) return;
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

    // In browser mode only bare filenames are available
    const paths: string[] = [];
    for (const file of files) paths.push(file.name);
    openDroppedPaths(paths);
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
