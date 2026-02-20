import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { useRepository, type ChangedFile } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { getModifierSymbol } from "../../platform";

export interface DiffPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
}

export const DiffPanel: Component<DiffPanelProps> = (props) => {
  const [files, setFiles] = createSignal<ChangedFile[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();

  // Load changed files when visible, repo changes, or repo content changes
  createEffect(() => {
    const visible = props.visible;
    const repoPath = props.repoPath;
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
        const changedFiles = await repo.getChangedFiles(repoPath);
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
    diffTabsStore.add(props.repoPath, file.path, file.status);
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

  return (
    <div id="diff-panel" class={props.visible ? "" : "hidden"}>
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

      <div class="panel-content">
        <Show when={loading()}>
          <div class="diff-empty">Loading changes...</div>
        </Show>

        <Show when={error()}>
          <div class="diff-empty error">Error: {error()}</div>
        </Show>

        <Show when={!loading() && !error() && files().length === 0}>
          <div class="diff-empty">
            {props.repoPath ? "No changes" : "No repository selected"}
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
