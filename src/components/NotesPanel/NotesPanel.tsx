import { Component, For, Show, createSignal } from "solid-js";
import { notesStore } from "../../stores/notes";
import { repositoriesStore } from "../../stores/repositories";
import { getModifierSymbol } from "../../platform";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { formatRelativeTime } from "../../utils/time";
import p from "../shared/panel.module.css";
import s from "./NotesPanel.module.css";

export interface NotesPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
  onSendToTerminal: (text: string) => void;
}

/** Extract last path segment as display name */
function deriveDisplayName(repoPath: string | null): string | null {
  if (!repoPath) return null;
  return repoPath.split("/").filter(Boolean).pop() ?? repoPath;
}

export const NotesPanel: Component<NotesPanelProps> = (props) => {
  const [inputText, setInputText] = createSignal("");
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [reassigningId, setReassigningId] = createSignal<string | null>(null);
  let textareaRef: HTMLTextAreaElement | undefined;

  const filteredNotes = () => notesStore.getFilteredNotes(props.repoPath);
  const badgeCount = () => notesStore.filteredCount(props.repoPath);

  const repoOptions = () => {
    const repos = repositoriesStore.state.repositories;
    return Object.entries(repos).map(([path, repo]) => ({
      path,
      displayName: repo.displayName,
    }));
  };

  const handleSubmit = () => {
    const text = inputText();
    if (!text.trim()) return;

    const editing = editingId();
    if (editing) {
      notesStore.removeNote(editing);
      setEditingId(null);
    }
    notesStore.addNote(text, props.repoPath, deriveDisplayName(props.repoPath));
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

  const handleReassign = (noteId: string, newRepoPath: string) => {
    if (newRepoPath === "__global__") {
      notesStore.reassignNote(noteId, null, null);
    } else {
      const repos = repositoriesStore.state.repositories;
      const displayName = repos[newRepoPath]?.displayName ?? deriveDisplayName(newRepoPath);
      notesStore.reassignNote(noteId, newRepoPath, displayName);
    }
    setReassigningId(null);
  };

  return (
    <div id="notes-panel" class={cx(s.panel, !props.visible && s.hidden)}>
      <PanelResizeHandle panelId="notes-panel" />
      <div class={p.header}>
        <div class={p.headerLeft}>
          <span class={p.title}><span style={{ filter: "grayscale(1) brightness(1.5)", "font-style": "normal" }}>💡</span> {t("notesPanel.title", "Ideas")}</span>
          <Show when={badgeCount() > 0}>
            <span class={p.fileCountBadge}>{badgeCount()}</span>
          </Show>
        </div>
        <button class={p.close} onClick={props.onClose} title={`${t("notesPanel.close", "Close")} (${getModifierSymbol()}N)`}>
          &times;
        </button>
      </div>

      <div class={cx(p.content, s.list)}>
        <Show when={filteredNotes().length === 0}>
          <div class={s.empty}>{t("notesPanel.empty", "No ideas yet. Add one below.")}</div>
        </Show>
        <For each={filteredNotes()}>
          {(note) => (
            <div class={cx(s.item, !!note.usedAt && s.itemUsed)}>
              <div class={s.body}>
                <span class={s.text} title={note.text}>{note.usedAt ? "✓ " : ""}{note.text}</span>
                <div class={s.meta}>
                  <span class={s.date}>{formatRelativeTime(note.createdAt, { showDateFallback: true })}</span>
                  <Show when={reassigningId() === note.id} fallback={
                    <button
                      class={cx(s.projectLabel, note.repoPath ? s.projectTagged : s.projectGlobal)}
                      onClick={() => setReassigningId(note.id)}
                      title="Click to reassign project"
                    >
                      {note.repoDisplayName ?? "Global"}
                    </button>
                  }>
                    <select
                      class={s.reassignSelect}
                      value={note.repoPath ?? "__global__"}
                      onChange={(e) => handleReassign(note.id, e.currentTarget.value)}
                      onBlur={() => setReassigningId(null)}
                      ref={(el) => requestAnimationFrame(() => el.focus())}
                    >
                      <option value="__global__">Global</option>
                      <For each={repoOptions()}>
                        {(repo) => (
                          <option value={repo.path}>{repo.displayName}</option>
                        )}
                      </For>
                    </select>
                  </Show>
                </div>
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
                  onClick={() => { props.onSendToTerminal(note.text); notesStore.markUsed(note.id); }}
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
