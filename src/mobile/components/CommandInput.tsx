import { createSignal } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import styles from "./CommandInput.module.css";

interface CommandInputProps {
  sessionId: string;
}

export function CommandInput(props: CommandInputProps) {
  const [value, setValue] = createSignal("");

  async function handleSubmit(e: Event) {
    e.preventDefault();
    const text = value().trim();
    if (!text) return;

    try {
      await rpc("write_pty", { sessionId: props.sessionId, data: text + "\n" });
      setValue("");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.warn("network", `Failed to send command: ${msg}`);
    }
  }

  return (
    <form class={styles.form} onSubmit={handleSubmit}>
      <input
        class={styles.input}
        type="text"
        placeholder="Type a command..."
        value={value()}
        onInput={(e) => setValue(e.currentTarget.value)}
        autocomplete="off"
        autocorrect="off"
        spellcheck={false}
        autocapitalize="off"
      />
      <button class={styles.send} type="submit">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
          <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
        </svg>
      </button>
    </form>
  );
}
