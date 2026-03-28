import { createSignal, createEffect } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import { sendCommand } from "../../utils/sendCommand";
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

export function CommandInput(props: CommandInputProps) {
  const [value, setValue] = createSignal("");
  let textareaEl: HTMLTextAreaElement | undefined;
  // When true, user is actively editing — don't overwrite with PTY input
  let userEditing = false;

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
      // Mobile: no live sync to PTY — input is sent only on explicit send().
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

  /** On any input change, update local state only — no PTY sync.
   *  This is the mobile CommandInput: live sync causes echo duplication
   *  because the PTY echoes the text back and then send() writes it again.
   *  Input is only written to PTY on explicit send (Enter/Send button).
   */
  function handleInput(e: InputEvent & { currentTarget: HTMLTextAreaElement }) {
    userEditing = true;
    const text = e.currentTarget.value;
    setValue(text);
    autoResize();
  }

  async function send() {
    // Read directly from the DOM element — on mobile, paste and autocomplete
    // may insert text without firing onInput, so the signal can be stale.
    const text = (textareaEl?.value ?? value()).trim();
    if (!text) return;

    userEditing = false;
    setValue("");
    if (textareaEl) { textareaEl.value = ""; textareaEl.style.height = "auto"; }
    try {
      await sendCommand(
        (data) => retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data })),
        text,
        props.agentType,
      );
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
    // Resume PTY→textarea sync only if textarea is empty (no draft to preserve)
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
