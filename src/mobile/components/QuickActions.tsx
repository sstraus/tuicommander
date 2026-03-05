import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import styles from "./QuickActions.module.css";

interface QuickActionsProps {
  sessionId: string;
}

const ACTIONS = [
  { label: "Yes", value: "yes" },
  { label: "No", value: "no" },
  { label: "y", value: "y" },
  { label: "n", value: "n" },
  { label: "Enter", value: "" },
  { label: "Ctrl-C", value: "\x03" },
] as const;

async function send(sessionId: string, data: string) {
  try {
    await retryWrite(() => rpc("write_pty", { sessionId, data: data + "\r" }));
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    appLogger.error("network", `Failed to send after retries: ${msg}`);
  }
}

export function QuickActions(props: QuickActionsProps) {
  return (
    <div class={styles.row}>
      {ACTIONS.map((action) => (
        <button
          class={styles.chip}
          onClick={() => send(props.sessionId, action.value)}
        >
          {action.label}
        </button>
      ))}
    </div>
  );
}
