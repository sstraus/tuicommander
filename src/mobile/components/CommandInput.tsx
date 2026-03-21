import { createSignal, createEffect, onCleanup } from "solid-js";
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
  /** Detected agent type (e.g. "claude-code", "aider"). When set, live sync
   *  to PTY is disabled — Ink-based agents in raw mode don't process Ctrl-U
   *  when bundled with text in the same PTY write. */
  agentType?: string | null;
}

// Debounce delay before syncing textarea content to PTY
const SYNC_DEBOUNCE_MS = 300;

export function CommandInput(props: CommandInputProps) {
  const [value, setValue] = createSignal("");
  let textareaEl: HTMLTextAreaElement | undefined;
  // When true, user is actively editing — don't overwrite with PTY input
  let userEditing = false;
  // Debounce timer for textarea→PTY sync
  let syncTimer: ReturnType<typeof setTimeout> | null = null;

  onCleanup(() => { if (syncTimer) clearTimeout(syncTimer); });

  /** Send the full textarea content to PTY (Ctrl-U + text).
   *  Works in cooked-mode shells (bash/zsh) where Ctrl-U is handled by the
   *  kernel line discipline before the app sees it. Does NOT work in raw-mode
   *  apps (Ink/Claude Code) where Ctrl-U bundled with text in a single write
   *  is not recognized as a control character. */
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
      setValue(pv.text);
      if (textareaEl) {
        textareaEl.value = pv.text;
        textareaEl.focus();
        autoResize();
      }
      // Immediate sync (no debounce) — only for shell sessions where Ctrl-U works
      if (!props.agentType) {
        if (syncTimer) clearTimeout(syncTimer);
        syncToPty(pv.text);
      }
    }
  });

  // PTY → textarea: last writer wins — only update when user is not editing
  createEffect(() => {
    const il = props.ptyInputLine;
    if (userEditing) return;
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

  /** On any input change, debounce a full-text sync to PTY.
   *  NOTE: We keep the signal in sync for send()/handleBlur() but do NOT
   *  bind `value={value()}` on the textarea — on mobile, the reactive
   *  write-back interferes with IME/autocorrect and causes text duplication.
   *
   *  Live sync is disabled for agent sessions — raw-mode apps (Ink/Claude Code)
   *  don't process Ctrl-U when bundled with text in a single PTY write, causing
   *  progressive input duplication instead of line replacement.
   */
  function handleInput(e: InputEvent & { currentTarget: HTMLTextAreaElement }) {
    userEditing = true;
    const text = e.currentTarget.value;
    setValue(text);
    autoResize();
    if (!props.agentType) {
      debouncedSync(text);
    }
  }

  async function send() {
    // Flush any pending debounced sync
    if (syncTimer) clearTimeout(syncTimer);
    // Read directly from the DOM element — on mobile, paste and autocomplete
    // may insert text without firing onInput, so the signal can be stale.
    const text = (textareaEl?.value ?? value()).trim();
    if (!text) return;

    userEditing = false;
    setValue("");
    if (textareaEl) { textareaEl.value = ""; textareaEl.style.height = "auto"; }
    try {
      if (props.agentType) {
        // Ink-based agents in raw mode: split into two writes.
        // Ctrl-U + text in one write, then \r separately — Ink treats \r
        // as newline when combined with text in a single write.
        await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\x15" + text }));
        await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\r" }));
      } else {
        // Shell sessions (cooked mode): single atomic write.
        // Ctrl-U is handled by kernel line discipline before the app sees it.
        await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\x15" + text + "\r" }));
      }
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
    // Flush pending sync immediately on blur (shell sessions only)
    if (syncTimer) {
      clearTimeout(syncTimer);
      if (!props.agentType) {
        syncToPty(value());
      }
    }
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
