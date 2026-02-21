import { Component, createEffect, createSignal } from "solid-js";
import { MarkdownRenderer } from "../ui";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
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

  return (
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
  );
};

export default MarkdownTab;
