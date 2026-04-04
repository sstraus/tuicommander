import { For } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import { sendCommand } from "../../utils/sendCommand";
import styles from "./QuickActions.module.css";

interface SuggestChipsProps {
  sessionId: string;
  items: string[];
  agentType?: string | null;
}

export function SuggestChips(props: SuggestChipsProps) {
  const send = async (text: string) => {
    try {
      await sendCommand(
        (data) => retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data })),
        text,
        props.agentType,
      );
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.warn("network", `Failed to send suggest action: ${msg}`);
    }
  };

  return (
    <div class={styles.row}>
      <For each={props.items}>
        {(item) => (
          <button class={styles.chip} onClick={() => send(item)}>
            {item}
          </button>
        )}
      </For>
    </div>
  );
}
