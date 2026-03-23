import { Component, createEffect, createSignal, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { useRepository } from "../../hooks/useRepository";
import { repositoriesStore } from "../../stores/repositories";
import { invoke } from "../../invoke";
import { mdTabsStore, type HtmlPreviewTab as HtmlPreviewTabData } from "../../stores/mdTabs";
import { shortenHomePath } from "../../platform";
import { openPath } from "@tauri-apps/plugin-opener";
import e from "../shared/editor-header.module.css";
import s from "./HtmlPreviewTab.module.css";

export interface HtmlPreviewTabProps {
  tab: HtmlPreviewTabData;
  onClose?: () => void;
}

export const HtmlPreviewTab: Component<HtmlPreviewTabProps> = (props) => {
  const [content, setContent] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const repo = useRepository();
  let wrapperRef: HTMLDivElement | undefined;

  const focusWrapper = () => requestAnimationFrame(() => wrapperRef?.focus({ preventScroll: true }));

  createEffect(() => {
    if (mdTabsStore.state.activeId === props.tab.id) focusWrapper();
  });

  /** Read file content — uses repo-scoped read for relative paths, external read for absolute. */
  const readFileContent = async (fsRoot: string | undefined, filePath: string): Promise<string> => {
    if (filePath.startsWith("/")) {
      return await invoke<string>("read_external_file", { path: filePath });
    }
    return fsRoot
      ? await repo.readFile(fsRoot, filePath)
      : await invoke<string>("read_external_file", { path: filePath });
  };

  createEffect(() => {
    const { repoPath, filePath, fsRoot } = props.tab;
    void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);

    if (!filePath) {
      setContent("");
      return;
    }

    setLoading(true);
    setError(null);

    (async () => {
      try {
        const fileContent = await readFileContent(fsRoot || repoPath, filePath);
        setContent(fileContent);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        appLogger.error("app", "HTML preview: read failed", { repoPath, filePath, error: msg });
        setError(msg);
        setContent("");
      } finally {
        setLoading(false);
      }
    })();
  });

  const handleOpenInBrowser = () => {
    const { fsRoot, repoPath, filePath } = props.tab;
    const root = fsRoot || repoPath;
    const absolutePath = filePath.startsWith("/") ? filePath : `${root}/${filePath}`;
    openPath(absolutePath);
  };

  const displayPath = () => {
    const { fsRoot, repoPath, filePath } = props.tab;
    const root = fsRoot || repoPath;
    return filePath.startsWith("/") ? shortenHomePath(filePath) : `${shortenHomePath(root)}/${filePath}`;
  };

  return (
    <div class={s.wrapper} ref={wrapperRef} tabIndex={-1}>
      <div class={e.header}>
        <span class={e.filename} title={displayPath()}>{props.tab.fileName}</span>
        <button class={e.btn} onClick={handleOpenInBrowser} title="Open in browser">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M9 2h5v5l-2-2-3 3-2-2 3-3zm-3 7l-3 3 2 2H0V9l2 2 3-3z"/>
          </svg>
        </button>
      </div>
      <Show when={loading()}>
        <div style={{ padding: "24px", color: "var(--fg-muted)" }}>Loading...</div>
      </Show>
      <Show when={error()}>
        <div style={{ padding: "24px", color: "var(--error)" }}>{error()}</div>
      </Show>
      <Show when={!loading() && !error() && content()}>
        <iframe
          class={s.iframe}
          sandbox="allow-same-origin"
          srcdoc={content()}
          title={props.tab.fileName}
        />
      </Show>
    </div>
  );
};

export default HtmlPreviewTab;
