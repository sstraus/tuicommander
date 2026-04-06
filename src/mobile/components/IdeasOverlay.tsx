import { For, Show, createSignal } from "solid-js";
import { notesStore } from "../../stores/notes";
import { rpc } from "../../transport";
import { retryWrite } from "../utils/retryWrite";
import { appLogger } from "../../stores/appLogger";
import { formatRelativeTime } from "../../utils/time";
import styles from "./IdeasOverlay.module.css";

interface IdeasOverlayProps {
  sessionId: string;
  repoPath: string | null;
  onDismiss: () => void;
}

export function IdeasOverlay(props: IdeasOverlayProps) {
  const [inputText, setInputText] = createSignal("");

  const notes = () => notesStore.getFilteredNotes(props.repoPath);

  function handleBackdrop(e: MouseEvent) {
    if (e.target === e.currentTarget) props.onDismiss();
  }

  function handleSubmit() {
    const text = inputText().trim();
    if (!text) return;
    notesStore.addNote(text, props.repoPath, deriveDisplayName(props.repoPath));
    setInputText("");
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
    if (e.key === "Escape") {
      props.onDismiss();
    }
  }

  async function handleSend(note: { id: string; text: string }) {
    props.onDismiss();
    notesStore.markUsed(note.id);
    try {
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\x15" + note.text }));
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\r" }));
    } catch (err) {
      appLogger.error("network", `Ideas send failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  return (
    <div class={styles.backdrop} onClick={handleBackdrop}>
      <div class={styles.sheet}>
        <div class={styles.header}>
          <span class={styles.title}>
            Ideas
            <Show when={notes().length > 0}>
              <span class={styles.badge}>{notes().length}</span>
            </Show>
          </span>
          <button class={styles.closeBtn} onClick={props.onDismiss}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        <div class={styles.list}>
          <Show when={notes().length === 0}>
            <div class={styles.empty}>No ideas yet. Add one below.</div>
          </Show>
          <For each={notes()}>
            {(note) => (
              <div class={styles.item} classList={{ [styles.itemUsed]: !!note.usedAt }}>
                <div class={styles.itemBody}>
                  <span class={styles.itemText}>{note.usedAt ? "\u2713 " : ""}{note.text}</span>
                  <div class={styles.itemMeta}>
                    <span class={styles.itemDate}>{formatRelativeTime(note.createdAt, { showDateFallback: true })}</span>
                    <Show when={note.repoDisplayName}>
                      <span class={styles.itemProject}>{note.repoDisplayName}</span>
                    </Show>
                  </div>
                </div>
                <div class={styles.itemActions}>
                  <button
                    class={`${styles.actionBtn} ${styles.sendBtn}`}
                    onClick={() => handleSend(note)}
                    title="Send to terminal"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <line x1="22" y1="2" x2="11" y2="13" />
                      <polygon points="22 2 15 22 11 13 2 9 22 2" />
                    </svg>
                  </button>
                  <button
                    class={`${styles.actionBtn} ${styles.deleteBtn}`}
                    onClick={() => notesStore.removeNote(note.id)}
                    title="Delete"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <line x1="18" y1="6" x2="6" y2="18" />
                      <line x1="6" y1="6" x2="18" y2="18" />
                    </svg>
                  </button>
                </div>
              </div>
            )}
          </For>
        </div>

        <div class={styles.inputArea}>
          <textarea
            class={styles.input}
            rows={1}
            placeholder="Type an idea..."
            value={inputText()}
            onInput={(e) => setInputText(e.currentTarget.value)}
            onKeyDown={handleKeyDown}
          />
          <button
            class={styles.submitBtn}
            onClick={handleSubmit}
            disabled={!inputText().trim()}
          >
            +
          </button>
        </div>
      </div>
    </div>
  );
}

function deriveDisplayName(repoPath: string | null): string | null {
  if (!repoPath) return null;
  return repoPath.split("/").filter(Boolean).pop() ?? repoPath;
}
