import { Component, createEffect, createSignal, Show, Switch, Match } from "solid-js";
import { convertFileSrc } from "@tauri-apps/api/core";
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

// ---------------------------------------------------------------------------
// File type detection
// ---------------------------------------------------------------------------

type PreviewKind = "html" | "pdf" | "image" | "video" | "audio" | "text";

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "ico", "bmp"]);
const VIDEO_EXTS = new Set(["mp4", "webm", "ogg", "mov"]);
const AUDIO_EXTS = new Set(["mp3", "wav", "flac", "aac", "ogg", "m4a"]);
const TEXT_EXTS = new Set(["txt", "json", "csv", "log", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf"]);

function detectKind(fileName: string): PreviewKind {
  const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
  if (ext === "html" || ext === "htm") return "html";
  if (ext === "pdf") return "pdf";
  if (IMAGE_EXTS.has(ext)) return "image";
  if (VIDEO_EXTS.has(ext)) return "video";
  if (AUDIO_EXTS.has(ext)) return "audio";
  if (TEXT_EXTS.has(ext)) return "text";
  return "text"; // fallback: render as plain text
}

/** Resolve full absolute path from tab data */
function absolutePath(tab: HtmlPreviewTabData): string {
  const root = tab.fsRoot || tab.repoPath;
  return tab.filePath.startsWith("/") ? tab.filePath : `${root}/${tab.filePath}`;
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

  const kind = () => detectKind(props.tab.fileName);

  /** Asset URL for binary files (PDF, images, video, audio) */
  const assetUrl = () => convertFileSrc(absolutePath(props.tab));

  /** Read file content — used for HTML and text previews */
  const readFileContent = async (fsRoot: string | undefined, filePath: string): Promise<string> => {
    if (filePath.startsWith("/")) {
      return await invoke<string>("read_external_file", { path: filePath });
    }
    return fsRoot
      ? await repo.readFile(fsRoot, filePath)
      : await invoke<string>("read_external_file", { path: filePath });
  };

  // Load text content for html/text kinds
  createEffect(() => {
    const { repoPath, filePath, fsRoot } = props.tab;
    void (repoPath ? repositoriesStore.getRevision(repoPath) : 0);
    const k = kind();

    if (!filePath || (k !== "html" && k !== "text")) {
      setContent("");
      return;
    }

    if (!content()) setLoading(true);
    setError(null);

    (async () => {
      try {
        const fileContent = await readFileContent(fsRoot || repoPath, filePath);
        setContent(fileContent);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        appLogger.error("app", "File preview: read failed", { repoPath, filePath, error: msg });
        setError(msg);
        setContent("");
      } finally {
        setLoading(false);
      }
    })();
  });

  const handleOpenExternal = () => {
    openPath(absolutePath(props.tab)).catch((err) =>
      appLogger.error("app", "Failed to open file externally", { path: absolutePath(props.tab), error: String(err) }),
    );
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
        <button class={e.btn} onClick={handleOpenExternal} title="Open externally">
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
      <Show when={!loading() && !error()}>
        <Switch>
          <Match when={kind() === "html"}>
            <iframe
              class={s.iframe}
              sandbox=""
              srcdoc={content()}
              title={props.tab.fileName}
            />
          </Match>
          <Match when={kind() === "pdf"}>
            <iframe
              class={s.iframe}
              src={assetUrl()}
              title={props.tab.fileName}
            />
          </Match>
          <Match when={kind() === "image"}>
            <div class={s.mediaContainer}>
              <img class={s.image} src={assetUrl()} alt={props.tab.fileName} />
            </div>
          </Match>
          <Match when={kind() === "video"}>
            <div class={s.mediaContainer}>
              <video class={s.media} src={assetUrl()} controls />
            </div>
          </Match>
          <Match when={kind() === "audio"}>
            <div class={s.mediaContainer}>
              <audio src={assetUrl()} controls />
            </div>
          </Match>
          <Match when={kind() === "text"}>
            <pre class={s.textContent}>{content()}</pre>
          </Match>
        </Switch>
      </Show>
    </div>
  );
};

export default HtmlPreviewTab;
