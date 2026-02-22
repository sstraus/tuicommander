import { Component, createEffect, createSignal, onMount, Show } from "solid-js";
import { MarkdownRenderer } from "../ui";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore, type MdTabData } from "../../stores/mdTabs";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { t } from "../../i18n";
import e from "../shared/editor-header.module.css";
import s from "./MarkdownTab.module.css";

export interface MarkdownTabProps {
  tab: MdTabData;
  active?: boolean;
  onClose?: () => void;
}

export const MarkdownTab: Component<MarkdownTabProps> = (props) => {
  const [content, setContent] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();
  const contextMenu = createContextMenu();
  let contentRef!: HTMLDivElement;

  // Focus the scroll container when this tab is active so wheel events route here, not the terminal canvas
  onMount(() => contentRef?.focus({ preventScroll: true }));
  createEffect(() => {
    if (props.active) contentRef?.focus({ preventScroll: true });
  });

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

  /** Full path for clipboard copy (repoPath + filePath) */
  const fullPath = () => {
    const tab = props.tab;
    if (tab.type !== "file") return null;
    return `${tab.repoPath}/${tab.filePath}`;
  };

  const handleCopyPath = () => {
    const path = fullPath();
    if (!path) return;
    navigator.clipboard.writeText(path).catch((err) =>
      console.error("Failed to copy path:", err),
    );
  };

  const handleHeaderContextMenu = (ev: MouseEvent) => {
    if (!fullPath()) return;
    ev.preventDefault();
    contextMenu.open(ev);
  };

  return (
    <div class={s.wrapper}>
      <div class={e.header} onContextMenu={handleHeaderContextMenu}>
        <span class={e.filename} title={displayPath()}>
          {displayPath()}
        </span>
        <Show when={props.tab.type === "file"}>
          <button class={e.btn} onClick={handleEdit} title={t("markdownTab.edit", "Edit file")}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M11.13 1.47a1.5 1.5 0 0 1 2.12 0l1.28 1.28a1.5 1.5 0 0 1 0 2.12L5.9 13.5a1 1 0 0 1-.5.27l-3.5.87a.5.5 0 0 1-.6-.6l.87-3.5a1 1 0 0 1 .27-.5L11.13 1.47ZM12.2 2.53l-8.46 8.47-.58 2.34 2.34-.58 8.47-8.46-1.77-1.77Z"/>
            </svg>
            {" "}{t("markdownTab.editBtn", "Edit")}
          </button>
        </Show>
      </div>
      <div ref={contentRef} tabIndex={-1} class={s.content}>
        <MarkdownRenderer
          content={content()}
          onLinkClick={handleMdLink}
          emptyMessage={
            loading()
              ? t("markdownTab.loading", "Loading...")
              : error()
                ? `${t("markdownTab.error", "Error:")} ${error()}`
                : t("markdownTab.noContent", "No content")
          }
        />
      </div>

      <ContextMenu
        items={[{ label: t("markdownTab.copyPath", "Copy Path"), action: handleCopyPath }]}
        x={contextMenu.position().x}
        y={contextMenu.position().y}
        visible={contextMenu.visible()}
        onClose={contextMenu.close}
      />
    </div>
  );
};

export default MarkdownTab;
