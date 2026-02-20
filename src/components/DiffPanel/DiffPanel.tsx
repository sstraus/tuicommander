import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { useRepository, type ChangedFile } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { getModifierSymbol } from "../../platform";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";

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
  const [scope, setScope] = createSignal<string | undefined>(undefined); // undefined = working tree, hash = specific commit
  const [commits, setCommits] = createSignal<RecentCommit[]>([]);
  const repo = useRepository();

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
    // Track repo revision so this effect re-runs on git operations
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

  // Handle file click - open diff in new tab
  const handleFileClick = (file: ChangedFile) => {
    if (!props.repoPath) return;
    diffTabsStore.add(props.repoPath, file.path, file.status, scope());
  };

  // Get status badge color and label
  const getStatusDisplay = (status: string): { label: string; className: string } => {
    switch (status) {
      case "M":
        return { label: "M", className: "status-modified" };
      case "A":
        return { label: "A", className: "status-added" };
      case "D":
        return { label: "D", className: "status-deleted" };
      case "R":
        return { label: "R", className: "status-renamed" };
      default:
        return { label: status, className: "status-unknown" };
    }
  };

  // Format stats display
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
    <div id="diff-panel" class={props.visible ? "" : "hidden"}>
      <PanelResizeHandle panelId="diff-panel" />
      <div class="panel-header">
        <div class="panel-header-left">
          <span class="panel-title">Changes</span>
          <Show when={!loading() && files().length > 0}>
            <span class="file-count-badge">{files().length}</span>
          </Show>
        </div>
        <button class="panel-close" onClick={props.onClose} title={`Close (${getModifierSymbol()}D)`}>
          &times;
        </button>
      </div>

      {/* Scope selector: Working tree or a recent commit */}
      <div class="diff-scope-bar">
        <select class="diff-scope-select" value={scope() ?? ""} onChange={handleScopeChange}>
          <option value="">Working tree</option>
          <For each={commits()}>
            {(commit) => (
              <option value={commit.hash}>
                {commit.short_hash} {commit.subject}
              </option>
            )}
          </For>
        </select>
      </div>

      <div class="panel-content">
        <Show when={loading()}>
          <div class="diff-empty">Loading changes...</div>
        </Show>

        <Show when={error()}>
          <div class="diff-empty error">Error: {error()}</div>
        </Show>

        <Show when={!loading() && !error() && files().length === 0}>
          <div class="diff-empty">
            {!props.repoPath ? "No repository selected" : scope() ? "No changes in this commit" : "No changes"}
          </div>
        </Show>

        <Show when={!loading() && !error() && files().length > 0}>
          <div class="file-list">
            <For each={files()}>
              {(file) => {
                const statusDisplay = getStatusDisplay(file.status);
                return (
                  <div
                    class="file-item"
                    onClick={() => handleFileClick(file)}
                    title={file.path}
                  >
                    <div class="file-status-container">
                      <span class={`file-status ${statusDisplay.className}`}>
                        {statusDisplay.label}
                      </span>
                    </div>
                    <div class="file-name">{file.path}</div>
                    <div class="file-stats">
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
