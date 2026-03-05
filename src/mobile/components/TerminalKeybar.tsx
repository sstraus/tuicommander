import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import styles from "./TerminalKeybar.module.css";

interface TerminalKeybarProps {
  sessionId: string;
  onCommandWidgetOpen?: () => void;
}

interface KeyDef { label: string; seq: string; danger?: boolean }

const KEYS: KeyDef[] = [
  { label: "Ctrl+C", seq: "\x03", danger: true },
  { label: "Ctrl+D", seq: "\x04" },
  { label: "Tab", seq: "\t" },
  { label: "Esc", seq: "\x1b" },
  { label: "\u2191", seq: "\x1b[A" },
  { label: "\u2193", seq: "\x1b[B" },
];

export function TerminalKeybar(props: TerminalKeybarProps) {
  async function send(seq: string) {
    try {
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: seq }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.error("network", `Key send failed after retries: ${msg}`);
    }
  }

  return (
    <div class={styles.bar}>
      {props.onCommandWidgetOpen && (
        <button class={`${styles.key} ${styles.accent}`} onClick={props.onCommandWidgetOpen}>
          /
        </button>
      )}
      {KEYS.map((k) => (
        <button
          class={styles.key}
          classList={{ [styles.danger]: !!k.danger }}
          onClick={() => send(k.seq)}
        >
          {k.label}
        </button>
      ))}
    </div>
  );
}
