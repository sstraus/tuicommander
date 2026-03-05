import { createSignal, createEffect } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import styles from "./CommandInput.module.css";

interface CommandInputProps {
  sessionId: string;
  /** When set, prefills the textarea and focuses it. Seq counter ensures re-fire on same text. */
  prefillValue?: { text: string; seq: number };
}

export function CommandInput(props: CommandInputProps) {
  const [value, setValue] = createSignal("");
  let textareaEl: HTMLTextAreaElement | undefined;

  // React to external prefill (e.g. slash menu selection)
  createEffect(() => {
    const pv = props.prefillValue;
    if (pv && pv.text) {
      setValue(pv.text);
      if (textareaEl) {
        textareaEl.value = pv.text;
        textareaEl.focus();
        autoResize();
      }
    }
  });

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = "auto";
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 120) + "px";
  }

  async function send() {
    // Read directly from the DOM element — on mobile, paste and autocomplete
    // may insert text without firing onInput, so the signal can be stale.
    const text = (textareaEl?.value ?? value()).trim();
    if (!text) return;

    setValue("");
    if (textareaEl) { textareaEl.value = ""; textareaEl.style.height = "auto"; }
    try {
      // Ctrl-U clears any existing PTY input (e.g. "/" typed before slash menu),
      // then send text and Enter as separate writes — when combined in a single
      // write, Ink-based TUIs (Claude Code) treat \r as newline in the
      // multiline input instead of as submit.
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
    // Shift+Enter: default textarea behavior (insert newline)
  }

  return (
    <div class={styles.form}>
      <textarea
        ref={textareaEl}
        class={styles.input}
        placeholder="Type a command..."
        value={value()}
        onInput={(e) => {
          setValue(e.currentTarget.value);
          autoResize();
        }}
        onKeyDown={handleKeyDown}
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
