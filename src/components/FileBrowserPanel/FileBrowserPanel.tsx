import { Component, createEffect, createSignal, For, Show, onCleanup } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { useFileBrowser } from "../../hooks/useFileBrowser";
import { getModifierSymbol } from "../../platform";
import { globToRegex } from "../../utils";
import { replaceBasename } from "../../utils/pathUtils";
import { ContextMenu, createContextMenu, type ContextMenuItem } from "../ContextMenu";
import { PromptDialog } from "../PromptDialog";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { t } from "../../i18n";
import { cx } from "../../utils";
import type { DirEntry } from "../../types/fs";
import p from "../shared/panel.module.css";
import g from "../shared/git-status.module.css";
import s from "./FileBrowserPanel.module.css";

export interface FileBrowserPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
  onFileOpen: (repoPath: string, filePath: string) => void;
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

export const FileBrowserPanel: Component<FileBrowserPanelProps> = (props) => {
  const [entries, setEntries] = createSignal<DirEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [currentSubdir, setCurrentSubdir] = createSignal(".");
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [refreshTrigger, setRefreshTrigger] = createSignal(0);
  const [searchQuery, setSearchQuery] = createSignal("");
  const fb = useFileBrowser();
  const contextMenu = createContextMenu();

  // Rename dialog state
  const [renameDialogVisible, setRenameDialogVisible] = createSignal(false);
  const [renameTarget, setRenameTarget] = createSignal<DirEntry | null>(null);

  // File clipboard state for copy/cut/paste
  const [clipboard, setClipboard] = createSignal<{ entry: DirEntry; mode: "copy" | "cut" } | null>(null);

  // Load entries when visible, repo changes, subdir changes, or repo content changes
  createEffect(() => {
    const visible = props.visible;
    const repoPath = props.repoPath;
    const subdir = currentSubdir();
    // Subscribe to repo revision for auto-refresh on git changes
    void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);
    // Also subscribe to manual refresh trigger
    void refreshTrigger();

    if (!visible || !repoPath) {
      setEntries([]);
      return;
    }

    setLoading(true);
    setError(null);

    (async () => {
      try {
        const result = await fb.listDirectory(repoPath, subdir);
        setEntries(result);
        setSelectedIndex(0);
      } catch (err) {
        setError(String(err));
        setEntries([]);
      } finally {
        setLoading(false);
      }
    })();
  });

  // Reset subdir when repo changes
  createEffect(() => {
    void props.repoPath;
    setCurrentSubdir(".");
  });

  /** Entries filtered by search query (supports glob wildcards) */
  const filteredEntries = () => {
    const q = searchQuery().trim();
    if (!q) return entries();
    const re = globToRegex(q);
    return entries().filter((e) => re.test(e.name) || re.test(e.path));
  };

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
    } else if (props.repoPath) {
      props.onFileOpen(props.repoPath, entry.path);
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

  const handleDelete = async (entry: DirEntry) => {
    if (!props.repoPath) return;
    if (entry.is_dir) return; // Safety: only delete files
    try {
      await fb.deletePath(props.repoPath, entry.path);
      refresh();
    } catch (err) {
      console.error("Failed to delete:", err);
    }
  };

  const handleAddToGitignore = async (entry: DirEntry) => {
    if (!props.repoPath) return;
    const pattern = entry.is_dir ? `${entry.path}/` : entry.path;
    try {
      await fb.addToGitignore(props.repoPath, pattern);
      refresh();
    } catch (err) {
      console.error("Failed to add to .gitignore:", err);
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
    if (!clip || !props.repoPath) return;
    const destDir = currentSubdir() === "." ? "" : `${currentSubdir()}/`;
    const destPath = `${destDir}${clip.entry.name}`;

    // Avoid pasting onto itself
    if (destPath === clip.entry.path) return;

    try {
      if (clip.mode === "copy") {
        await fb.copyPath(props.repoPath, clip.entry.path, destPath);
      } else {
        // Cut = rename (move)
        await fb.renamePath(props.repoPath, clip.entry.path, destPath);
        setClipboard(null);
      }
      refresh();
    } catch (err) {
      console.error(`Failed to ${clip.mode === "copy" ? "copy" : "move"}:`, err);
    }
  };

  const handleRenameConfirm = async (newName: string) => {
    const entry = renameTarget();
    if (!entry || !props.repoPath) return;
    // Build new path: same parent directory, new name
    const newPath = replaceBasename(entry.path, newName);
    try {
      await fb.renamePath(props.repoPath, entry.path, newPath);
      refresh();
    } catch (err) {
      console.error("Failed to rename:", err);
    }
  };

  const handleCopyPath = (entry: DirEntry) => {
    const repoPath = props.repoPath;
    if (!repoPath) return;
    const fullPath = `${repoPath}/${entry.path}`;
    navigator.clipboard.writeText(fullPath).catch((err) =>
      console.error("Failed to copy path:", err),
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

    items.push({
      label: t("fileBrowser.rename", "Rename\u2026"),
      action: () => handleRename(entry),
    });

    if (!entry.is_dir) {
      items.push({
        label: t("fileBrowser.delete", "Delete"),
        action: () => handleDelete(entry),
      });
    }

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

      const isMeta = e.metaKey || e.ctrlKey;
      const list = filteredEntries();

      // Copy/Cut/Paste shortcuts (work even with empty list for paste)
      if (isMeta && e.key === "c" && list.length > 0) {
        e.preventDefault();
        const selected = list[selectedIndex()];
        if (selected && !selected.is_dir) handleCopy(selected);
        return;
      }
      if (isMeta && e.key === "x" && list.length > 0) {
        e.preventDefault();
        const selected = list[selectedIndex()];
        if (selected && !selected.is_dir) handleCut(selected);
        return;
      }
      if (isMeta && e.key === "v") {
        e.preventDefault();
        handlePaste();
        return;
      }

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

      {/* Search filter */}
      <div class={p.search}>
        <input
          type="text"
          class={p.searchInput}
          placeholder={t("fileBrowser.filter", "Filter... (*, ** wildcards)")}
          value={searchQuery()}
          onInput={(e) => {
            setSearchQuery(e.currentTarget.value);
            setSelectedIndex(0);
          }}
        />
        <Show when={searchQuery()}>
          <button class={p.searchClear} onClick={() => { setSearchQuery(""); setSelectedIndex(0); }}>&times;</button>
        </Show>
      </div>

      {/* Breadcrumb navigation */}
      <Show when={breadcrumbs().length > 0}>
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
      </Show>

      <div class={p.content}>
        <Show when={loading()}>
          <div class={s.empty}>{t("fileBrowser.loading", "Loading...")}</div>
        </Show>

        <Show when={error()}>
          <div class={cx(s.empty, s.error)}>{t("fileBrowser.error", "Error:")} {error()}</div>
        </Show>

        <Show when={!loading() && !error() && filteredEntries().length === 0}>
          <div class={s.empty}>
            {!props.repoPath
              ? t("fileBrowser.noRepo", "No repository selected")
              : searchQuery()
              ? t("fileBrowser.noMatches", "No matches")
              : t("fileBrowser.emptyDir", "Empty directory")}
          </div>
        </Show>

        <Show when={!loading() && !error() && filteredEntries().length > 0}>
          {/* Go up entry when in a subdirectory */}
          <Show when={currentSubdir() !== "." && currentSubdir() !== ""}>
            <div class={cx(s.entry, s.entryUp)} onClick={navigateUp}>
              <span class={s.entryIcon}>..</span>
              <span class={s.entryName}>{t("fileBrowser.parent", "(parent)")}</span>
            </div>
          </Show>

          <For each={filteredEntries()}>
            {(entry, index) => (
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
                <span class={s.entryName} title={entry.path}>{entry.name}</span>
                <Show when={entry.git_status}>
                  <span class={cx(g.dot, getStatusClass(entry.git_status))} title={entry.git_status} />
                </Show>
                <Show when={!entry.is_dir && entry.size > 0}>
                  <span class={s.entrySize}>{formatSize(entry.size)}</span>
                </Show>
              </div>
            )}
          </For>
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
    </div>
  );
};

export default FileBrowserPanel;
