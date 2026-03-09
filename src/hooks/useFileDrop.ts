import { createSignal, onCleanup } from "solid-js";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { repositoriesStore } from "../stores/repositories";

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
 * Hook for handling external file drag & drop onto a container element.
 * Returns isDragging signal and a ref callback to attach to the drop zone.
 */
export function useFileDrop() {
  const [isDragging, setIsDragging] = createSignal(false);
  let dragCounter = 0;
  let containerEl: HTMLElement | null = null;

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    if (e.dataTransfer) {
      e.dataTransfer.dropEffect = "copy";
    }
  };

  const handleDragEnter = (e: DragEvent) => {
    e.preventDefault();
    dragCounter++;
    if (dragCounter === 1) {
      setIsDragging(true);
    }
  };

  const handleDragLeave = (e: DragEvent) => {
    e.preventDefault();
    dragCounter--;
    if (dragCounter === 0) {
      setIsDragging(false);
    }
  };

  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    dragCounter = 0;
    setIsDragging(false);

    const files = e.dataTransfer?.files;
    if (!files?.length) return;

    const paths = Array.from(files)
      .map((f) => (f as unknown as { path?: string }).path)
      .filter((p): p is string => Boolean(p));

    for (const absolutePath of paths) {
      const [repoPath, filePath] = resolveRepoPaths(absolutePath);
      const type = classifyDroppedFile(absolutePath);

      if (type === "markdown") {
        mdTabsStore.add(repoPath, filePath);
      } else {
        editorTabsStore.add(repoPath, filePath);
      }
    }
  };

  const attachTo = (el: HTMLElement) => {
    containerEl = el;
    el.addEventListener("dragover", handleDragOver);
    el.addEventListener("dragenter", handleDragEnter);
    el.addEventListener("dragleave", handleDragLeave);
    el.addEventListener("drop", handleDrop);
  };

  onCleanup(() => {
    if (containerEl) {
      containerEl.removeEventListener("dragover", handleDragOver);
      containerEl.removeEventListener("dragenter", handleDragEnter);
      containerEl.removeEventListener("dragleave", handleDragLeave);
      containerEl.removeEventListener("drop", handleDrop);
    }
  });

  return { isDragging, attachTo };
}
