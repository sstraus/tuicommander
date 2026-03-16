import { createSignal, onCleanup } from "solid-js";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
import { rpc } from "../transport";

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

/**
 * Hook for handling external file drag & drop via Tauri's native API.
 * Returns isDragging signal for overlay UI.
 */
export function useFileDrop() {
  const [isDragging, setIsDragging] = createSignal(false);

  const setup = getCurrentWebview().onDragDropEvent((event) => {
    const { type } = event.payload;

    if (type === "enter" || type === "over") {
      setIsDragging(true);
    } else if (type === "leave") {
      setIsDragging(false);
    } else if (type === "drop") {
      setIsDragging(false);

      const paths = event.payload.paths;
      if (!paths?.length) return;

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

      // No active PTY — open files in the appropriate tab
      for (const absolutePath of paths) {
        const [repoPath, filePath] = resolveRepoPaths(absolutePath);
        const fileType = classifyDroppedFile(absolutePath);

        if (fileType === "markdown") {
          mdTabsStore.add(repoPath, filePath);
        } else {
          editorTabsStore.add(repoPath, filePath);
        }
      }

      appLogger.info("app", `Opened ${paths.length} file(s) via drag & drop`);
    }
  });

  onCleanup(() => {
    setup.then((unlisten) => unlisten());
  });

  return { isDragging };
}
