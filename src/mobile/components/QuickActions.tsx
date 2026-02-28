import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
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
    await rpc("write_pty", { sessionId, data: data + "\n" });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    appLogger.warn("network", `Failed to send: ${msg}`);
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
