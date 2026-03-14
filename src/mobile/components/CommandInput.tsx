import { createEffect, onCleanup } from "solid-js";
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

// Debounce delay before syncing textarea content to PTY
const SYNC_DEBOUNCE_MS = 300;

export function CommandInput(props: CommandInputProps) {
  let textareaEl: HTMLTextAreaElement | undefined;
  // When true, user is actively editing — don't overwrite with PTY input
  let userEditing = false;
  // Debounce timer for textarea→PTY sync
  let syncTimer: ReturnType<typeof setTimeout> | null = null;
  // Whether a mobile IME composition is in progress
  let composing = false;

  onCleanup(() => { if (syncTimer) clearTimeout(syncTimer); });

  /** Send the full textarea content to PTY (Ctrl-U + text). */
  function syncToPty(text: string) {
    rpc("write_pty", { sessionId: props.sessionId, data: "\x15" + text }).catch(() => {});
  }

  /** Schedule a debounced sync of textarea content to PTY. */
  function debouncedSync(text: string) {
    if (syncTimer) clearTimeout(syncTimer);
    syncTimer = setTimeout(() => syncToPty(text), SYNC_DEBOUNCE_MS);
  }

  // React to external prefill (e.g. slash menu selection)
  createEffect(() => {
    const pv = props.prefillValue;
    if (pv && pv.text) {
      userEditing = true;
      if (textareaEl) {
        textareaEl.value = pv.text;
        textareaEl.focus();
        autoResize();
      }
      // Immediate sync (no debounce) — user selected from menu
      if (syncTimer) clearTimeout(syncTimer);
      syncToPty(pv.text);
    }
  });

  // PTY → textarea: last writer wins — only update when user is not editing
  createEffect(() => {
    const il = props.ptyInputLine;
    if (userEditing) return;
    const text = il ?? "";
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

  /** On any input change, debounce a full-text sync to PTY. */
  function handleInput(e: InputEvent & { currentTarget: HTMLTextAreaElement }) {
    userEditing = true;
    // During IME composition, just resize — don't sync partial composition to PTY
    if (composing) {
      autoResize();
      return;
    }
    const text = e.currentTarget.value;
    autoResize();
    debouncedSync(text);
  }

  async function send() {
    // Flush any pending debounced sync
    if (syncTimer) clearTimeout(syncTimer);
    // Read directly from the DOM element — on mobile, paste and autocomplete
    // may insert text without firing onInput, so the signal can be stale.
    const text = (textareaEl?.value ?? "").trim();
    if (!text) return;

    userEditing = false;
    if (textareaEl) { textareaEl.value = ""; textareaEl.style.height = "auto"; }
    try {
      // Single atomic write: Ctrl-U clears existing PTY input, then text + Enter.
      // Must be ONE write to prevent in-flight debouncedSync from interleaving.
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\x15" + text + "\r" }));
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
    // Flush pending sync immediately on blur
    if (syncTimer) {
      clearTimeout(syncTimer);
      syncToPty(textareaEl?.value ?? "");
    }
    // Resume PTY sync only if textarea is empty (no draft to preserve)
    if (!(textareaEl?.value ?? "").trim()) {
      userEditing = false;
    }
  }

  return (
    <div class={styles.form}>
      <textarea
        ref={textareaEl}
        class={styles.input}
        placeholder="Type a command..."
        onInput={handleInput}
        onKeyDown={handleKeyDown}
        onFocus={handleFocus}
        onBlur={handleBlur}
        onCompositionStart={() => { composing = true; }}
        onCompositionEnd={(e) => {
          composing = false;
          // Fire a sync now that composition is finalized
          userEditing = true;
          const text = (e.currentTarget as HTMLTextAreaElement).value;
          autoResize();
          debouncedSync(text);
        }}
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
