import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { mdTabsStore } from "../../stores/mdTabs";
import { getModifierSymbol } from "../../platform";

export interface MarkdownPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
}

export const MarkdownPanel: Component<MarkdownPanelProps> = (props) => {
  const [files, setFiles] = createSignal<string[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();

  // Load markdown files when visible and repo changes
  createEffect(() => {
    const visible = props.visible;
    const repoPath = props.repoPath;

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

  // Handle file click - open markdown in new tab
  const handleFileClick = (file: string) => {
    if (!props.repoPath) return;
    mdTabsStore.add(props.repoPath, file);
  };

  // Group files by directory for tree view
  const groupedFiles = () => {
    const allFiles = files();
    const groups: Record<string, string[]> = {};

    for (const file of allFiles) {
      const parts = file.split("/");
      if (parts.length === 1) {
        // Root level file
        if (!groups["/"]) groups["/"] = [];
        groups["/"].push(file);
      } else {
        // File in subdirectory
        const dir = parts.slice(0, -1).join("/");
        if (!groups[dir]) groups[dir] = [];
        groups[dir].push(file);
      }
    }

    return groups;
  };

  return (
    <div id="markdown-panel" class={props.visible ? "" : "hidden"}>
      <div class="panel-header">
        <div class="panel-header-left">
          <span class="panel-title">Markdown Files</span>
          <Show when={!loading() && files().length > 0}>
            <span class="file-count-badge">{files().length}</span>
          </Show>
        </div>
        <button class="panel-close" onClick={props.onClose} title={`Close (${getModifierSymbol()}M)`}>
          &times;
        </button>
      </div>

      <div class="panel-content">
        <Show when={loading()}>
          <div class="markdown-loading">Loading files...</div>
        </Show>

        <Show when={error()}>
          <div class="markdown-error">Error: {error()}</div>
        </Show>

        <Show when={!loading() && !error() && files().length === 0}>
          <div class="markdown-empty">
            {props.repoPath ? "No markdown files found" : "No repository selected"}
          </div>
        </Show>

        <Show when={!loading() && !error() && files().length > 0}>
          <div class="file-list md-file-list">
            <For each={Object.entries(groupedFiles()).sort(([a], [b]) => a.localeCompare(b))}>
              {([dir, dirFiles]) => (
                <>
                  <Show when={dir !== "/"}>
                    <div class="md-dir-header">{dir}/</div>
                  </Show>
                  <For each={dirFiles.sort()}>
                    {(file) => {
                      const fileName = file.split("/").pop() || file;
                      return (
                        <div
                          class="file-item md-file-item"
                          onClick={() => handleFileClick(file)}
                          title={file}
                        >
                          <span class="md-file-icon">📄</span>
                          <div class="file-name">{fileName}</div>
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
    </div>
  );
};

export default MarkdownPanel;
