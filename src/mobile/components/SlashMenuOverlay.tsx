import { For } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import type { SlashMenuItem } from "../useSessions";
import styles from "./SlashMenuOverlay.module.css";

interface SlashMenuOverlayProps {
  sessionId: string;
  items: SlashMenuItem[];
  onDismiss: () => void;
}

export function SlashMenuOverlay(props: SlashMenuOverlayProps) {
  const select = async (command: string) => {
    props.onDismiss();
    try {
      // Clear the current input (Ctrl-U) then send the slash command + CR
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\x15" + command + "\r" }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.error("network", `Failed to send slash command after retries: ${msg}`);
    }
  };

  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) {
      props.onDismiss();
    }
  };

  return (
    <div class={styles.backdrop} onClick={handleBackdropClick}>
      <div class={styles.sheet}>
        <For each={props.items}>
          {(item) => (
            <button
              class={styles.item}
              classList={{ [styles.itemHighlighted]: item.highlighted }}
              onClick={() => select(item.command)}
            >
              <span class={styles.command}>{item.command}</span>
              <span class={styles.description}>{item.description}</span>
            </button>
          )}
        </For>
      </div>
    </div>
  );
}
