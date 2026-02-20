import { Component, Show, createEffect, createSignal, on, onCleanup } from "solid-js";
import { createCodeMirror, createEditorControlledValue, createEditorReadonly } from "solid-codemirror";
import { lineNumbers, drawSelection, highlightActiveLine, highlightActiveLineGutter, keymap } from "@codemirror/view";
import { defaultKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import type { LanguageSupport } from "@codemirror/language";
import { editorTabsStore } from "../../stores/editorTabs";
import { repositoriesStore } from "../../stores/repositories";
import { useFileBrowser } from "../../hooks/useFileBrowser";
import { codeEditorTheme } from "./theme";
import { detectLanguage } from "./languageDetection";

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
  const [code, setCode] = createSignal("");
  const [savedContent, setSavedContent] = createSignal("");
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [isReadOnly, setIsReadOnly] = createSignal(false);
  /** True when the file changed on disk while editor has unsaved changes */
  const [diskConflict, setDiskConflict] = createSignal(false);
  const fb = useFileBrowser();

  const isDirty = () => code() !== savedContent();

  // Sync dirty state to tab store for the tab bar indicator
  createEffect(() => {
    editorTabsStore.setDirty(props.id, isDirty());
  });

  // Load file content
  createEffect(
    on(
      () => [props.repoPath, props.filePath] as const,
      async ([repoPath, filePath]) => {
        if (!repoPath || !filePath) return;

        setLoading(true);
        setError(null);

        try {
          const content = await fb.readFile(repoPath, filePath);
          setCode(content);
          setSavedContent(content);
        } catch (err) {
          setError(String(err));
          setCode("");
          setSavedContent("");
        } finally {
          setLoading(false);
        }
      },
    ),
  );

  // Re-check file content on git changes (revision signal)
  createEffect(() => {
    const repoPath = props.repoPath;
    if (!repoPath) return;
    const rev = repositoriesStore.getRevision(repoPath);
    // Skip the initial load (rev 0 or first render when savedContent is empty)
    if (rev === 0 || !savedContent()) return;

    (async () => {
      try {
        const diskContent = await fb.readFile(repoPath, props.filePath);
        // File hasn't changed from what we last saved/loaded
        if (diskContent === savedContent()) return;

        if (isDirty()) {
          // Editor has unsaved changes AND file changed on disk — conflict
          setDiskConflict(true);
        } else {
          // Editor is clean — silently reload
          setCode(diskContent);
          setSavedContent(diskContent);
        }
      } catch {
        // File may have been deleted — ignore
      }
    })();
  });

  /** Reload content from disk, discarding local changes */
  const handleReloadFromDisk = async () => {
    try {
      const diskContent = await fb.readFile(props.repoPath, props.filePath);
      setCode(diskContent);
      setSavedContent(diskContent);
      setDiskConflict(false);
    } catch (err) {
      console.error("Failed to reload file:", err);
    }
  };

  /** Keep local changes, dismiss the conflict banner (next save will overwrite disk) */
  const handleKeepLocal = () => {
    setDiskConflict(false);
  };

  const { ref, editorView, createExtension } = createCodeMirror({
    onValueChange: (value) => {
      setCode(value);
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
  createExtension(keymap.of([...defaultKeymap, indentWithTab]));

  // Reactive language extension
  createExtension((): Extension => langSupport() ?? []);

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
        if (code().length > LARGE_FILE_BYTES) {
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
    if (!isDirty() || isReadOnly()) return;
    try {
      await fb.writeFile(props.repoPath, props.filePath, code());
      setSavedContent(code());
    } catch (err) {
      console.error("Failed to save file:", err);
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
        handleSave();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  return (
    <div class="editor-tab-content" data-editor-tab-id={props.id}>
      <div class="editor-header">
        <span class="editor-filename" title={props.filePath}>
          {props.filePath}
        </span>
        <Show when={isDirty()}>
          <span class="editor-dirty-dot" title="Unsaved changes" />
        </Show>
        <button
          class="editor-btn"
          onClick={() => setIsReadOnly((v) => !v)}
          title={isReadOnly() ? "Unlock editing" : "Lock (read-only)"}
        >
          {isReadOnly() ? "\u{1F512}" : "\u{1F513}"}
        </button>
        <Show when={isDirty() && !isReadOnly()}>
          <button class="editor-btn editor-btn-save" onClick={handleSave}>
            Save
          </button>
        </Show>
      </div>

      <Show when={diskConflict()}>
        <div class="editor-conflict-banner">
          <span>File changed on disk.</span>
          <button class="editor-btn" onClick={handleReloadFromDisk}>Reload</button>
          <button class="editor-btn" onClick={handleKeepLocal}>Keep mine</button>
        </div>
      </Show>

      <Show when={loading()}>
        <div class="editor-empty">Loading...</div>
      </Show>

      <Show when={error()}>
        <div class="editor-empty" style={{ color: "var(--error)" }}>
          Error: {error()}
        </div>
      </Show>

      <Show when={!loading() && !error()}>
        <div class="editor-content" ref={ref} />
      </Show>
    </div>
  );
};

export default CodeEditorTab;
