import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { mdTabsStore } from "../../stores/mdTabs";
import { getModifierSymbol } from "../../platform";
import { globToRegex } from "../../utils";

/** Markdown file entry from Rust backend */
interface MdFileEntry {
  path: string;
  git_status: string;
}

export interface MarkdownPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
}

/** Git status badge CSS class (shared pattern with FileBrowser) */
const getStatusClass = (status: string): string => {
  switch (status) {
    case "modified": return "fb-status-modified";
    case "staged": return "fb-status-staged";
    case "untracked": return "fb-status-untracked";
    default: return "";
  }
};

export const MarkdownPanel: Component<MarkdownPanelProps> = (props) => {
  const [files, setFiles] = createSignal<MdFileEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [searchQuery, setSearchQuery] = createSignal("");
  const repo = useRepository();

  /** Files filtered by search query (supports glob wildcards) */
  const filteredFiles = () => {
    const q = searchQuery().trim();
    if (!q) return files();
    const re = globToRegex(q);
    return files().filter((f) => re.test(f.path));
  };

  // Load markdown files when visible, repo changes, or repo content changes
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
  const handleFileClick = (filePath: string) => {
    if (!props.repoPath) return;
    mdTabsStore.add(props.repoPath, filePath);
  };

  // Group files by directory for tree view
  const groupedFiles = () => {
    const allFiles = filteredFiles();
    const groups: Record<string, MdFileEntry[]> = {};

    for (const entry of allFiles) {
      const parts = entry.path.split("/");
      if (parts.length === 1) {
        if (!groups["/"]) groups["/"] = [];
        groups["/"].push(entry);
      } else {
        const dir = parts.slice(0, -1).join("/");
        if (!groups[dir]) groups[dir] = [];
        groups[dir].push(entry);
      }
    }

    return groups;
  };

  return (
    <div id="markdown-panel" class={props.visible ? "" : "hidden"}>
      <div class="panel-header">
        <div class="panel-header-left">
          <span class="panel-title">Markdown Files</span>
          <Show when={!loading() && filteredFiles().length > 0}>
            <span class="file-count-badge">{filteredFiles().length}</span>
          </Show>
        </div>
        <button class="panel-close" onClick={props.onClose} title={`Close (${getModifierSymbol()}M)`}>
          &times;
        </button>
      </div>

      {/* Search filter */}
      <div class="panel-search">
        <input
          type="text"
          class="panel-search-input"
          placeholder="Filter... (*, ** wildcards)"
          value={searchQuery()}
          onInput={(e) => setSearchQuery(e.currentTarget.value)}
        />
        <Show when={searchQuery()}>
          <button class="panel-search-clear" onClick={() => setSearchQuery("")}>&times;</button>
        </Show>
      </div>

      <div class="panel-content">
        <Show when={loading()}>
          <div class="markdown-loading">Loading files...</div>
        </Show>

        <Show when={error()}>
          <div class="markdown-error">Error: {error()}</div>
        </Show>

        <Show when={!loading() && !error() && filteredFiles().length === 0}>
          <div class="markdown-empty">
            {!props.repoPath ? "No repository selected" : searchQuery() ? "No matches" : "No markdown files found"}
          </div>
        </Show>

        <Show when={!loading() && !error() && filteredFiles().length > 0}>
          <div class="file-list md-file-list">
            <For each={Object.entries(groupedFiles()).sort(([a], [b]) => a.localeCompare(b))}>
              {([dir, dirEntries]) => (
                <>
                  <Show when={dir !== "/"}>
                    <div class="md-dir-header">{dir}/</div>
                  </Show>
                  <For each={dirEntries.sort((a, b) => a.path.localeCompare(b.path))}>
                    {(entry) => {
                      const fileName = entry.path.split("/").pop() || entry.path;
                      return (
                        <div
                          class="file-item md-file-item"
                          onClick={() => handleFileClick(entry.path)}
                          title={entry.path}
                        >
                          <span class="md-file-icon">{"\u{1F4C4}"}</span>
                          <div class="file-name">{fileName}</div>
                          <Show when={entry.git_status}>
                            <span class={`fb-status-dot ${getStatusClass(entry.git_status)}`} title={entry.git_status} />
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
    </div>
  );
};

export default MarkdownPanel;
