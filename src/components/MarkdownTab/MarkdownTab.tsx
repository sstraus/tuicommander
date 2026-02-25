import { Component, createEffect, createSignal, Show, onMount } from "solid-js";
import { MarkdownRenderer } from "../ui";
import { appLogger } from "../../stores/appLogger";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { editorTabsStore } from "../../stores/editorTabs";
import { invoke } from "../../invoke";
import { mdTabsStore, type MdTabData } from "../../stores/mdTabs";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { t } from "../../i18n";
import e from "../shared/editor-header.module.css";
import s from "./MarkdownTab.module.css";

export interface MarkdownTabProps {
  tab: MdTabData;
  onClose?: () => void;
}

export const MarkdownTab: Component<MarkdownTabProps> = (props) => {
  const [content, setContent] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();
  const contextMenu = createContextMenu();
  let wrapperRef: HTMLDivElement | undefined;

  // When this tab is active, focus the wrapper so wheel events route by cursor
  // position rather than following xterm's retained textarea focus.
  // We focus .wrapper (no overflow) not .content (overflow:auto) to avoid
  // triggering a spurious scrollbar.
  const focusWrapper = () => requestAnimationFrame(() => wrapperRef?.focus({ preventScroll: true }));

  onMount(() => {
    if (mdTabsStore.state.activeId === props.tab.id) focusWrapper();
  });

  createEffect(() => {
    if (mdTabsStore.state.activeId === props.tab.id) focusWrapper();
  });

  createEffect(() => {
    const tab = props.tab;

    if (tab.type === "file") {
      const { repoPath, filePath } = tab;
      // Re-run on git changes (file may have been modified externally)
      void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

      if (!filePath) {
        setContent("");
        return;
      }

      setLoading(true);
      setError(null);

      (async () => {
        try {
          // Absolute path without repo (e.g. plugin README) — read directly
          const fileContent = repoPath
            ? await repo.readFile(repoPath, filePath)
            : await invoke<string>("plugin_read_file", { path: filePath, pluginId: "_system" });
          setContent(fileContent);
        } catch (err) {
          setError(String(err));
          setContent("");
        } finally {
          setLoading(false);
        }
      })();
    } else if (tab.type === "virtual") {
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
    } else {
      // Plugin panel tab — HTML content rendered by PluginPanel, not here
      setContent("");
      setLoading(false);
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

  /** Absolute directory containing the source file, for resolving relative image paths */
  const baseDir = () => {
    const tab = props.tab;
    if (tab.type !== "file") return undefined;
    // Absolute path without repo (e.g. plugin README) — directory is the parent
    if (!tab.repoPath && tab.filePath.startsWith("/")) {
      const lastSlash = tab.filePath.lastIndexOf("/");
      return lastSlash > 0 ? tab.filePath.slice(0, lastSlash) : "/";
    }
    const dir = tab.filePath.includes("/")
      ? tab.filePath.slice(0, tab.filePath.lastIndexOf("/"))
      : "";
    return dir ? `${tab.repoPath}/${dir}` : tab.repoPath;
  };

  /** Full path for clipboard copy (repoPath + filePath) */
  const fullPath = () => {
    const tab = props.tab;
    if (tab.type !== "file") return null;
    return tab.repoPath ? `${tab.repoPath}/${tab.filePath}` : tab.filePath;
  };

  const handleCopyPath = () => {
    const path = fullPath();
    if (!path) return;
    navigator.clipboard.writeText(path).catch((err) =>
      appLogger.error("app", "Failed to copy path", err),
    );
  };

  const handleHeaderContextMenu = (ev: MouseEvent) => {
    if (!fullPath()) return;
    ev.preventDefault();
    contextMenu.open(ev);
  };

  return (
    <div ref={wrapperRef} class={s.wrapper} tabIndex={-1}>
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
      <div class={s.content}>
        <MarkdownRenderer
          content={content()}
          baseDir={baseDir()}
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
