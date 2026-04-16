import { createSignal, createEffect, Show } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { sendPtyKey } from "../../utils/sendCommand";
import { retryWrite } from "../utils/retryWrite";
import { SlashMenuOverlay } from "./SlashMenuOverlay";
import { ChoicePromptOverlay } from "./ChoicePromptOverlay";
import { isPostSendGuardActive, isSupersetEcho } from "./syncGuards";
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
  // What we last sent to PTY — used to compute deltas and to gate which
  // PTY echoes we accept (only strict extensions — see sync effect below).
  let syncedText = "";
  // Timestamp of the last Enter (send()). Within POST_SEND_GUARD_MS, all
  // incoming ptyInputLine updates are ignored to prevent a lagging echo of
  // the just-sent command from flashing back into the cleared textarea
  // before the shell advances the prompt.
  let lastSendAt = 0;

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

  // PTY → textarea sync. The PWA textarea is the source of truth for user
  // input; we stream deltas to the PTY via syncDelta(). Two gates:
  //   1. Post-send guard — within POST_SEND_GUARD_MS of Enter, ignore every
  //      PTY echo (suppresses the ghost flash of the just-sent command).
  //   2. Strict-extension rule — outside the guard, accept a PTY update
  //      only if it extends syncedText (tab completion / autocomplete).
  // Everything else (prompt redraws, lagging echoes over slow links,
  // history-nav replacements) is ignored so the textarea can't be clobbered.
  createEffect(() => {
    const text = props.ptyInputLine ?? "";
    if (isPostSendGuardActive(Date.now(), lastSendAt)) return;
    if (!isSupersetEcho(text, syncedText)) return;
    syncedText = text;
    setValue(text);
    if (textareaEl) { textareaEl.value = text; autoResize(); }
  });

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = "auto";
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 120) + "px";
  }

  function writePty(data: string) {
    rpc("write_pty", { sessionId: props.sessionId, data })
      .catch((err: unknown) => {
        appLogger.warn("network", "Failed to write to PTY", { error: err });
      });
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
