import { createEffect, createSignal, onCleanup } from "solid-js";
import { mdTabsStore } from "../stores/mdTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
import { toastsStore } from "../stores/toasts";
import { dragDropStore, clearDropPayload } from "../stores/dragDrop";
import { rpc, isTauri } from "../transport";
import { invoke } from "../invoke";
import { classifyFile } from "../utils/filePreview";

/**
 * Global flag to suppress the file-drop overlay during internal drags
 * (tab reorder, sidebar repo drag, task queue drag, etc.).
 * Internal drag handlers set this to true on dragstart and false on dragend.
 */
let internalDragCount = 0;
export function markInternalDragStart(): void { internalDragCount++; }
export function markInternalDragEnd(): void { internalDragCount = Math.max(0, internalDragCount - 1); }

/** Classify a file path for opening — delegates to shared utility.
 *  @deprecated Use classifyFile from utils/filePreview instead */
export function classifyDroppedFile(filePath: string): "markdown" | "editor" {
  const target = classifyFile(filePath);
  return target === "markdown" ? "markdown" : "editor";
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

/**
 * Quote a path for shell consumption when writing to a PTY.
 * Uses single quotes and escapes embedded single quotes as `'\''`.
 * Needed because absolute paths may contain spaces, parens, etc.
 */
function shellQuote(path: string): string {
  return `'${path.replace(/'/g, "'\\''")}'`;
}

/** Write absolute paths to the active terminal as space-separated shell-quoted tokens. */
function writePathsToTerminal(paths: string[]): boolean {
  const active = terminalsStore.getActive();
  if (!active?.sessionId || !terminalsStore.state.activeId) return false;
  const joined = paths.map(shellQuote).join(" ");
  rpc("write_pty", { sessionId: active.sessionId, data: joined }).catch((err) => {
    appLogger.error("terminal", "Failed to write dropped file paths", err);
  });
  return true;
}

/** Open absolute paths as editor / markdown tabs. */
function openPathsAsTabs(paths: string[]): void {
  for (const filePath of paths) {
    const [repoPath, relPath] = resolveRepoPaths(filePath);
    const target = classifyFile(filePath);
    if (target === "markdown") {
      mdTabsStore.add(repoPath, relPath);
    } else if (target === "preview") {
      mdTabsStore.addHtmlPreview(repoPath, relPath);
    } else {
      editorTabsStore.add(repoPath, relPath);
    }
  }
  appLogger.info("app", `Opened ${paths.length} file(s) via drag & drop`);
}

/**
 * Hit-test the element under a physical-pixel coordinate from a Tauri drop event.
 * Tauri's `position` is in *physical* pixels; `elementFromPoint` needs CSS pixels.
 * The ratio is `window.devicePixelRatio`.
 */
function elementAtDropPoint(physicalX: number, physicalY: number): Element | null {
  const dpr = window.devicePixelRatio || 1;
  return document.elementFromPoint(physicalX / dpr, physicalY / dpr);
}

/**
 * Walk up from `el` looking for an ancestor declaring `data-drop-target`.
 * Returns the element and its associated data (absolute path for folder drops).
 */
interface DropTargetInfo {
  kind: "folder";
  absPath: string;
}
function findDropTarget(el: Element | null): DropTargetInfo | null {
  let cur: Element | null = el;
  while (cur) {
    const target = (cur as HTMLElement).dataset?.dropTarget;
    if (target === "folder") {
      const absPath = (cur as HTMLElement).dataset.absPath;
      if (absPath) return { kind: "folder", absPath };
    }
    cur = cur.parentElement;
  }
  return null;
}

export interface FolderDropRequest {
  destDir: string;
  paths: string[];
  mode: "move" | "copy";
}

interface FolderDropOpts {
  /** Called when the transfer requires user confirmation (directory, not yet authorized). */
  onNeedsConfirm: (req: FolderDropRequest) => void;
  /** Whether recursive directory transfer is already authorized. */
  allowRecursive: boolean;
}

/**
 * Invoke `fs_transfer_paths` and surface the result via toasts.
 * Returns true if the transfer was attempted (i.e., not deferred for confirm).
 */
async function executeFolderDrop(req: FolderDropRequest, opts: FolderDropOpts): Promise<boolean> {
  try {
    const result = await invoke<{
      moved: number;
      skipped: number;
      errors: string[];
      needs_confirm: boolean;
    }>("fs_transfer_paths", {
      destDir: req.destDir,
      paths: req.paths,
      mode: req.mode,
      allowRecursive: opts.allowRecursive,
    });

    if (result.needs_confirm) {
      opts.onNeedsConfirm(req);
      return false;
    }

    const verb = req.mode === "move" ? "Moved" : "Copied";
    const parts: string[] = [];
    if (result.moved > 0) parts.push(`${verb} ${result.moved}`);
    if (result.skipped > 0) parts.push(`skipped ${result.skipped}`);
    if (result.errors.length > 0) parts.push(`${result.errors.length} error(s)`);
    const level: "info" | "warn" | "error" = result.errors.length > 0 ? "warn" : "info";
    toastsStore.add(
      verb,
      parts.join(" · ") || "Nothing to do",
      level,
    );
    if (result.errors.length > 0) {
      appLogger.warn("app", "Drop transfer errors", { errors: result.errors });
    }
    return true;
  } catch (err) {
    appLogger.error("app", "fs_transfer_paths failed", err);
    toastsStore.add("Transfer failed", String(err), "error");
    return true;
  }
}

/** Callback type for pending folder drop confirmation (directory requires allow_recursive). */
type PendingConfirmHandler = (req: FolderDropRequest) => void;
let pendingConfirmHandler: PendingConfirmHandler | null = null;
/** Registered by a confirm dialog host (e.g., App.tsx) at mount time. */
export function setFolderDropConfirmHandler(handler: PendingConfirmHandler | null): void {
  pendingConfirmHandler = handler;
}

/** Dispatch a Tauri drop payload: either folder transfer, terminal paste, or tabs. */
async function dispatchTauriDrop(paths: string[], x: number, y: number): Promise<void> {
  if (!paths.length) return;

  const el = elementAtDropPoint(x, y);
  const target = findDropTarget(el);

  if (target?.kind === "folder") {
    const mode: "move" | "copy" = dragDropStore.copyModifierHeld() ? "copy" : "move";
    await executeFolderDrop(
      { destDir: target.absPath, paths, mode },
      {
        allowRecursive: false,
        onNeedsConfirm: (req) => {
          if (pendingConfirmHandler) pendingConfirmHandler(req);
          else appLogger.warn("app", "Folder drop needs confirm but no handler registered");
        },
      },
    );
    return;
  }

  // Fallback: if a terminal exists, paste absolute paths into it; else open as tabs.
  if (writePathsToTerminal(paths)) return;
  openPathsAsTabs(paths);
}

/** Confirmation follow-up: re-run transfer with allow_recursive=true. */
export async function confirmFolderDrop(req: FolderDropRequest): Promise<void> {
  await executeFolderDrop(req, {
    allowRecursive: true,
    onNeedsConfirm: () => {
      /* already confirmed */
    },
  });
}

/**
 * Global dispatcher: subscribes to `dragDropStore.dropPayload` and routes each
 * Tauri drop event. Registered once at module load.
 */
let dispatcherRegistered = false;
function registerGlobalDispatcher() {
  if (dispatcherRegistered || !isTauri()) return;
  dispatcherRegistered = true;

  // Use a raw effect at module scope — SolidJS runs it reactively.
  // We still need a root to avoid "computations created outside a createRoot" warnings.
  import("solid-js").then(({ createRoot }) => {
    createRoot(() => {
      createEffect(() => {
        const payload = dragDropStore.dropPayload();
        if (!payload) return;
        // Clear immediately so rapid successive drops each fire the effect anew.
        clearDropPayload();
        void dispatchTauriDrop(payload.paths, payload.position.x, payload.position.y);
      });
    });
  });
}
registerGlobalDispatcher();

/**
 * Hook for the terminal-area drag overlay.
 *
 * Only manages the *visual* state ("drop files here" highlight) via HTML5 drag
 * events scoped to the container. Actual path extraction and routing happens
 * globally via the Tauri onDragDropEvent listener — this hook does not need
 * access to file paths.
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
    // Path extraction happens via the Tauri listener — nothing to do here.
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
