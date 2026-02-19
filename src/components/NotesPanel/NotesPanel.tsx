import { Component, For, Show, createSignal } from "solid-js";
import { notesStore } from "../../stores/notes";
import { getModifierSymbol } from "../../platform";

export interface NotesPanelProps {
  visible: boolean;
  onClose: () => void;
  onSendToTerminal: (text: string) => void;
}

/** Format a timestamp as a short relative or absolute string */
function formatDate(timestamp: number): string {
  const now = Date.now();
  const diffMs = now - timestamp;
  const diffMin = Math.floor(diffMs / 60_000);

  if (diffMin < 1) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;

  const diffHours = Math.floor(diffMin / 60);
  if (diffHours < 24) return `${diffHours}h ago`;

  const date = new Date(timestamp);
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export const NotesPanel: Component<NotesPanelProps> = (props) => {
  const [inputText, setInputText] = createSignal("");
  const [editingId, setEditingId] = createSignal<string | null>(null);
  let textareaRef: HTMLTextAreaElement | undefined;

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
    <div id="notes-panel" class={props.visible ? "" : "hidden"}>
      <div class="panel-header">
        <div class="panel-header-left">
          <span class="panel-title">💡 Ideas</span>
          <Show when={notesStore.count() > 0}>
            <span class="file-count-badge">{notesStore.count()}</span>
          </Show>
        </div>
        <button class="panel-close" onClick={props.onClose} title={`Close (${getModifierSymbol()}N)`}>
          &times;
        </button>
      </div>

      <div class="panel-content notes-list">
        <Show when={notesStore.state.notes.length === 0}>
          <div class="notes-empty">No ideas yet. Add one below.</div>
        </Show>
        <For each={notesStore.state.notes}>
          {(note) => (
            <div class="note-item">
              <div class="note-body">
                <span class="note-text" title={note.text}>{note.text}</span>
                <span class="note-date">{formatDate(note.createdAt)}</span>
              </div>
              <div class="note-actions">
                <button
                  class="note-action-btn note-edit-btn"
                  onClick={() => handleEdit(note.id, note.text)}
                  title="Edit note"
                >
                  ✎
                </button>
                <button
                  class="note-action-btn note-send-btn"
                  onClick={() => props.onSendToTerminal(note.text)}
                  title="Send to terminal"
                >
                  ▶
                </button>
                <button
                  class="note-action-btn note-delete-btn"
                  onClick={() => notesStore.removeNote(note.id)}
                  title="Delete note"
                >
                  ✕
                </button>
              </div>
            </div>
          )}
        </For>
      </div>

      <div class="note-input-area">
        <textarea
          ref={textareaRef}
          class="note-input"
          rows={5}
          placeholder="Type an idea and press Enter..."
          value={inputText()}
          onInput={(e) => setInputText(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
        />
        <button class="note-submit-btn" onClick={handleSubmit} disabled={!inputText().trim()} title="Add note (Enter)">
          +
        </button>
      </div>
    </div>
  );
};

export default NotesPanel;
