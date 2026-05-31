import { createEffect, createSignal, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { rpc } from "../../transport";
import { sendPtyKey } from "../../utils/sendCommand";
import type { ChoicePrompt, SlashMenuItem } from "../useSessions";
import { retryWrite } from "../utils/retryWrite";
import { ChoicePromptOverlay } from "./ChoicePromptOverlay";
import styles from "./CommandInput.module.css";
import { SlashMenuOverlay } from "./SlashMenuOverlay";
import { computeInputDelta, isPostSendGuardActive, isSupersetEcho } from "./syncGuards";

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
		if (pv?.text) {
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
		if (textareaEl) {
			textareaEl.value = text;
			autoResize();
		}
	});

	function autoResize() {
		if (!textareaEl) return;
		textareaEl.style.height = "auto";
		textareaEl.style.height = Math.min(textareaEl.scrollHeight, 120) + "px";
	}

	function writePty(data: string) {
		rpc("write_pty", { sessionId: props.sessionId, data }).catch((err: unknown) => {
			appLogger.warn("network", "Failed to write to PTY", { error: err });
		});
	}

	/** Send character deltas to PTY so the remote input stays in sync.
	 *  Uses a minimal end-anchored delta (computeInputDelta) — a mid-line edit
	 *  backspaces only the divergent tail instead of nuking and retyping the
	 *  whole line, which previously caused a keystroke storm and visible mess. */
	function syncDelta(newText: string) {
		const delta = computeInputDelta(syncedText, newText);
		if (delta) writePty(delta);
		syncedText = newText;
	}

	function handleInput(e: InputEvent & { currentTarget: HTMLTextAreaElement }) {
		const text = e.currentTarget.value;
		setValue(text);
		autoResize();
		syncDelta(text);
	}

	function handleSlashSelect(command: string) {
		// Preserve any prefix typed before the slash (e.g. "ciao /he" → "ciao /help ").
		// Without this, a mid-line slash selection would wipe the prefix in the PWA
		// while the agent's buffer still holds it — the two sides diverge.
		const current = textareaEl?.value ?? value();
		const slashIdx = current.lastIndexOf("/");
		const prefix = slashIdx >= 0 ? current.slice(0, slashIdx) : "";
		const text = prefix + command + " ";
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
		if (textareaEl) {
			textareaEl.value = "";
			textareaEl.style.height = "auto";
		}
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
			if (textareaEl) {
				textareaEl.value = "";
			}
		}
	}

	const showDropup = () => value().includes("/") && (props.slashItems?.length ?? 0) > 0;
	const showChoicePrompt = () => !!props.choicePrompt;

	async function handleChoiceSelect(key: string) {
		try {
			const write = (data: string) => rpc<void>("write_pty", { sessionId: props.sessionId, data });
			await sendPtyKey(write, key);
			// Raw-mode prompts (edit-confirm, bash-confirm) have a footer with
			// dismiss_key — a single key press suffices. Line-mode prompts (LSP
			// install, etc.) lack the footer and need Enter to submit.
			if (!props.choicePrompt?.dismiss_key) {
				await write("\r");
			}
		} catch (err) {
			appLogger.warn("terminal", "ChoicePrompt sendPtyKey failed", { error: err });
		}
	}

	return (
		<div class={styles.form} style={{ position: "relative" }}>
			<Show when={showChoicePrompt()}>
				<ChoicePromptOverlay prompt={props.choicePrompt!} onSelect={handleChoiceSelect} />
			</Show>
			<Show when={showDropup() && !showChoicePrompt()}>
				<SlashMenuOverlay items={props.slashItems ?? []} sessionId={props.sessionId} onSelect={handleSlashSelect} />
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
