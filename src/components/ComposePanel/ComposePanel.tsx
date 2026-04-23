import { Component, createEffect, onCleanup, type Accessor } from "solid-js";
import { createCodeMirror } from "solid-codemirror";
import { EditorView, drawSelection, keymap } from "@codemirror/view";
import { defaultKeymap, historyKeymap, history } from "@codemirror/commands";
import { cx } from "../../utils";
import s from "./ComposePanel.module.css";

const composeTheme = EditorView.theme(
  {
    "&": {
      width: "100%",
      height: "100%",
      fontSize: "13px",
      background: "var(--bg-primary)",
    },
    ".cm-scroller": {
      fontFamily: "var(--font-mono)",
      overflow: "auto",
    },
    ".cm-content": {
      caretColor: "var(--accent)",
      padding: "8px 12px",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--accent)",
    },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
      {
        backgroundColor: "rgba(122, 162, 247, 0.2)",
      },
    ".cm-activeLine": {
      backgroundColor: "transparent",
    },
    "&.cm-focused": {
      outline: "none",
    },
  },
  { dark: true },
);

export interface ComposePanelProps {
  isOpen: Accessor<boolean>;
  initialText: Accessor<string>;
  onClose: () => void;
  onSend: (text: string) => void | Promise<void>;
}

export const ComposePanel: Component<ComposePanelProps> = (props) => {
  const { ref, editorView, createExtension } = createCodeMirror({
    onValueChange: () => {},
  });

  createExtension(composeTheme);
  createExtension(drawSelection());
  createExtension(history());
  createExtension(EditorView.lineWrapping);
  createExtension(
    keymap.of([
      {
        key: "Ctrl-Enter",
        run: (view) => {
          const text = view.state.doc.toString().trim();
          if (text) props.onSend(text);
          return true;
        },
      },
      {
        key: "Escape",
        run: () => {
          props.onClose();
          return true;
        },
      },
      ...defaultKeymap,
      ...historyKeymap,
    ]),
  );

  createEffect(() => {
    if (!props.isOpen()) return;
    const initial = props.initialText();
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        const view = editorView();
        if (view) {
          view.dispatch({
            changes: { from: 0, to: view.state.doc.length, insert: initial },
            selection: { anchor: initial.length },
          });
          view.focus();
        }
      }),
    );
  });

  createEffect(() => {
    if (!props.isOpen()) return;
    const handleFocusOut = (e: FocusEvent) => {
      const related = e.relatedTarget as Node | null;
      const panel = editorView()?.dom?.closest(`.${s.panel}`);
      if (related && panel?.contains(related)) return;
      requestAnimationFrame(() => {
        if (props.isOpen()) editorView()?.focus();
      });
    };
    const cmDom = editorView()?.dom;
    cmDom?.addEventListener("focusout", handleFocusOut);
    onCleanup(() => cmDom?.removeEventListener("focusout", handleFocusOut));
  });

  const handleSend = () => {
    const view = editorView();
    if (!view) return;
    const text = view.state.doc.toString().trim();
    if (text) props.onSend(text);
  };

  return (
    <div
      class={cx(s.panel, props.isOpen() && s.panelOpen)}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <div class={s.editor} ref={ref} />
      <div class={s.statusBar}>
        <span>Ctrl+Enter to send &middot; Esc to close</span>
        <button
          class={s.sendButton}
          onClick={handleSend}
          title="Send (Ctrl+Enter)"
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M4 2l10 6-10 6V2z" />
          </svg>
        </button>
      </div>
    </div>
  );
};
