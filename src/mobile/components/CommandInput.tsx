import { createSignal } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import styles from "./CommandInput.module.css";

interface CommandInputProps {
  sessionId: string;
}

export function CommandInput(props: CommandInputProps) {
  const [value, setValue] = createSignal("");

  async function send() {
    const text = value().trim();
    if (!text) return;

    setValue("");
    try {
      // PTY expects \r (carriage return) for Enter — \n is a line feed
      // and won't trigger command submission in raw-mode programs (Ink, etc.)
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: text + "\r" }));
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
        class={styles.input}
        placeholder="Type a command..."
        value={value()}
        onInput={(e) => setValue(e.currentTarget.value)}
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
