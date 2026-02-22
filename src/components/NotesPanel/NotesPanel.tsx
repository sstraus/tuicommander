import { Component, For, Show, createEffect, createSignal } from "solid-js";
import { notesStore } from "../../stores/notes";
import { getModifierSymbol } from "../../platform";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { formatRelativeTime } from "../../utils/time";
import p from "../shared/panel.module.css";
import s from "./NotesPanel.module.css";

export interface NotesPanelProps {
  visible: boolean;
  onClose: () => void;
  onSendToTerminal: (text: string) => void;
}

export const NotesPanel: Component<NotesPanelProps> = (props) => {
  const [inputText, setInputText] = createSignal("");
  const [editingId, setEditingId] = createSignal<string | null>(null);
  let textareaRef: HTMLTextAreaElement | undefined;
  let contentRef!: HTMLDivElement;

  // Focus the scroll container when panel opens so wheel events route here, not the terminal
  createEffect(() => {
    if (props.visible) contentRef?.focus({ preventScroll: true });
  });

  const handleSubmit = () => {
    const text = inputText();
    if (!text.trim()) return;

    const editing = editingId();
    if (editing) {
      notesStore.removeNote(editing);
      setEditingId(null);
    }
    notesStore.addNote(text);
    setInputText("");
  };

  const handleEdit = (id: string, text: string) => {
    setInputText(text);
    setEditingId(id);
    textareaRef?.focus();
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    <div id="notes-panel" class={cx(s.panel, !props.visible && s.hidden)}>
      <PanelResizeHandle panelId="notes-panel" />
      <div class={p.header}>
        <div class={p.headerLeft}>
          <span class={p.title}><span style={{ filter: "grayscale(1) brightness(1.5)", "font-style": "normal" }}>💡</span> {t("notesPanel.title", "Ideas")}</span>
          <Show when={notesStore.count() > 0}>
            <span class={p.fileCountBadge}>{notesStore.count()}</span>
          </Show>
        </div>
        <button class={p.close} onClick={props.onClose} title={`${t("notesPanel.close", "Close")} (${getModifierSymbol()}N)`}>
          &times;
        </button>
      </div>

      <div ref={contentRef} tabIndex={-1} class={cx(p.content, s.list)}>
        <Show when={notesStore.state.notes.length === 0}>
          <div class={s.empty}>{t("notesPanel.empty", "No ideas yet. Add one below.")}</div>
        </Show>
        <For each={notesStore.state.notes}>
          {(note) => (
            <div class={s.item}>
              <div class={s.body}>
                <span class={s.text} title={note.text}>{note.text}</span>
                <span class={s.date}>{formatRelativeTime(note.createdAt, { showDateFallback: true })}</span>
              </div>
              <div class={s.actions}>
                <button
                  class={cx(s.actionBtn, s.editBtn)}
                  onClick={() => handleEdit(note.id, note.text)}
                  title={t("notesPanel.edit", "Edit note")}
                >
                  ✎
                </button>
                <button
                  class={cx(s.actionBtn, s.sendBtn)}
                  onClick={() => props.onSendToTerminal(note.text)}
                  title={t("notesPanel.send", "Send to terminal")}
                >
                  ▶
                </button>
                <button
                  class={cx(s.actionBtn, s.deleteBtn)}
                  onClick={() => notesStore.removeNote(note.id)}
                  title={t("notesPanel.delete", "Delete note")}
                >
                  ✕
                </button>
              </div>
            </div>
          )}
        </For>
      </div>

      <div class={s.inputArea}>
        <textarea
          ref={textareaRef}
          class={s.input}
          rows={5}
          placeholder={t("notesPanel.placeholder", "Type an idea and press Enter...")}
          value={inputText()}
          onInput={(e) => setInputText(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
        />
        <button class={s.submitBtn} onClick={handleSubmit} disabled={!inputText().trim()} title={t("notesPanel.submit", "Add note (Enter)")}>
          +
        </button>
      </div>
    </div>
  );
};

export default NotesPanel;
