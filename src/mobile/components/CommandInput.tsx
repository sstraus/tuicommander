import { createSignal, createEffect, Show } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { sendPtyKey } from "../../utils/sendCommand";
import { retryWrite } from "../utils/retryWrite";
import { SlashMenuOverlay } from "./SlashMenuOverlay";
import { ChoicePromptOverlay } from "./ChoicePromptOverlay";
import { isSendGuardActive, classifyEcho } from "./syncGuards";
import type { SlashMenuItem, ChoicePrompt } from "../useSessions";
import styles from "./CommandInput.module.css";

interface CommandInputProps {
  sessionId: string;
  /** When set, prefills the textarea and focuses it. Seq counter ensures re-fire on same text. */
  prefillValue?: { text: string; seq: number };
  /** Current PTY input line text (synced from terminal prompt via WebSocket). */
  ptyInputLine?: string | null;
  /** Detected agent type (e.g. "claude-code", "aider"). */
  agentType?: string | null;
  /** Slash menu items from session state (populated by backend parser). */
  slashItems?: SlashMenuItem[];
  /** Active numbered choice dialog parsed from agent output. */
  choicePrompt?: ChoicePrompt;
  /** Registers the triggerSlash function so parent can invoke it. */
  onRegisterTrigger?: (fn: () => void) => void;
}

export function CommandInput(props: CommandInputProps) {
  const [value, setValue] = createSignal("");
  let textareaEl: HTMLTextAreaElement | undefined;
  let lastSendAt = 0;
  // What we last sent to PTY — used to compute deltas
  let syncedText = "";
  // Number of write_pty RPCs awaiting response. While > 0, incoming
  // ptyInputLine is echo of our own writes — don't update syncedText.
  // When 0, the terminal is driving (autocomplete, history nav, tab
  // completion) and syncedText must accept the PTY value.
  let pendingWrites = 0;
  // Timestamp when pendingWrites last dropped to 0. Used by classifyEcho
  // to reject stale echoes that arrive after RPCs resolved but before the
  // WebSocket echo catches up.
  let lastWriteSettledAt = 0;

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

  // PTY input line sync. Three layers of protection against stale echoes:
  // 1. Send guard (1s post-Enter) — suppresses prompt-clear noise
  // 2. pendingWrites > 0 — our writes are still in-flight, display-only
  // 3. classifyEcho — after writes settle, rejects stale prefixes and
  //    holds unrelated text during a 300ms grace window (superset echoes
  //    like tab completion are accepted immediately)
  createEffect(() => {
    const il = props.ptyInputLine;
    if (isSendGuardActive(Date.now(), lastSendAt)) return;
    const text = il ?? "";
    if (text === syncedText) return; // exact echo — skip entirely

    if (pendingWrites > 0) {
      // Writes in-flight — display only, don't touch syncedText
      setValue(text);
      if (textareaEl) { textareaEl.value = text; autoResize(); }
      return;
    }

    // pendingWrites === 0 — use smart echo classification
    const verdict = classifyEcho(text, syncedText, Date.now(), lastWriteSettledAt);
    if (verdict === "reject") return; // stale prefix — ignore entirely
    if (verdict === "accept") syncedText = text;
    // "display-only" and "accept" both update the display
    setValue(text);
    if (textareaEl) { textareaEl.value = text; autoResize(); }
  });

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = "auto";
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 120) + "px";
  }

  function writePty(data: string) {
    pendingWrites++;
    // Safety: if the rpc promise never settles (transport tear-down races
    // where neither resolve nor reject fires before the 30s fetch timeout),
    // pendingWrites would stay elevated forever and block input sync. A 5s
    // watchdog decrements once even if the rpc is still in flight.
    let decremented = false;
    const dec = () => {
      if (!decremented) {
        decremented = true;
        pendingWrites--;
        if (pendingWrites === 0) lastWriteSettledAt = Date.now();
      }
    };
    const watchdog = window.setTimeout(dec, 5000);
    rpc("write_pty", { sessionId: props.sessionId, data })
      .catch((err: unknown) => {
        appLogger.warn("network", "Failed to write to PTY", { error: err });
      })
      .finally(() => { window.clearTimeout(watchdog); dec(); });
  }

  /** Send character deltas to PTY so the remote input stays in sync. */
  function syncDelta(newText: string) {
    const oldText = syncedText;

    if (newText.startsWith(oldText)) {
      const delta = newText.slice(oldText.length);
      if (delta) writePty(delta);
    } else if (oldText.startsWith(newText)) {
      const count = oldText.length - newText.length;
      writePty("\x7f".repeat(count));
    } else {
      // Complex edit (paste, cut, etc.) — delete old text with backspaces, then type new
      writePty("\x7f".repeat(oldText.length) + newText);
    }

    syncedText = newText;
  }

  function handleInput(e: InputEvent & { currentTarget: HTMLTextAreaElement }) {
    const text = e.currentTarget.value;
    setValue(text);
    autoResize();
    syncDelta(text);
  }

  function handleSlashSelect(command: string) {
    // Dismiss the agent's slash menu then type the selected command
    // Replace current text with selected command using delta sync
    // (backspaces to delete, then type new text — works in any mode)
    const text = command + " ";
    setValue(text);
    syncDelta(text);
    if (textareaEl) {
      textareaEl.value = text;
      textareaEl.focus();
      autoResize();
    }
  }

  /** Externally trigger slash mode (e.g. from TerminalKeybar "/" button). */
  function triggerSlash() {
    syncedText = "";
    setValue("/");
    if (textareaEl) {
      textareaEl.value = "/";
      textareaEl.focus();
      autoResize();
    }
    syncDelta("/");
  }

  // Register triggerSlash with parent via callback prop
  createEffect(() => {
    if (textareaEl) {
      props.onRegisterTrigger?.(triggerSlash);
    }
  });

  async function send() {
    const text = (textareaEl?.value ?? value()).trim();
    if (!text) return;

    lastSendAt = Date.now();
    syncedText = "";
    setValue("");
    if (textareaEl) { textareaEl.value = ""; textareaEl.style.height = "auto"; }
    try {
      // Text is already in the PTY via live delta sync — just press Enter
      await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\r" }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.error("network", `Failed to send command after retries: ${msg}`);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Tab") {
      e.preventDefault();
      writePty("\t");
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
    if (e.key === "Escape") {
      writePty("\x1b");
      syncedText = "";
      setValue("");
      if (textareaEl) { textareaEl.value = ""; }
    }
  }

  const showDropup = () => value().startsWith("/") && (props.slashItems?.length ?? 0) > 0;
  const showChoicePrompt = () => !!props.choicePrompt;

  async function handleChoiceSelect(key: string) {
    try {
      await sendPtyKey(
        (data) => rpc("write_pty", { sessionId: props.sessionId, data }),
        key,
      );
    } catch (err) {
      appLogger.warn("terminal", "ChoicePrompt sendPtyKey failed", err);
    }
  }

  return (
    <div class={styles.form} style={{ position: "relative" }}>
      <Show when={showChoicePrompt()}>
        <ChoicePromptOverlay
          prompt={props.choicePrompt!}
          onSelect={handleChoiceSelect}
        />
      </Show>
      <Show when={showDropup() && !showChoicePrompt()}>
        <SlashMenuOverlay
          items={props.slashItems ?? []}
          onSelect={handleSlashSelect}
        />
      </Show>
      <textarea
        ref={textareaEl}
        class={styles.input}
        placeholder="Type a command..."
        onInput={handleInput}
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
