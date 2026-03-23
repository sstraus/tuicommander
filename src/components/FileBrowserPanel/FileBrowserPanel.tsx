import { Component, createEffect, createMemo, createSignal, For, Show, onCleanup, untrack } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { appLogger } from "../../stores/appLogger";
import { useFileBrowser } from "../../hooks/useFileBrowser";
import { invoke, listen } from "../../invoke";
import { getModifierSymbol, shortenHomePath } from "../../platform";
import { replaceBasename } from "../../utils/pathUtils";
import { ContextMenu, createContextMenu, type ContextMenuItem } from "../ContextMenu";
import { Dropdown } from "../ui/Dropdown";
import { ConfirmDialog } from "../ConfirmDialog";
import { PromptDialog } from "../PromptDialog";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { t } from "../../i18n";
import { cx } from "../../utils";
import type { DirEntry, ContentMatch } from "../../types/fs";
import type { ContentSearchOptions } from "../../hooks/useFileBrowser";
import p from "../shared/panel.module.css";
import g from "../shared/git-status.module.css";
import s from "./FileBrowserPanel.module.css";

export interface FileBrowserPanelProps {
  visible: boolean;
  repoPath: string | null;
  /** Effective filesystem root (worktree path when on a linked worktree) */
  fsRoot?: string | null;
  onClose: () => void;
  onFileOpen: (repoPath: string, filePath: string, line?: number) => void;
}

/** Format file size for display */
function formatSize(bytes: number): string {
  if (bytes === 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Git status badge CSS class */
const getStatusClass = (status: string): string => {
  switch (status) {
    case "modified": return g.modified;
    case "staged": return g.staged;
    case "untracked": return g.untracked;
    default: return "";
  }
};

/** SVG icons for content search toggle buttons (same as SearchBar) */
const CaseSensitiveIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8.854 11.702h-1l-.816-2.159H3.772l-.768 2.16H2L5.09 4h.76l3.004 7.702zm-2.27-3.074L5.452 5.549a1.635 1.635 0 01-.066-.252h-.02a1.674 1.674 0 01-.07.256L4.17 8.628h2.415zM13.995 11.7v-.73c-.37.47-.955.792-1.705.792-1.2 0-2.088-.797-2.088-1.836 0-1.092.855-1.792 2.156-1.867l1.636-.09v-.362c0-.788-.49-1.257-1.328-1.257-.678 0-1.174.31-1.399.778h-.91c.153-.95 1.085-1.635 2.333-1.635 1.39 0 2.227.79 2.227 2.04V11.7h-.922z"/>
  </svg>
);

const WholeWordIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M2 6h1v7H2V6zm5.38 4.534h-.022c-.248.371-.7.596-1.205.596C5.344 11.13 4.7 10.48 4.7 9.6c0-.925.604-1.46 1.703-1.522l1-.052V7.72c0-.547-.336-.87-.918-.87-.514 0-.836.22-1.002.563H4.59c.156-.734.808-1.253 1.866-1.253 1.117 0 1.825.6 1.825 1.548v3.023h-.9v-.197zM5.604 9.6c0 .37.283.64.674.64.548 0 .96-.373.96-.836v-.45l-.864.046c-.57.034-.77.256-.77.6zM10.552 6.26c.467 0 .824.186 1.078.54V4h.904v8.73h-.9v-.65c-.258.456-.66.72-1.158.72C9.546 12.8 8.8 11.88 8.8 10.52c0-1.37.74-2.26 1.752-2.26zm.18.816c-.647 0-1.038.56-1.038 1.44s.39 1.46 1.048 1.46c.647 0 1.043-.57 1.043-1.45s-.396-1.45-1.053-1.45z"/>
    <path d="M1 13h14v1H1z"/>
  </svg>
);

const RegexIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M10.012 2h.976v3.113l2.56-1.557.486.885L11.47 6l2.564 1.559-.486.885-2.56-1.557V10h-.976V6.887l-2.56 1.557-.486-.885L9.53 6 6.966 4.441l.486-.885 2.56 1.557V2zM2 10h4v4H2v-4z"/>
  </svg>
);

/** Filename mode icon — simple "F" in a doc shape */
const FilenameModeIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M4 2h5l3 3v9H4V2zm1 1v10h6V6H9V3H5zm1.5 4h3v1h-3V7zm0 2h3v1h-3V9z"/>
  </svg>
);

/** Content mode icon — magnifier with lines */
const ContentModeIcon = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M11.5 7a4.5 4.5 0 1 0-1.77 3.56l3.35 3.36.71-.71-3.36-3.35A4.48 4.48 0 0 0 11.5 7zM7 10.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7zM5 6h4v1H5V6zm0 2h4v1H5V8z"/>
  </svg>
);

export const FileBrowserPanel: Component<FileBrowserPanelProps> = (props) => {
  const [entries, setEntries] = createSignal<DirEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [currentSubdir, setCurrentSubdir] = createSignal(".");
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [refreshTrigger, setRefreshTrigger] = createSignal(0);
  const [searchQuery, setSearchQuery] = createSignal("");
  const fb = useFileBrowser();
  /** Effective filesystem root — worktree path when on a linked worktree */
  const root = () => props.fsRoot || props.repoPath;
  const contextMenu = createContextMenu();

  // Rename dialog state
  const [renameDialogVisible, setRenameDialogVisible] = createSignal(false);
  const [renameTarget, setRenameTarget] = createSignal<DirEntry | null>(null);

  // Delete confirmation dialog state
  const [deleteDialogVisible, setDeleteDialogVisible] = createSignal(false);
  const [deleteTarget, setDeleteTarget] = createSignal<DirEntry | null>(null);

  // File clipboard state for copy/cut/paste
  const [clipboard, setClipboard] = createSignal<{ entry: DirEntry; mode: "copy" | "cut" } | null>(null);

  // Search mode: "filename" (default) or "content" (full-text grep)
  type SearchMode = "filename" | "content";
  const [searchMode, setSearchMode] = createSignal<SearchMode>("filename");
  const [contentSearching, setContentSearching] = createSignal(false);
  const [contentMatches, setContentMatches] = createSignal<ContentMatch[]>([]);
  const [contentStats, setContentStats] = createSignal<{
    filesSearched: number;
    filesSkipped: number;
    truncated: boolean;
  }>({ filesSearched: 0, filesSkipped: 0, truncated: false });
  const [caseSensitive, setCaseSensitive] = createSignal(false);
  const [useRegex, setUseRegex] = createSignal(false);
  const [wholeWord, setWholeWord] = createSignal(false);

  // Sort mode: "name" (default, dirs first + alpha) or "date" (dirs first + newest first)
  type SortMode = "name" | "date";
  const [sortBy, setSortBy] = createSignal<SortMode>("name");
  const [sortDropdownOpen, setSortDropdownOpen] = createSignal(false);

  // Directory watcher revision — bumped when dir-changed event arrives
  const [dirRevision, setDirRevision] = createSignal(0);

  // Track repoPath changes to reset subdir synchronously before fetching
  let lastRepoPath: string | null = null;

  // Load entries when visible, repo changes, subdir changes, or repo content changes
  createEffect(() => {
    if (!props.visible || !root()) {
      setEntries([]);
      return;
    }

    const fsRoot = root()!;

    // Reset subdir when root changes (merged from separate effect to avoid double fetch)
    if (fsRoot !== lastRepoPath) {
      lastRepoPath = fsRoot;
      setCurrentSubdir(".");
    }

    const subdir = currentSubdir();
    // Subscribe to repo revision for auto-refresh on git changes
    void (props.repoPath ? repositoriesStore.getRevision(props.repoPath) : 0);
    // Subscribe to dir watcher revision for auto-refresh on filesystem changes
    const dirRev = dirRevision();
    // Also subscribe to manual refresh trigger
    void refreshTrigger();

    // Preserve selection by path on dir-watcher refreshes (dirRev > 0)
    // untrack: reading filteredEntries inside this effect would create a circular
    // dependency (effect sets entries → filteredEntries recomputes → effect re-runs)
    const prevSelectedPath = dirRev > 0 ? untrack(() => filteredEntries()[selectedIndex()]?.path) : undefined;

    // Only show loading spinner on initial load — suppress it on auto-refreshes to
    // avoid visible flicker when the directory content hasn't actually changed.
    const isInitialLoad = untrack(() => entries().length === 0);
    if (isInitialLoad) setLoading(true);
    setError(null);

    (async () => {
      try {
        const result = await fb.listDirectory(fsRoot, subdir);
        // Skip re-render when entries are identical: same count and every entry
        // matches on the fields that drive visible state (path, git badge, mtime,
        // ignored flag). New object instances from Rust would otherwise cause a
        // full DOM repaint even when nothing changed.
        const current = untrack(() => entries());
        const changed =
          current.length !== result.length ||
          result.some((e, i) => {
            const c = current[i];
            return e.path !== c.path || e.git_status !== c.git_status ||
              e.modified_at !== c.modified_at || e.is_ignored !== c.is_ignored;
          });
        if (changed) {
          setEntries(result);
          // Restore selection by path after auto-refresh, reset on initial load
          if (prevSelectedPath) {
            const idx = result.findIndex((e) => e.path === prevSelectedPath);
            setSelectedIndex(idx >= 0 ? idx : 0);
          } else {
            setSelectedIndex(0);
          }
        }
      } catch (err) {
        setError(String(err));
        setEntries([]);
      } finally {
        setLoading(false);
      }
    })();
  });

  // Directory watcher lifecycle: start/stop watcher as directory or visibility changes
  createEffect(() => {
    if (!props.visible || !root()) return;

    const fsRoot = root()!;
    const subdir = currentSubdir();
    // Don't watch during search (search is recursive, watcher is not)
    if (searchQuery().trim()) return;

    const absPath = subdir === "." || subdir === "" ? fsRoot : `${fsRoot}/${subdir}`;

    invoke("start_dir_watcher", { path: absPath }).catch((err) => {
      appLogger.warn("app", `Dir watcher failed for ${absPath}: ${err}`);
    });

    // Listen for dir-changed events matching this path
    const unlisten = listen<{ dir_path: string }>("dir-changed", (event) => {
      if (event.payload.dir_path === absPath) {
        setDirRevision((n) => n + 1);
      }
    });

    onCleanup(() => {
      invoke("stop_dir_watcher", { path: absPath }).catch((err) => {
        appLogger.warn("app", `Failed to stop dir watcher for ${absPath}`, err);
      });
      unlisten.then((fn) => fn());
    });
  });

  // Search results from recursive Rust search (when query is active)
  const [searchResults, setSearchResults] = createSignal<DirEntry[]>([]);
  const [searching, setSearching] = createSignal(false);

  // Debounced recursive filename search — only fires in filename mode
  createEffect(() => {
    if (searchMode() !== "filename") return;
    const q = searchQuery().trim();
    const fsRoot = root();

    if (!q || !fsRoot) {
      setSearchResults([]);
      setSearching(false);
      return;
    }

    setSearching(true);
    const timer = setTimeout(async () => {
      try {
        const results = await fb.searchFiles(fsRoot, q);
        setSearchResults(results);
        setSelectedIndex(0);
      } catch (err) {
        appLogger.error("app", "File search failed", err);
        setSearchResults([]);
      } finally {
        setSearching(false);
      }
    }, 200);

    onCleanup(() => clearTimeout(timer));
  });

  // Content search — fires when in content mode and query changes
  createEffect(() => {
    if (searchMode() !== "content") return;
    const q = searchQuery().trim();
    const fsRoot = root();

    if (!q || q.length < 3 || !fsRoot) {
      setContentMatches([]);
      setContentSearching(false);
      setContentStats({ filesSearched: 0, filesSkipped: 0, truncated: false });
      return;
    }

    // Track current search options as reactive deps
    const opts: ContentSearchOptions = {
      caseSensitive: caseSensitive(),
      useRegex: useRegex(),
      wholeWord: wholeWord(),
    };

    setContentSearching(true);
    setContentMatches([]);
    setContentStats({ filesSearched: 0, filesSkipped: 0, truncated: false });

    let cancelled = false;
    let unlistenBatch: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;

    const timer = setTimeout(async () => {
      if (cancelled) return;

      // Set up batch listener before starting the search
      try {
        const batchPromise = fb.onContentSearchBatch((batch) => {
          if (cancelled) return;
          setContentMatches((prev) => [...prev, ...batch.matches]);
          setContentStats({
            filesSearched: batch.files_searched,
            filesSkipped: batch.files_skipped,
            truncated: batch.truncated,
          });
          if (batch.is_final) {
            setContentSearching(false);
          }
        });
        const errorPromise = fb.onContentSearchError((err) => {
          if (cancelled) return;
          appLogger.error("app", "Content search error", err);
          setContentSearching(false);
        });

        const [batchUn, errorUn] = await Promise.all([batchPromise, errorPromise]);
        unlistenBatch = batchUn;
        unlistenError = errorUn;

        if (cancelled) {
          unlistenBatch();
          unlistenError();
          return;
        }

        await fb.searchContent(fsRoot, q, opts);
      } catch (err) {
        if (!cancelled) {
          appLogger.error("app", "Content search failed", err);
          setContentSearching(false);
        }
      }
    }, 500);

    onCleanup(() => {
      cancelled = true;
      clearTimeout(timer);
      unlistenBatch?.();
      unlistenError?.();
    });
  });

  /** Content matches grouped by file path */
  const contentMatchGroups = createMemo(() => {
    const matches = contentMatches();
    if (matches.length === 0) return [];
    const groups: { path: string; matches: ContentMatch[] }[] = [];
    const map = new Map<string, ContentMatch[]>();
    for (const m of matches) {
      let arr = map.get(m.path);
      if (!arr) {
        arr = [];
        map.set(m.path, arr);
        groups.push({ path: m.path, matches: arr });
      }
      arr.push(m);
    }
    return groups;
  });

  /** Visible entries: search results when query active, directory listing otherwise, sorted */
  const filteredEntries = createMemo(() => {
    const raw = searchQuery().trim() ? searchResults() : entries();
    if (sortBy() === "name") return raw; // already sorted by name from Rust
    // Sort by date: dirs first, then newest first
    return [...raw].sort((a, b) =>
      (b.is_dir ? 1 : 0) - (a.is_dir ? 1 : 0) || b.modified_at - a.modified_at,
    );
  });

  const refresh = () => setRefreshTrigger((n) => n + 1);

  const navigateInto = (entry: DirEntry) => {
    setCurrentSubdir(entry.path);
  };

  const navigateUp = () => {
    const current = currentSubdir();
    if (current === "." || current === "") return;
    const parts = current.split("/");
    parts.pop();
    setCurrentSubdir(parts.length === 0 ? "." : parts.join("/"));
  };

  const handleEntryClick = (entry: DirEntry) => {
    if (entry.is_dir) {
      navigateInto(entry);
    } else if (root()) {
      props.onFileOpen(root()!, entry.path);
    }
  };

  // Breadcrumb segments from currentSubdir
  const breadcrumbs = () => {
    const subdir = currentSubdir();
    if (subdir === "." || subdir === "") return [];
    return subdir.split("/");
  };

  const handleBreadcrumbClick = (index: number) => {
    const segments = breadcrumbs();
    if (index < 0) {
      setCurrentSubdir(".");
    } else {
      setCurrentSubdir(segments.slice(0, index + 1).join("/"));
    }
  };

  // Context menu actions
  const handleRename = (entry: DirEntry) => {
    setRenameTarget(entry);
    setRenameDialogVisible(true);
  };

  const handleDelete = (entry: DirEntry) => {
    setDeleteTarget(entry);
    setDeleteDialogVisible(true);
  };

  const confirmDelete = async () => {
    const entry = deleteTarget();
    if (!entry || !root()) return;
    setDeleteDialogVisible(false);
    try {
      await fb.deletePath(root()!, entry.path);
      refresh();
    } catch (err) {
      appLogger.error("app", "Failed to delete", err);
    }
  };

  const handleAddToGitignore = async (entry: DirEntry) => {
    if (!root()) return;
    const pattern = entry.is_dir ? `${entry.path}/` : entry.path;
    try {
      await fb.addToGitignore(root()!, pattern);
      refresh();
    } catch (err) {
      appLogger.error("git", "Failed to add to .gitignore", err);
    }
  };

  const handleCopy = (entry: DirEntry) => {
    setClipboard({ entry, mode: "copy" });
  };

  const handleCut = (entry: DirEntry) => {
    setClipboard({ entry, mode: "cut" });
  };

  const handlePaste = async () => {
    const clip = clipboard();
    if (!clip || !root()) return;
    const destDir = currentSubdir() === "." ? "" : `${currentSubdir()}/`;
    const destPath = `${destDir}${clip.entry.name}`;

    // Avoid pasting onto itself
    if (destPath === clip.entry.path) return;

    try {
      if (clip.mode === "copy") {
        await fb.copyPath(root()!, clip.entry.path, destPath);
      } else {
        // Cut = rename (move)
        await fb.renamePath(root()!, clip.entry.path, destPath);
        setClipboard(null);
      }
      refresh();
    } catch (err) {
      appLogger.error("app", `Failed to ${clip.mode === "copy" ? "copy" : "move"}`, err);
    }
  };

  const handleRenameConfirm = async (newName: string) => {
    const entry = renameTarget();
    if (!entry || !root()) return;
    // Build new path: same parent directory, new name
    const newPath = replaceBasename(entry.path, newName);
    try {
      await fb.renamePath(root()!, entry.path, newPath);
      refresh();
    } catch (err) {
      appLogger.error("app", "Failed to rename", err);
    }
  };

  const handleCopyPath = (entry: DirEntry) => {
    const fsRoot = root();
    if (!fsRoot) return;
    const fullPath = `${fsRoot}/${entry.path}`;
    navigator.clipboard.writeText(shortenHomePath(fullPath)).catch((err) =>
      appLogger.error("app", "Failed to copy path", err),
    );
  };

  const getContextMenuItems = (entry: DirEntry): ContextMenuItem[] => {
    const mod = getModifierSymbol();
    const items: ContextMenuItem[] = [];

    items.push({
      label: t("fileBrowser.copyPath", "Copy Path"),
      action: () => handleCopyPath(entry),
    });

    if (!entry.is_dir) {
      items.push({
        label: t("fileBrowser.copy", "Copy"),
        shortcut: `${mod}C`,
        action: () => handleCopy(entry),
      });
      items.push({
        label: t("fileBrowser.cut", "Cut"),
        shortcut: `${mod}X`,
        action: () => handleCut(entry),
      });
    }

    items.push({
      label: t("fileBrowser.paste", "Paste"),
      shortcut: `${mod}V`,
      action: handlePaste,
      disabled: !clipboard(),
      separator: true,
    });

    if (!entry.is_dir && (entry.path.endsWith(".html") || entry.path.endsWith(".htm"))) {
      items.push({
        label: "Open in Browser",
        action: () => {
          const r = root();
          if (r) {
            import("@tauri-apps/plugin-opener").then(({ openPath }) => {
              const abs = entry.path.startsWith("/") ? entry.path : `${r}/${entry.path}`;
              openPath(abs).catch((err) => appLogger.error("app", "Failed to open HTML in browser", err));
            });
          }
        },
        separator: true,
      });
    }

    items.push({
      label: t("fileBrowser.rename", "Rename\u2026"),
      action: () => handleRename(entry),
    });

    items.push({
      label: t("fileBrowser.delete", "Delete"),
      action: () => handleDelete(entry),
    });

    items.push({
      label: t("fileBrowser.addGitignore", "Add to .gitignore"),
      action: () => handleAddToGitignore(entry),
      disabled: entry.is_ignored,
      separator: true,
    });

    return items;
  };

  // Track which entry the context menu is for
  const [contextEntry, setContextEntry] = createSignal<DirEntry | null>(null);

  const handleContextMenu = (e: MouseEvent, entry: DirEntry) => {
    e.preventDefault();
    e.stopPropagation();
    setContextEntry(entry);
    contextMenu.open(e);
  };

  // Keyboard navigation
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      // Only handle if the panel is focused (not terminal)
      const panel = document.getElementById("file-browser-panel");
      if (!panel?.contains(document.activeElement) && document.activeElement !== panel) return;

      // Let the search input handle its own keyboard events
      const isInputFocused = document.activeElement instanceof HTMLInputElement;

      const isMeta = e.metaKey || e.ctrlKey;
      const list = filteredEntries();

      // Copy/Cut/Paste shortcuts (work even with empty list for paste)
      if (!isInputFocused && isMeta && e.key === "c" && list.length > 0) {
        e.preventDefault();
        const selected = list[selectedIndex()];
        if (selected && !selected.is_dir) handleCopy(selected);
        return;
      }
      if (!isInputFocused && isMeta && e.key === "x" && list.length > 0) {
        e.preventDefault();
        const selected = list[selectedIndex()];
        if (selected && !selected.is_dir) handleCut(selected);
        return;
      }
      if (!isInputFocused && isMeta && e.key === "v") {
        e.preventDefault();
        handlePaste();
        return;
      }

      // Don't capture navigation keys when typing in the search input
      if (isInputFocused) return;

      if (list.length === 0) return;

      switch (e.key) {
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((i) => Math.max(0, i - 1));
          break;
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((i) => Math.min(list.length - 1, i + 1));
          break;
        case "Enter": {
          e.preventDefault();
          const selected = list[selectedIndex()];
          if (selected) handleEntryClick(selected);
          break;
        }
        case "Backspace":
          e.preventDefault();
          navigateUp();
          break;
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  return (
    <div id="file-browser-panel" class={cx(s.panel, !props.visible && s.hidden)} tabIndex={-1}>
      <PanelResizeHandle panelId="file-browser-panel" />
      <div class={p.header}>
        <div class={p.headerLeft}>
          <span class={p.title}>{t("fileBrowser.title", "Files")}</span>
          <Show when={!loading() && entries().length > 0}>
            <span class={p.fileCountBadge}>{entries().length}</span>
          </Show>
          <span class={p.headerSep} />
          <div class={g.legend}>
            <span class={g.legendItem} title={t("fileBrowser.modified", "Modified (unstaged changes)")}><span class={cx(g.dot, g.modified)} /> mod</span>
            <span class={g.legendItem} title={t("fileBrowser.staged", "Staged for commit")}><span class={cx(g.dot, g.staged)} /> staged</span>
            <span class={g.legendItem} title={t("fileBrowser.untracked", "Untracked (new file)")}><span class={cx(g.dot, g.untracked)} /> new</span>
          </div>
        </div>
        <button class={p.close} onClick={props.onClose} title={`${t("fileBrowser.close", "Close")} (${getModifierSymbol()}E)`}>
          &times;
        </button>
      </div>

      {/* Search filter with F/C mode toggle */}
      <div class={s.searchBar}>
        <button
          class={cx(s.modeToggle, searchMode() === "content" && s.modeToggleActive)}
          onClick={() => {
            const next = searchMode() === "filename" ? "content" : "filename";
            setSearchMode(next);
            // Clear results from the other mode
            if (next === "content") {
              setSearchResults([]);
            } else {
              setContentMatches([]);
              setContentSearching(false);
              setContentStats({ filesSearched: 0, filesSkipped: 0, truncated: false });
            }
            setSelectedIndex(0);
          }}
          title={searchMode() === "filename" ? "Switch to content search" : "Switch to filename search"}
        >
          <Show when={searchMode() === "filename"} fallback={<ContentModeIcon />}>
            <FilenameModeIcon />
          </Show>
        </button>
        <input
          type="text"
          class={p.searchInput}
          placeholder={searchMode() === "filename"
            ? t("fileBrowser.search", "Search files\u2026 (*, ** wildcards)")
            : t("fileBrowser.searchContent", "Search in file contents\u2026")}
          value={searchQuery()}
          onInput={(e) => {
            setSearchQuery(e.currentTarget.value);
            setSelectedIndex(0);
          }}
        />
        <Show when={searchMode() === "content"}>
          <button
            class={cx(s.toggleBtn, caseSensitive() && s.toggleActive)}
            onClick={() => setCaseSensitive((v) => !v)}
            title="Match Case"
          >
            <CaseSensitiveIcon />
          </button>
          <button
            class={cx(s.toggleBtn, useRegex() && s.toggleActive)}
            onClick={() => setUseRegex((v) => !v)}
            title="Use Regular Expression"
          >
            <RegexIcon />
          </button>
          <button
            class={cx(s.toggleBtn, wholeWord() && s.toggleActive)}
            onClick={() => setWholeWord((v) => !v)}
            title="Match Whole Word"
          >
            <WholeWordIcon />
          </button>
        </Show>
        <Show when={searchQuery()}>
          <button class={p.searchClear} onClick={() => {
            setSearchQuery("");
            setSelectedIndex(0);
            setContentMatches([]);
            setContentSearching(false);
            setContentStats({ filesSearched: 0, filesSkipped: 0, truncated: false });
          }}>&times;</button>
        </Show>
      </div>

      {/* Toolbar: breadcrumb + sort */}
      <div class={s.toolbar}>
        <div class={s.breadcrumb}>
          <span class={s.breadcrumbSegment} onClick={() => handleBreadcrumbClick(-1)}>
            /
          </span>
          <For each={breadcrumbs()}>
            {(segment, index) => (
              <>
                <span class={s.breadcrumbSep}>/</span>
                <span
                  class={cx(s.breadcrumbSegment, index() === breadcrumbs().length - 1 && s.breadcrumbCurrent)}
                  onClick={() => handleBreadcrumbClick(index())}
                >
                  {segment}
                </span>
              </>
            )}
          </For>
        </div>
        <div class={s.sortControl}>
          <button
            class={s.sortTrigger}
            onClick={() => setSortDropdownOpen((v) => !v)}
            title={`${t("fileBrowser.sortBy", "Sort by")} ${sortBy()}`}
          >
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
              <path d="M1 2h14L9.5 8.5V13l-3 1.5V8.5z" />
            </svg>
          </button>
          <Dropdown
            items={[
              { id: "name", label: t("fileBrowser.sortName", "Name") },
              { id: "date", label: t("fileBrowser.sortDate", "Date") },
            ]}
            selected={sortBy()}
            visible={sortDropdownOpen()}
            onSelect={(id) => { setSortBy(id as SortMode); setSortDropdownOpen(false); }}
            onClose={() => setSortDropdownOpen(false)}
          />
        </div>
      </div>

      {/* Content search status bar */}
      <Show when={searchMode() === "content" && searchQuery().trim().length >= 3}>
        <div class={cx(s.searchStatus, contentStats().truncated && s.searchStatusTruncated, contentSearching() && s.searchStatusSearching)}>
          <Show when={contentSearching()} fallback={
            contentMatches().length > 0
              ? `${contentMatches().length} match${contentMatches().length !== 1 ? "es" : ""} in ${contentMatchGroups().length} file${contentMatchGroups().length !== 1 ? "s" : ""}${contentStats().truncated ? " (results limited)" : ""}${contentStats().filesSkipped > 0 ? ` \u00B7 ${contentStats().filesSkipped} skipped` : ""}`
              : "No matches"
          }>
            {"Searching\u2026"}
          </Show>
        </div>
      </Show>

      <div class={p.content}>
        <Show when={loading() || (searching() && searchMode() === "filename")}>
          <div class={s.empty}>{searching() ? t("fileBrowser.searching", "Searching\u2026") : t("fileBrowser.loading", "Loading...")}</div>
        </Show>

        <Show when={error()}>
          <div class={cx(s.empty, s.error)}>{t("fileBrowser.error", "Error:")} {error()}</div>
        </Show>

        {/* Content search results */}
        <Show when={searchMode() === "content" && searchQuery().trim().length >= 3}>
          <Show when={!contentSearching() && contentMatches().length === 0}>
            <div class={s.empty}>{t("fileBrowser.noMatches", "No matches")}</div>
          </Show>
          <For each={contentMatchGroups()}>
            {(group) => (
              <div class={s.contentGroup}>
                <div
                  class={s.contentGroupHeader}
                  onClick={() => {
                    if (root() && group.matches.length > 0) {
                      props.onFileOpen(root()!, group.path, group.matches[0].line_number);
                    }
                  }}
                >
                  <span>{group.path}</span>
                  <span class={s.contentGroupCount}>
                    {group.matches.length} match{group.matches.length !== 1 ? "es" : ""}
                  </span>
                </div>
                <For each={group.matches}>
                  {(match) => (
                    <div
                      class={s.contentMatch}
                      onClick={() => {
                        if (root()) {
                          props.onFileOpen(root()!, match.path, match.line_number);
                        }
                      }}
                    >
                      <span class={s.contentMatchLine}>{match.line_number}</span>
                      <span class={s.contentMatchText}>
                        {match.match_start > 0 ? match.line_text.slice(0, match.match_start) : ""}
                        <span class={s.contentMatchHighlight}>
                          {match.line_text.slice(match.match_start, match.match_end)}
                        </span>
                        {match.match_end < match.line_text.length ? match.line_text.slice(match.match_end) : ""}
                      </span>
                    </div>
                  )}
                </For>
              </div>
            )}
          </For>
        </Show>

        {/* Filename mode: directory listing or filename search results */}
        <Show when={searchMode() === "filename"}>
          <Show when={!loading() && !searching() && !error() && filteredEntries().length === 0}>
            <div class={s.empty}>
              {!root()
                ? t("fileBrowser.noRepo", "No repository selected")
                : searchQuery()
                ? t("fileBrowser.noMatches", "No matches")
                : t("fileBrowser.emptyDir", "Empty directory")}
            </div>
          </Show>

          <Show when={!loading() && !searching() && !error() && filteredEntries().length > 0}>
            {/* Go up entry when in a subdirectory and not searching */}
            <Show when={!searchQuery().trim() && currentSubdir() !== "." && currentSubdir() !== ""}>
              <div class={cx(s.entry, s.entryParent)} onClick={navigateUp}>
                <span class={s.entryIcon}>
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M8 2L2 8l6 6V10h6V6H8V2z" />
                  </svg>
                </span>
                <span class={s.entryName}>..</span>
              </div>
            </Show>

            <For each={filteredEntries()}>
              {(entry, index) => {
                const isSearch = !!searchQuery().trim();
                return (
                  <div
                    class={cx(
                      s.entry,
                      entry.is_dir && s.entryDir,
                      selectedIndex() === index() && s.entrySelected,
                      entry.is_ignored && s.entryIgnored,
                      clipboard()?.mode === "cut" && clipboard()?.entry.path === entry.path && s.entryCut,
                    )}
                    onClick={() => {
                      setSelectedIndex(index());
                      handleEntryClick(entry);
                    }}
                    onContextMenu={(e) => handleContextMenu(e, entry)}
                  >
                    <span class={s.entryIcon}>{entry.is_dir ? "\u{1F4C1}" : "\u{1F4C4}"}</span>
                    <span class={s.entryName} title={entry.path}>{isSearch ? entry.path : entry.name}</span>
                    <Show when={entry.git_status}>
                      <span class={cx(g.dot, getStatusClass(entry.git_status))} title={entry.git_status} />
                    </Show>
                    <Show when={!entry.is_dir && entry.size > 0}>
                      <span class={s.entrySize}>{formatSize(entry.size)}</span>
                    </Show>
                  </div>
                );
              }}
            </For>
          </Show>
        </Show>
      </div>

      {/* Context menu */}
      <ContextMenu
        items={contextEntry() ? getContextMenuItems(contextEntry()!) : []}
        x={contextMenu.position().x}
        y={contextMenu.position().y}
        visible={contextMenu.visible()}
        onClose={contextMenu.close}
      />

      {/* Rename dialog */}
      <PromptDialog
        visible={renameDialogVisible()}
        title={t("fileBrowser.renameTitle", "Rename")}
        placeholder={t("fileBrowser.renamePlaceholder", "New name")}
        defaultValue={renameTarget()?.name || ""}
        confirmLabel={t("fileBrowser.renameConfirm", "Rename")}
        onClose={() => setRenameDialogVisible(false)}
        onConfirm={handleRenameConfirm}
      />

      {/* Delete confirmation dialog */}
      <ConfirmDialog
        visible={deleteDialogVisible()}
        title={deleteTarget()?.is_dir ? "Delete Folder" : "Delete File"}
        message={`Permanently delete "${deleteTarget()?.name ?? ""}"${deleteTarget()?.is_dir ? " and all its contents" : ""}?`}
        confirmLabel="Delete"
        kind="warning"
        onClose={() => setDeleteDialogVisible(false)}
        onConfirm={confirmDelete}
      />
    </div>
  );
};

export default FileBrowserPanel;
