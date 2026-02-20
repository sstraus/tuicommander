import { Component, createEffect, createSignal, For, Show, onCleanup } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { useFileBrowser } from "../../hooks/useFileBrowser";
import { getModifierSymbol } from "../../platform";
import type { DirEntry } from "../../types/fs";

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

export const FileBrowserPanel: Component<FileBrowserPanelProps> = (props) => {
  const [entries, setEntries] = createSignal<DirEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [currentSubdir, setCurrentSubdir] = createSignal(".");
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const fb = useFileBrowser();

  // Load entries when visible, repo changes, subdir changes, or repo content changes
  createEffect(() => {
    const visible = props.visible;
    const repoPath = props.repoPath;
    const subdir = currentSubdir();
    // Subscribe to repo revision for auto-refresh on git changes
    void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

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

  // Git status badge color
  const getStatusClass = (status: string): string => {
    switch (status) {
      case "modified": return "fb-status-modified";
      case "staged": return "fb-status-staged";
      case "untracked": return "fb-status-untracked";
      default: return "";
    }
  };

  // Keyboard navigation
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      // Only handle if the panel is focused (not terminal)
      const panel = document.getElementById("file-browser-panel");
      if (!panel?.contains(document.activeElement) && document.activeElement !== panel) return;

      const list = entries();
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
    <div id="file-browser-panel" class={props.visible ? "" : "hidden"} tabIndex={-1}>
      <div class="panel-header">
        <div class="panel-header-left">
          <span class="panel-title">Files</span>
          <Show when={!loading() && entries().length > 0}>
            <span class="file-count-badge">{entries().length}</span>
          </Show>
        </div>
        <button class="panel-close" onClick={props.onClose} title={`Close (${getModifierSymbol()}E)`}>
          &times;
        </button>
      </div>

      {/* Breadcrumb navigation */}
      <Show when={breadcrumbs().length > 0}>
        <div class="fb-breadcrumb">
          <span class="fb-breadcrumb-segment fb-breadcrumb-root" onClick={() => handleBreadcrumbClick(-1)}>
            /
          </span>
          <For each={breadcrumbs()}>
            {(segment, index) => (
              <>
                <span class="fb-breadcrumb-sep">/</span>
                <span
                  class="fb-breadcrumb-segment"
                  classList={{ "fb-breadcrumb-current": index() === breadcrumbs().length - 1 }}
                  onClick={() => handleBreadcrumbClick(index())}
                >
                  {segment}
                </span>
              </>
            )}
          </For>
        </div>
      </Show>

      <div class="panel-content">
        <Show when={loading()}>
          <div class="fb-empty">Loading...</div>
        </Show>

        <Show when={error()}>
          <div class="fb-empty fb-error">Error: {error()}</div>
        </Show>

        <Show when={!loading() && !error() && entries().length === 0}>
          <div class="fb-empty">
            {props.repoPath ? "Empty directory" : "No repository selected"}
          </div>
        </Show>

        <Show when={!loading() && !error() && entries().length > 0}>
          {/* Go up entry when in a subdirectory */}
          <Show when={currentSubdir() !== "." && currentSubdir() !== ""}>
            <div class="fb-entry fb-entry-up" onClick={navigateUp}>
              <span class="fb-entry-icon">..</span>
              <span class="fb-entry-name">(parent)</span>
            </div>
          </Show>

          <For each={entries()}>
            {(entry, index) => (
              <div
                class="fb-entry"
                classList={{
                  "fb-entry-dir": entry.is_dir,
                  "fb-entry-selected": selectedIndex() === index(),
                }}
                onClick={() => {
                  setSelectedIndex(index());
                  handleEntryClick(entry);
                }}
              >
                <span class="fb-entry-icon">{entry.is_dir ? "\u{1F4C1}" : "\u{1F4C4}"}</span>
                <span class="fb-entry-name" title={entry.path}>{entry.name}</span>
                <Show when={entry.git_status}>
                  <span class={`fb-status-dot ${getStatusClass(entry.git_status)}`} title={entry.git_status} />
                </Show>
                <Show when={!entry.is_dir && entry.size > 0}>
                  <span class="fb-entry-size">{formatSize(entry.size)}</span>
                </Show>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

export default FileBrowserPanel;
