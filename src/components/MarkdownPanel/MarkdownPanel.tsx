import { Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { appLogger } from "../../stores/appLogger";
import { mdTabsStore } from "../../stores/mdTabs";
import { ContextMenu, createContextMenu, type ContextMenuItem } from "../ContextMenu";
import { getModifierSymbol } from "../../platform";
import { globToRegex } from "../../utils";
import { pathBasename, pathDirname } from "../../utils/pathUtils";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { t } from "../../i18n";
import { cx } from "../../utils";
import p from "../shared/panel.module.css";
import g from "../shared/git-status.module.css";
import s from "./MarkdownPanel.module.css";

/** Markdown file entry from Rust backend */
interface MdFileEntry {
  path: string;
  git_status: string;
  is_ignored: boolean;
}

export interface MarkdownPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
}

/** Git status badge CSS class (shared pattern with FileBrowser) */
const getStatusClass = (status: string): string => {
  switch (status) {
    case "modified": return g.modified;
    case "staged": return g.staged;
    case "untracked": return g.untracked;
    default: return "";
  }
};

export const MarkdownPanel: Component<MarkdownPanelProps> = (props) => {
  const [files, setFiles] = createSignal<MdFileEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [searchQuery, setSearchQuery] = createSignal("");
  const repo = useRepository();
  const contextMenu = createContextMenu();
  const [contextEntry, setContextEntry] = createSignal<MdFileEntry | null>(null);

  /** Files filtered by search query (supports glob wildcards) */
  const filteredFiles = createMemo(() => {
    const q = searchQuery().trim();
    if (!q) return files();
    const re = globToRegex(q);
    return files().filter((f) => re.test(f.path));
  });

  // Load markdown files when visible, repo changes, or repo content changes
  createEffect(() => {
    const visible = props.visible;
    const repoPath = props.repoPath;
    void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

    if (!visible || !repoPath) {
      setFiles([]);
      return;
    }

    setLoading(true);
    setError(null);

    (async () => {
      try {
        const mdFiles = await repo.listMarkdownFiles(repoPath);
        setFiles(mdFiles);
      } catch (err) {
        setError(String(err));
        setFiles([]);
      } finally {
        setLoading(false);
      }
    })();
  });

  const handleFileClick = (filePath: string) => {
    if (!props.repoPath) return;
    mdTabsStore.add(props.repoPath, filePath);
  };

  /** Group files by directory for tree view, sorted by dir name */
  const sortedGroups = createMemo(() => {
    const allFiles = filteredFiles();
    const groups: Record<string, MdFileEntry[]> = {};

    for (const entry of allFiles) {
      const dir = pathDirname(entry.path);
      const key = dir || "/";
      if (!groups[key]) groups[key] = [];
      groups[key].push(entry);
    }

    return Object.entries(groups).sort(([a], [b]) => a.localeCompare(b));
  });

  const handleContextMenu = (ev: MouseEvent, entry: MdFileEntry) => {
    ev.preventDefault();
    ev.stopPropagation();
    setContextEntry(entry);
    contextMenu.open(ev);
  };

  const getContextMenuItems = (): ContextMenuItem[] => {
    const entry = contextEntry();
    if (!entry || !props.repoPath) return [];
    return [{
      label: t("markdownPanel.copyPath", "Copy Path"),
      action: () => {
        navigator.clipboard.writeText(`${props.repoPath}/${entry.path}`).catch((err) =>
          appLogger.error("app", "Failed to copy path", err),
        );
      },
    }];
  };

  return (
    <div id="markdown-panel" class={cx(s.panel, !props.visible && s.hidden)}>
      <PanelResizeHandle panelId="markdown-panel" />
      <div class={p.header}>
        <div class={p.headerLeft}>
          <span class={p.title}>{t("markdownPanel.title", "Markdown Files")}</span>
          <Show when={!loading() && filteredFiles().length > 0}>
            <span class={p.fileCountBadge}>{filteredFiles().length}</span>
          </Show>
          <span class={p.headerSep} />
          <div class={g.legend}>
            <span class={g.legendItem} title={t("markdownPanel.modified", "Modified (unstaged changes)")}><span class={cx(g.dot, g.modified)} /> mod</span>
            <span class={g.legendItem} title={t("markdownPanel.staged", "Staged for commit")}><span class={cx(g.dot, g.staged)} /> staged</span>
            <span class={g.legendItem} title={t("markdownPanel.untracked", "Untracked (new file)")}><span class={cx(g.dot, g.untracked)} /> new</span>
          </div>
        </div>
        <button class={p.close} onClick={props.onClose} title={`${t("markdownPanel.close", "Close")} (${getModifierSymbol()}M)`}>
          &times;
        </button>
      </div>

      {/* Search filter */}
      <div class={p.search}>
        <input
          type="text"
          class={p.searchInput}
          placeholder={t("markdownPanel.filter", "Filter... (*, ** wildcards)")}
          value={searchQuery()}
          onInput={(e) => setSearchQuery(e.currentTarget.value)}
        />
        <Show when={searchQuery()}>
          <button class={p.searchClear} onClick={() => setSearchQuery("")}>&times;</button>
        </Show>
      </div>

      <div class={p.content}>
        <Show when={loading()}>
          <div class={s.empty}>{t("markdownPanel.loading", "Loading files...")}</div>
        </Show>

        <Show when={error()}>
          <div class={s.error}>{t("markdownPanel.error", "Error:")} {error()}</div>
        </Show>

        <Show when={!loading() && !error() && filteredFiles().length === 0}>
          <div class={s.empty}>
            {!props.repoPath
              ? t("markdownPanel.noRepo", "No repository selected")
              : searchQuery()
              ? t("markdownPanel.noMatches", "No matches")
              : t("markdownPanel.noFiles", "No markdown files found")}
          </div>
        </Show>

        <Show when={!loading() && !error() && filteredFiles().length > 0}>
          <div class={s.fileList}>
            <For each={sortedGroups()}>
              {([dir, dirEntries]) => (
                <>
                  <Show when={dir !== "/"}>
                    <div class={s.dirHeader}>{dir}/</div>
                  </Show>
                  <For each={dirEntries}>
                    {(entry) => {
                      const fileName = pathBasename(entry.path) || entry.path;
                      return (
                        <div
                          class={cx(s.fileItem, entry.is_ignored && s.fileIgnored)}
                          onClick={() => handleFileClick(entry.path)}
                          onContextMenu={(ev) => handleContextMenu(ev, entry)}
                          title={entry.path}
                        >
                          <span class={s.fileIcon}>{"\u{1F4C4}"}</span>
                          <div class={s.fileName}>{fileName}</div>
                          <Show when={entry.git_status}>
                            <span class={cx(g.dot, getStatusClass(entry.git_status))} title={entry.git_status} />
                          </Show>
                        </div>
                      );
                    }}
                  </For>
                </>
              )}
            </For>
          </div>
        </Show>
      </div>

      <ContextMenu
        items={getContextMenuItems()}
        x={contextMenu.position().x}
        y={contextMenu.position().y}
        visible={contextMenu.visible()}
        onClose={contextMenu.close}
      />
    </div>
  );
};

export default MarkdownPanel;
