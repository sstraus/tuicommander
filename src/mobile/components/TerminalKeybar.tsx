import { For, Show } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import styles from "./TerminalKeybar.module.css";

interface TerminalKeybarProps {
  sessionId: string;
  agentType?: string | null;
  awaitingInput?: boolean;
  onCommandWidgetOpen?: () => void;
}

interface KeyDef { label: string; seq: string; danger?: boolean; autoEnter?: boolean; confirm?: boolean }

const STANDARD_KEYS: KeyDef[] = [
  { label: "Ctrl+C", seq: "\x03", danger: true },
  { label: "Ctrl+D", seq: "\x04" },
  { label: "Tab", seq: "\t" },
  { label: "Esc", seq: "\x1b" },
  { label: "\u2191", seq: "\x1b[A" },
  { label: "\u2193", seq: "\x1b[B" },
];

const CONFIRM_KEYS: KeyDef[] = [
  { label: "Yes", seq: "yes", confirm: true, autoEnter: true },
  { label: "No", seq: "no", confirm: true, autoEnter: true },
];

export function TerminalKeybar(props: TerminalKeybarProps) {
  async function send(seq: string, autoEnter?: boolean) {
    const data = autoEnter ? seq + "\r" : seq;
    try {
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.error("network", `Key send failed after retries: ${msg}`);
    }
  }

  function handleSlash() {
    if (props.agentType && props.onCommandWidgetOpen) {
      props.onCommandWidgetOpen();
    } else {
      send("/");
    }
  }

  return (
    <div class={styles.bar}>
      <Show when={props.awaitingInput}>
        <For each={CONFIRM_KEYS}>{(k) => (
          <button
            class={`${styles.key} ${styles.confirm}`}
            onClick={() => send(k.seq, k.autoEnter)}
          >
            {k.label}
          </button>
        )}</For>
        <div class={styles.divider} />
      </Show>
      <button class={`${styles.key} ${styles.accent}`} onClick={handleSlash}>
        /
      </button>
      <For each={STANDARD_KEYS}>{(k) => (
        <button
          class={styles.key}
          classList={{ [styles.danger]: !!k.danger }}
          onClick={() => send(k.seq)}
        >
          {k.label}
        </button>
      )}</For>
    </div>
  );
}
