import { createSignal, createEffect } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import styles from "./CommandInput.module.css";

interface CommandInputProps {
  sessionId: string;
  /** When set, prefills the textarea and focuses it. Seq counter ensures re-fire on same text. */
  prefillValue?: { text: string; seq: number };
  /** Current PTY input line text (synced from terminal prompt via WebSocket). */
  ptyInputLine?: string | null;
}

// After syncing to PTY, ignore incoming input_line echoes for this duration
const SYNC_GUARD_MS = 400;

export function CommandInput(props: CommandInputProps) {
  const [value, setValue] = createSignal("");
  let textareaEl: HTMLTextAreaElement | undefined;
  // When true, user is actively editing — don't overwrite with PTY input
  let userEditing = false;
  // Timestamp of last outbound write; used to suppress echo-back
  let lastSyncTs = 0;

  // React to external prefill (e.g. slash menu selection)
  createEffect(() => {
    const pv = props.prefillValue;
    if (pv && pv.text) {
      userEditing = true;
      setValue(pv.text);
      if (textareaEl) {
        textareaEl.value = pv.text;
        textareaEl.focus();
        autoResize();
      }
      // Push to PTY: Ctrl-U clears existing input, then type the full text
      lastSyncTs = Date.now();
      rpc("write_pty", { sessionId: props.sessionId, data: "\x15" + pv.text }).catch(() => {});
    }
  });

  // Sync PTY input line → textarea (only when user is not editing and no recent outbound write)
  createEffect(() => {
    const il = props.ptyInputLine;
    if (userEditing) return;
    if (Date.now() - lastSyncTs < SYNC_GUARD_MS) return;
    const text = il ?? "";
    setValue(text);
    if (textareaEl) {
      textareaEl.value = text;
      autoResize();
    }
  });

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = "auto";
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 120) + "px";
  }

  /** Send incremental edits to PTY based on InputEvent type. */
  function handleInput(e: InputEvent & { currentTarget: HTMLTextAreaElement }) {
    userEditing = true;
    setValue(e.currentTarget.value);
    autoResize();

    lastSyncTs = Date.now();
    const inputType = e.inputType;

    const ta = e.currentTarget;
    const cursorAtEnd = ta.selectionStart === ta.value.length;

    if (inputType === "insertText" && e.data && cursorAtEnd) {
      // Simple append at end — send just the new characters
      rpc("write_pty", { sessionId: props.sessionId, data: e.data }).catch(() => {});
    } else if (inputType === "deleteContentBackward" && cursorAtEnd) {
      // Backspace at end
      rpc("write_pty", { sessionId: props.sessionId, data: "\x7f" }).catch(() => {});
    } else {
      // Any other edit (paste, mid-text insert, cut, word-delete, cursor move+type):
      // resync full textarea content to PTY
      rpc("write_pty", { sessionId: props.sessionId, data: "\x15" + ta.value }).catch(() => {});
    }
  }

  async function send() {
    // Read directly from the DOM element — on mobile, paste and autocomplete
    // may insert text without firing onInput, so the signal can be stale.
    const text = (textareaEl?.value ?? value()).trim();
    if (!text) return;

    userEditing = false;
    lastSyncTs = Date.now();
    setValue("");
    if (textareaEl) { textareaEl.value = ""; textareaEl.style.height = "auto"; }
    try {
      // Ctrl-U clears any existing PTY input, then send text and Enter as
      // separate writes — Ink-based TUIs treat \r as newline when combined.
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\x15" }));
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: text }));
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\r" }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.error("network", `Failed to send command after retries: ${msg}`);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }

  function handleFocus() {
    userEditing = true;
  }

  function handleBlur() {
    // Resume PTY sync only if textarea is empty (no draft to preserve)
    if (!value().trim()) {
      userEditing = false;
    }
  }

  return (
    <div class={styles.form}>
      <textarea
        ref={textareaEl}
        class={styles.input}
        placeholder="Type a command..."
        value={value()}
        onInput={handleInput}
        onKeyDown={handleKeyDown}
        onFocus={handleFocus}
        onBlur={handleBlur}
        autocomplete="off"
        autocorrect="off"
        spellcheck={false}
        autocapitalize="off"
        inputmode="text"
        rows={1}
      />
      <button class={styles.send} type="button" onClick={send}>
        <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
          <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
        </svg>
      </button>
    </div>
  );
}
