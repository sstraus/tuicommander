import { Component, Show, createEffect, createSignal, on, onCleanup } from "solid-js";
import { createCodeMirror, createEditorControlledValue, createEditorReadonly } from "solid-codemirror";
import { lineNumbers, drawSelection, highlightActiveLine, highlightActiveLineGutter, keymap } from "@codemirror/view";
import { defaultKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import { search, searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import type { Extension } from "@codemirror/state";
import type { LanguageSupport } from "@codemirror/language";
import { editorTabsStore } from "../../stores/editorTabs";
import { repositoriesStore } from "../../stores/repositories";
import { appLogger } from "../../stores/appLogger";
import { useFileBrowser } from "../../hooks/useFileBrowser";
import { invoke } from "../../invoke";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import { codeEditorTheme } from "./theme";
import { detectLanguage } from "./languageDetection";
import { t } from "../../i18n";
import e from "../shared/editor-header.module.css";
import s from "./CodeEditorTab.module.css";

export interface CodeEditorTabProps {
  id: string;
  repoPath: string;
  filePath: string;
  onClose?: () => void;
}

/** Large file threshold — skip syntax highlighting above this size */
const LARGE_FILE_BYTES = 500 * 1024;

export const CodeEditorTab: Component<CodeEditorTabProps> = (props) => {
  const [langSupport, setLangSupport] = createSignal<LanguageSupport | null>(null);
  /** Signal for external content pushes (disk load/reload) — drives createEditorControlledValue */
  const [code, setCode] = createSignal("");
  /** Mutable ref tracking live editor value without triggering reactivity on every keystroke */
  let currentCode = "";
  const [savedContent, setSavedContent] = createSignal("");
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [isReadOnly, setIsReadOnly] = createSignal(false);
  /** True when the file changed on disk while editor has unsaved changes */
  const [diskConflict, setDiskConflict] = createSignal(false);
  /** Reactive dirty flag — only transitions on save/load, not every keystroke */
  const [dirty, setDirty] = createSignal(false);
  const contextMenu = createContextMenu();
  const fb = useFileBrowser();

  /** True when the file path is absolute (outside the repository) */
  const isExternal = () => props.filePath.startsWith("/");

  /** Read file content — uses the right command depending on internal vs external */
  const readContent = async (): Promise<string> => {
    if (isExternal()) {
      return invoke<string>("read_external_file", { path: props.filePath });
    }
    return fb.readFile(props.repoPath, props.filePath);
  };

  // Sync dirty state to tab store for the tab bar indicator
  createEffect(() => {
    editorTabsStore.setDirty(props.id, dirty());
  });

  // Load file content
  createEffect(
    on(
      () => [props.repoPath, props.filePath] as const,
      async ([repoPath, filePath]) => {
        if (!repoPath || !filePath) return;

        setLoading(true);
        setError(null);
        if (isExternal()) setIsReadOnly(true);

        try {
          const content = await readContent();
          currentCode = content;
          setCode(content);
          setSavedContent(content);
          setDirty(false);
        } catch (err) {
          setError(String(err));
          currentCode = "";
          setCode("");
          setSavedContent("");
          setDirty(false);
        } finally {
          setLoading(false);
        }
      },
    ),
  );

  /** Check disk content and reload or show conflict banner */
  const checkDiskContent = async () => {
    if (!savedContent()) return;
    try {
      const diskContent = await readContent();
      if (diskContent === savedContent()) return;

      if (currentCode !== savedContent()) {
        setDiskConflict(true);
      } else {
        currentCode = diskContent;
        setCode(diskContent);
        setSavedContent(diskContent);
        setDirty(false);
      }
    } catch (err) {
      appLogger.debug("app", `checkDiskContent failed (file may be deleted): ${props.filePath}`, err);
    }
  };

  // Re-check file content on git changes (revision signal)
  createEffect(() => {
    const repoPath = props.repoPath;
    if (!repoPath || isExternal()) return;
    const rev = repositoriesStore.getRevision(repoPath);
    if (rev === 0 || !savedContent()) return;
    void checkDiskContent();
  });

  // Poll for file changes (agent edits, external tools).
  // 5s interval, skip while tab is hidden to avoid competing with terminal I/O.
  createEffect(() => {
    if (!props.filePath) return;
    const timer = setInterval(() => {
      if (document.visibilityState === "hidden") return;
      void checkDiskContent();
    }, 5000);
    onCleanup(() => clearInterval(timer));
  });

  /** Reload content from disk, discarding local changes */
  const handleReloadFromDisk = async () => {
    try {
      const diskContent = await readContent();
      currentCode = diskContent;
      setCode(diskContent);
      setSavedContent(diskContent);
      setDirty(false);
      setDiskConflict(false);
    } catch (err) {
      appLogger.error("app", "Failed to reload file", err);
    }
  };

  /** Keep local changes, dismiss the conflict banner (next save will overwrite disk) */
  const handleKeepLocal = () => {
    setDiskConflict(false);
  };

  const { ref, editorView, createExtension } = createCodeMirror({
    onValueChange: (value) => {
      currentCode = value;
      const nowDirty = value !== savedContent();
      if (nowDirty !== dirty()) setDirty(nowDirty);
    },
  });

  // Controlled value — sync external changes into editor
  createEditorControlledValue(editorView, code);

  // Read-only mode
  createEditorReadonly(editorView, isReadOnly);

  // Base extensions
  createExtension(codeEditorTheme);
  createExtension(lineNumbers());
  createExtension(drawSelection());
  createExtension(highlightActiveLine());
  createExtension(highlightActiveLineGutter());
  createExtension(bracketMatching());
  createExtension(indentOnInput());
  createExtension(keymap.of([...defaultKeymap, ...searchKeymap, indentWithTab]));
  createExtension(search());
  createExtension(highlightSelectionMatches());

  // Reactive language extension
  createExtension((): Extension => langSupport() ?? []);

  // Force CodeMirror to recalculate layout when the editor container resizes.
  // The container starts as display:none (.terminal-pane without .active),
  // so CodeMirror computes zero dimensions during initial mount. When the
  // container becomes visible (0→real size), ResizeObserver fires and we
  // tell CodeMirror to re-measure. We also re-measure when loading completes
  // (display:none → visible transition on the editor div itself).
  let editorDiv: HTMLDivElement | undefined;
  createEffect(() => {
    const view = editorView();
    if (!view || !editorDiv) return;
    const ro = new ResizeObserver(() => {
      // Use rAF to ensure the browser has completed the layout pass before
      // CodeMirror measures. Plain requestMeasure() can run too early after
      // a display:none → block transition.
      requestAnimationFrame(() => view.requestMeasure());
    });
    ro.observe(editorDiv);
    onCleanup(() => ro.disconnect());
  });

  // Load language support
  createEffect(
    on(
      () => props.filePath,
      async (filePath) => {
        if (!filePath) {
          setLangSupport(null);
          return;
        }
        // Skip syntax highlighting for large files
        if (currentCode.length > LARGE_FILE_BYTES) {
          setLangSupport(null);
          return;
        }
        const lang = await detectLanguage(filePath);
        setLangSupport(lang);
      },
    ),
  );

  // Save handler
  const handleSave = async () => {
    if (!dirty() || isReadOnly()) return;
    try {
      await fb.writeFile(props.repoPath, props.filePath, currentCode);
      setSavedContent(currentCode);
      setDirty(false);
      // Notify revision-subscribed panels (e.g. MarkdownTab) that a file changed on disk
      if (props.repoPath) {
        repositoriesStore.bumpRevision(props.repoPath);
      }
    } catch (err) {
      appLogger.error("app", "Failed to save file", err);
    }
  };

  // Cmd+S save shortcut
  createEffect(() => {
    const handleKeydown = (e: KeyboardEvent) => {
      const isMeta = e.metaKey || e.ctrlKey;
      if (isMeta && e.key === "s") {
        // Only handle if this tab's container has focus
        const container = document.querySelector(`[data-editor-tab-id="${props.id}"]`);
        if (!container?.contains(document.activeElement)) return;

        e.preventDefault();
        void handleSave();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  return (
    <div class={s.tabContent} data-editor-tab-id={props.id}>
      <div class={e.header} onContextMenu={(ev) => { ev.preventDefault(); contextMenu.open(ev); }}>
        <span class={e.filename} title={props.filePath}>
          {props.filePath}
        </span>
        <Show when={dirty()}>
          <span class={e.dirtyDot} title={t("codeEditor.unsaved", "Unsaved changes")} />
        </Show>
        <button
          class={e.btn}
          onClick={() => { if (!isExternal()) setIsReadOnly((v) => !v); }}
          title={isExternal() ? t("codeEditor.external", "External file (read-only)") : isReadOnly() ? t("codeEditor.unlock", "Unlock editing") : t("codeEditor.lock", "Lock (read-only)")}
        >
          {isReadOnly()
            ? <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a3 3 0 0 0-3 3v3H4a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-1V4a3 3 0 0 0-3-3zm1.5 6H6.5V4a1.5 1.5 0 0 1 3 0v3z"/></svg>
            : <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a3 3 0 0 1 3 3v1h.5a1.5 1.5 0 0 1 1.5 1.5V14a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6.5A1.5 1.5 0 0 1 4.5 5H5V4a3 3 0 0 1 3-3zm1.5 4V4a1.5 1.5 0 0 0-3 0v1h3z"/></svg>
          }
        </button>
        <Show when={dirty() && !isReadOnly()}>
          <button class={e.btn} onClick={handleSave} title={`${t("codeEditor.save", "Save")} (${"\u2318"}S)`}>
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M13.354 1.146a.5.5 0 0 1 .146.354v12a1.5 1.5 0 0 1-1.5 1.5h-8A1.5 1.5 0 0 1 2.5 13.5v-11A1.5 1.5 0 0 1 4 1h8.5a.5.5 0 0 1 .354.146L13.354 1.146zM4 2.5a.5.5 0 0 0-.5.5v10.5a.5.5 0 0 0 .5.5h1V10a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v4h1a.5.5 0 0 0 .5-.5V2.207L11.793 2H11v2.5A1.5 1.5 0 0 1 9.5 6h-3A1.5 1.5 0 0 1 5 4.5V2H4zm2 0v2.5a.5.5 0 0 0 .5.5h3a.5.5 0 0 0 .5-.5V2H6zm0 8v4h4v-4H6z"/></svg>
          </button>
        </Show>
      </div>

      <Show when={diskConflict()}>
        <div class={s.conflictBanner}>
          <span>{t("codeEditor.fileChanged", "File changed on disk.")}</span>
          <button class={e.btn} onClick={handleReloadFromDisk}>{t("codeEditor.reload", "Reload")}</button>
          <button class={e.btn} onClick={handleKeepLocal}>{t("codeEditor.keepMine", "Keep mine")}</button>
        </div>
      </Show>

      <Show when={loading()}>
        <div class={s.empty}>{t("codeEditor.loading", "Loading...")}</div>
      </Show>

      <Show when={error()}>
        <div class={s.empty} style={{ color: "var(--error)" }}>
          {t("codeEditor.error", "Error:")} {error()}
        </div>
      </Show>

      {/* Always mount the editor div so solid-codemirror's ref callback fires during
          initial component mount. Wrapping in <Show> defers the ref, causing onMount
          inside createCodeMirror to never fire in production builds — the editorView
          signal stays undefined and content/extensions are never applied. */}
      <div class={s.editorContent} ref={(el) => { editorDiv = el; ref(el); }} style={{ display: loading() || error() ? "none" : undefined }} />

      <ContextMenu
        items={[{
          label: t("codeEditor.copyPath", "Copy Path"),
          action: () => {
            const fullPath = isExternal() ? props.filePath : `${props.repoPath}/${props.filePath}`;
            navigator.clipboard.writeText(fullPath).catch((err) =>
              appLogger.error("app", "Failed to copy path", err),
            );
          },
        }]}
        x={contextMenu.position().x}
        y={contextMenu.position().y}
        visible={contextMenu.visible()}
        onClose={contextMenu.close}
      />
    </div>
  );
};

export default CodeEditorTab;
