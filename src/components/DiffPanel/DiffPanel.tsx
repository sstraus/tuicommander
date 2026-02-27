import { Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { useRepository, type ChangedFile } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore, type DiffStatus } from "../../stores/diffTabs";
import { getModifierSymbol } from "../../platform";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { t } from "../../i18n";
import { cx, globToRegex } from "../../utils";
import p from "../shared/panel.module.css";
import s from "./DiffPanel.module.css";

/** Recent commit info from Rust */
interface RecentCommit {
  hash: string;
  short_hash: string;
  subject: string;
}

export interface DiffPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
}

export const DiffPanel: Component<DiffPanelProps> = (props) => {
  const [files, setFiles] = createSignal<ChangedFile[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [scope, setScope] = createSignal<string | undefined>(undefined);
  const [commits, setCommits] = createSignal<RecentCommit[]>([]);
  const [searchQuery, setSearchQuery] = createSignal("");
  const repo = useRepository();

  /** Files filtered by search query (supports glob wildcards) */
  const filteredFiles = createMemo(() => {
    const q = searchQuery().trim();
    if (!q) return files();
    const re = globToRegex(q);
    return files().filter((f) => re.test(f.path));
  });

  // Load recent commits when panel becomes visible or repo changes
  createEffect(() => {
    const visible = props.visible;
    const repoPath = props.repoPath;
    void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

    if (!visible || !repoPath) {
      setCommits([]);
      return;
    }

    (async () => {
      const recent = await repo.getRecentCommits(repoPath, 5);
      setCommits(recent);
    })();
  });

  // Load changed files when visible, repo changes, scope changes, or repo content changes
  createEffect(() => {
    const visible = props.visible;
    const repoPath = props.repoPath;
    const currentScope = scope();
    void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

    if (!visible || !repoPath) {
      setFiles([]);
      return;
    }

    setLoading(true);
    setError(null);

    (async () => {
      try {
        const changedFiles = await repo.getChangedFiles(repoPath, currentScope);
        setFiles(changedFiles);
      } catch (err) {
        setError(String(err));
        setFiles([]);
      } finally {
        setLoading(false);
      }
    })();
  });

  const handleFileClick = (file: ChangedFile) => {
    if (!props.repoPath) return;
    diffTabsStore.add(props.repoPath, file.path, file.status as DiffStatus, scope(), file.status === "?" || undefined);
  };

  const getStatusDisplay = (status: string): { label: string; className: string } => {
    switch (status) {
      case "M":
        return { label: "M", className: p.statusModified };
      case "A":
        return { label: "A", className: p.statusAdded };
      case "D":
        return { label: "D", className: p.statusDeleted };
      case "R":
        return { label: "R", className: p.statusRenamed };
      case "?":
        return { label: "?", className: p.statusAdded };
      default:
        return { label: status, className: p.statusUnknown };
    }
  };

  const formatStats = (additions: number, deletions: number): string => {
    const parts: string[] = [];
    if (additions > 0) parts.push(`+${additions}`);
    if (deletions > 0) parts.push(`-${deletions}`);
    return parts.join(" ");
  };

  const handleScopeChange = (e: Event) => {
    const value = (e.target as HTMLSelectElement).value;
    setScope(value === "" ? undefined : value);
  };

  return (
    <div id="diff-panel" class={cx(s.panel, !props.visible && s.hidden)}>
      <PanelResizeHandle panelId="diff-panel" />
      <div class={p.header}>
        <div class={p.headerLeft}>
          <span class={p.title}>{t("diffPanel.title", "Changes")}</span>
          <Show when={!loading() && files().length > 0}>
            <span class={p.fileCountBadge}>
              {searchQuery() ? `${filteredFiles().length}/${files().length}` : files().length}
            </span>
          </Show>
        </div>
        <button class={p.close} onClick={props.onClose} title={`${t("diffPanel.close", "Close")} (${getModifierSymbol()}D)`}>
          &times;
        </button>
      </div>

      {/* Scope selector: Working tree or a recent commit */}
      <div class={s.scopeBar}>
        <select class={s.scopeSelect} value={scope() ?? ""} onChange={handleScopeChange}>
          <option value="">{t("diffPanel.workingTree", "Working tree")}</option>
          <For each={commits()}>
            {(commit) => (
              <option value={commit.hash}>
                {commit.short_hash} {commit.subject}
              </option>
            )}
          </For>
        </select>
      </div>

      {/* Search filter */}
      <div class={p.search}>
        <input
          type="text"
          class={p.searchInput}
          placeholder={t("diffPanel.filter", "Filter... (*, ** wildcards)")}
          value={searchQuery()}
          onInput={(e) => setSearchQuery(e.currentTarget.value)}
        />
        <Show when={searchQuery()}>
          <button class={p.searchClear} onClick={() => setSearchQuery("")}>&times;</button>
        </Show>
      </div>

      <div class={p.content}>
        <Show when={loading()}>
          <div class={s.empty}>{t("diffPanel.loading", "Loading changes...")}</div>
        </Show>

        <Show when={error()}>
          <div class={s.emptyError}>{t("diffPanel.error", "Error:")} {error()}</div>
        </Show>

        <Show when={!loading() && !error() && files().length === 0}>
          <div class={s.empty}>
            {!props.repoPath
              ? t("diffPanel.noRepo", "No repository selected")
              : scope()
              ? t("diffPanel.noCommitChanges", "No changes in this commit")
              : t("diffPanel.noChanges", "No changes")}
          </div>
        </Show>

        <Show when={!loading() && !error() && files().length > 0 && filteredFiles().length === 0}>
          <div class={s.empty}>{t("diffPanel.noMatch", "No matching files")}</div>
        </Show>

        <Show when={!loading() && !error() && filteredFiles().length > 0}>
          <div class={p.fileList}>
            <For each={filteredFiles()}>
              {(file) => {
                const statusDisplay = getStatusDisplay(file.status);
                return (
                  <div
                    class={p.fileItem}
                    onClick={() => handleFileClick(file)}
                    title={file.path}
                  >
                    <div class={p.fileStatusContainer}>
                      <span class={cx(p.fileStatus, statusDisplay.className)}>
                        {statusDisplay.label}
                      </span>
                    </div>
                    <div class={p.fileName}>{file.path}</div>
                    <div class={p.fileStats}>
                      {formatStats(file.additions, file.deletions)}
                    </div>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
};

export default DiffPanel;
