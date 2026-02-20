import { Component, createEffect, createSignal } from "solid-js";
import { MarkdownRenderer } from "../ui";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { mdTabsStore } from "../../stores/mdTabs";

export interface MarkdownTabProps {
  repoPath: string;
  filePath: string;
  onClose?: () => void;
}

export const MarkdownTab: Component<MarkdownTabProps> = (props) => {
  const [content, setContent] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();

  // Load file content when props change or repo content changes
  createEffect(() => {
    const repoPath = props.repoPath;
    const filePath = props.filePath;
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
  });

  /** Resolve a relative .md href against the current file's directory and open it */
  const handleMdLink = (href: string) => {
    // Resolve relative path against the directory of the current file
    const currentDir = props.filePath.includes("/")
      ? props.filePath.slice(0, props.filePath.lastIndexOf("/"))
      : "";
    const resolved = currentDir ? `${currentDir}/${href}` : href;
    mdTabsStore.add(props.repoPath, resolved);
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
