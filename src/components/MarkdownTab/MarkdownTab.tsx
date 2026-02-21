import { Component, createEffect, createSignal, Show } from "solid-js";
import { MarkdownRenderer } from "../ui";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore, type MdTabData } from "../../stores/mdTabs";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";

export interface MarkdownTabProps {
  tab: MdTabData;
  onClose?: () => void;
}

export const MarkdownTab: Component<MarkdownTabProps> = (props) => {
  const [content, setContent] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();

  createEffect(() => {
    const tab = props.tab;

    if (tab.type === "file") {
      const { repoPath, filePath } = tab;
      // Re-run on git changes (file may have been modified externally)
      void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

      if (!repoPath || !filePath) {
        setContent("");
        return;
      }

      setLoading(true);
      setError(null);

      (async () => {
        try {
          const fileContent = await repo.readFile(repoPath, filePath);
          setContent(fileContent);
        } catch (err) {
          setError(String(err));
          setContent("");
        } finally {
          setLoading(false);
        }
      })();
    } else {
      // Virtual tab: resolve content through markdownProviderRegistry
      const { contentUri } = tab;

      setLoading(true);
      setError(null);

      (async () => {
        try {
          const result = await markdownProviderRegistry.resolve(contentUri);
          if (result === null) {
            setError("Content unavailable");
            setContent("");
          } else {
            setContent(result);
          }
        } catch (err) {
          setError(String(err));
          setContent("");
        } finally {
          setLoading(false);
        }
      })();
    }
  });

  /** Resolve a relative .md href against the current file's directory and open it.
   * Only applicable for file tabs. */
  const handleMdLink = (href: string) => {
    const tab = props.tab;
    if (tab.type !== "file") return;
    const currentDir = tab.filePath.includes("/")
      ? tab.filePath.slice(0, tab.filePath.lastIndexOf("/"))
      : "";
    const resolved = currentDir ? `${currentDir}/${href}` : href;
    mdTabsStore.add(tab.repoPath, resolved);
  };

  const handleEdit = () => {
    const tab = props.tab;
    if (tab.type === "file") {
      editorTabsStore.add(tab.repoPath, tab.filePath);
    }
  };

  const displayPath = () => {
    const tab = props.tab;
    return tab.type === "file" ? tab.filePath : tab.title;
  };

  return (
    <div class="md-tab-wrapper">
      <div class="editor-header">
        <span class="editor-filename" title={displayPath()}>
          {displayPath()}
        </span>
        <Show when={props.tab.type === "file"}>
          <button class="editor-btn" onClick={handleEdit} title="Edit file">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M11.13 1.47a1.5 1.5 0 0 1 2.12 0l1.28 1.28a1.5 1.5 0 0 1 0 2.12L5.9 13.5a1 1 0 0 1-.5.27l-3.5.87a.5.5 0 0 1-.6-.6l.87-3.5a1 1 0 0 1 .27-.5L11.13 1.47ZM12.2 2.53l-8.46 8.47-.58 2.34 2.34-.58 8.47-8.46-1.77-1.77Z"/>
            </svg>
            {" Edit"}
          </button>
        </Show>
      </div>
      <div class="md-tab-content">
        <MarkdownRenderer
          content={content()}
          onLinkClick={handleMdLink}
          emptyMessage={
            loading()
              ? "Loading..."
              : error()
                ? `Error: ${error()}`
                : "No content"
          }
        />
      </div>
    </div>
  );
};

export default MarkdownTab;
