import { createSignal, For, Show } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { retryWrite } from "../utils/retryWrite";
import styles from "./TerminalKeybar.module.css";

interface TerminalKeybarProps {
  sessionId: string;
  agentType?: string | null;
  awaitingInput?: boolean;
  /** True when the question was detected with high confidence (Ink menu footer) */
  questionConfident?: boolean;
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
  { label: "\u21B5", seq: "\r" },
];

/** Agents that use Ink/Bubble Tea menus where Enter=select, Escape=cancel */
const INK_AGENTS = new Set(["claude", "codex", "opencode"]);

/** Resolve confirm keys based on agent type and question confidence.
 *  - Ink-based agents with confident (menu) detection: Enter/Escape
 *  - Text-based agents or low-confidence questions: send y/n + Enter */
function getConfirmKeys(agentType?: string | null, questionConfident?: boolean): KeyDef[] {
  const isInkAgent = agentType ? INK_AGENTS.has(agentType) : false;

  if (isInkAgent && questionConfident) {
    // Ink multiselect: Enter selects highlighted, Escape cancels
    return [
      { label: "Yes", seq: "\r", confirm: true },
      { label: "No", seq: "\x1b", confirm: true },
    ];
  }

  // Text-based prompts (Aider Y/N, generic questions, non-confident detection):
  // send the actual letter + Enter
  return [
    { label: "Yes", seq: "y\r", confirm: true },
    { label: "No", seq: "n\r", confirm: true },
  ];
}

export function TerminalKeybar(props: TerminalKeybarProps) {
  const [sending, setSending] = createSignal(false);

  async function send(seq: string, autoEnter?: boolean) {
    const data = autoEnter ? seq + "\r" : seq;
    const label = seq.length <= 3 ? JSON.stringify(seq) : `${seq.length}b`;
    appLogger.debug("terminal", `TerminalKeybar send: ${label} to ${props.sessionId}`);
    setSending(true);
    try {
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.error("network", `Key send failed after retries: ${msg}`);
    } finally {
      setSending(false);
    }
  }

  function handleSlash() {
    // Escape dismisses any open slash menu (and clears the "/" from the agent's
    // input buffer), then "/" starts a fresh menu. Without Escape, a second press
    // appends "/" → "//" which the agent ignores.
    send("\x1b");
    send("/");
  }

  const confirmKeys = () => getConfirmKeys(props.agentType, props.questionConfident);

  return (
    <div class={styles.bar}>
      <Show when={props.awaitingInput}>
        <For each={confirmKeys()}>{(k) => (
          <button
            class={`${styles.key} ${styles.confirm}`}
            classList={{ [styles.sending]: sending() }}
            disabled={sending()}
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
