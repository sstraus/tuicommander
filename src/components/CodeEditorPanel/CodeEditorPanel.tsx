import { Component, Show, createEffect, createSignal, on, onCleanup } from "solid-js";
import { createCodeMirror, createEditorControlledValue, createEditorReadonly } from "solid-codemirror";
import { lineNumbers, drawSelection, highlightActiveLine, highlightActiveLineGutter, keymap } from "@codemirror/view";
import { defaultKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import type { LanguageSupport } from "@codemirror/language";
import { codeEditorStore } from "../../stores/codeEditor";
import { codeEditorTheme } from "./theme";
import { detectLanguage } from "./languageDetection";

export interface CodeEditorPanelProps {
  visible: boolean;
  onClose: () => void;
  onSave?: () => void;
}

/** Large file threshold — skip syntax highlighting above this size */
const LARGE_FILE_BYTES = 500 * 1024;

export const CodeEditorPanel: Component<CodeEditorPanelProps> = (props) => {
  const [langSupport, setLangSupport] = createSignal<LanguageSupport | null>(null);
  const [code, setCode] = createSignal("");

  // Sync store content → local signal
  createEffect(() => {
    if (codeEditorStore.state.isLoading) return;
    setCode(codeEditorStore.state.content);
  });

  const { ref, editorView, createExtension } = createCodeMirror({
    onValueChange: (value) => {
      setCode(value);
      codeEditorStore.setContent(value);
    },
  });

  // Controlled value — sync external changes into editor
  createEditorControlledValue(editorView, code);

  // Read-only mode
  createEditorReadonly(editorView, () => codeEditorStore.state.isReadOnly);

  // Base extensions
  createExtension(codeEditorTheme);
  createExtension(lineNumbers());
  createExtension(drawSelection());
  createExtension(highlightActiveLine());
  createExtension(highlightActiveLineGutter());
  createExtension(bracketMatching());
  createExtension(indentOnInput());
  createExtension(keymap.of([...defaultKeymap, indentWithTab]));

  // Reactive language extension — swaps when file changes
  createExtension((): Extension => {
    const lang = langSupport();
    return lang ?? [];
  });

  // Load language support when file changes
  createEffect(
    on(
      () => codeEditorStore.state.filePath,
      async (filePath) => {
        if (!filePath) {
          setLangSupport(null);
          return;
        }

        // Skip syntax highlighting for large files
        const contentLen = codeEditorStore.state.content.length;
        if (contentLen > LARGE_FILE_BYTES) {
          setLangSupport(null);
          return;
        }

        const lang = await detectLanguage(filePath);
        setLangSupport(lang);
      },
    ),
  );

  // Cmd+S save shortcut when editor is focused
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      const isMeta = e.metaKey || e.ctrlKey;
      if (isMeta && e.key === "s") {
        // Only handle if the editor panel contains focus
        const panel = document.getElementById("code-editor-panel");
        if (!panel?.contains(document.activeElement)) return;

        e.preventDefault();
        if (codeEditorStore.state.isDirty && !codeEditorStore.state.isReadOnly) {
          props.onSave?.();
        }
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  // Extract just the filename from the path for display
  const displayName = () => {
    const fp = codeEditorStore.state.filePath;
    if (!fp) return null;
    return fp;
  };

  return (
    <div id="code-editor-panel" class={props.visible ? "" : "hidden"}>
      <Show when={codeEditorStore.state.filePath} fallback={
        <div class="editor-empty">No file open</div>
      }>
        <div class="editor-header">
          <span class="editor-filename" title={codeEditorStore.state.filePath || ""}>
            {displayName()}
          </span>
          <Show when={codeEditorStore.state.isDirty}>
            <span class="editor-dirty-dot" title="Unsaved changes" />
          </Show>
          <button
            class="editor-btn"
            classList={{ "editor-btn-active": codeEditorStore.state.isReadOnly }}
            onClick={() => codeEditorStore.setReadOnly(!codeEditorStore.state.isReadOnly)}
            title={codeEditorStore.state.isReadOnly ? "Unlock editing" : "Lock (read-only)"}
          >
            {codeEditorStore.state.isReadOnly ? "\u{1F512}" : "\u{1F513}"}
          </button>
          <Show when={codeEditorStore.state.isDirty && !codeEditorStore.state.isReadOnly}>
            <button class="editor-btn editor-btn-save" onClick={() => props.onSave?.()}>
              Save
            </button>
          </Show>
          <button class="panel-close" onClick={props.onClose}>
            &times;
          </button>
        </div>

        <Show when={codeEditorStore.state.isLoading}>
          <div class="editor-empty">Loading...</div>
        </Show>

        <Show when={codeEditorStore.state.error}>
          <div class="editor-empty" style={{ color: "var(--error)" }}>
            Error: {codeEditorStore.state.error}
          </div>
        </Show>

        <Show when={!codeEditorStore.state.isLoading && !codeEditorStore.state.error}>
          <div class="editor-content" ref={ref} />
        </Show>
      </Show>
    </div>
  );
};

export default CodeEditorPanel;
