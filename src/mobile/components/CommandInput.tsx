import { createEffect, createSignal, Show } from "solid-js";
import { uploadAttachment } from "../../services/uploadAttachment";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { HttpRpcError, rpc } from "../../transport";
import { sendPtyKey, waitForAgentEnterGap } from "../../utils/sendCommand";
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
	/** The Codex question panel has been opened with Alt+Up in this view. */
	codexQuestionOpen?: boolean;
	/** Managed agent waiting for a free-text answer. */
	managedSession?: boolean;
	awaitingInput?: boolean;
	sessionExists?: boolean;
	/** Registers character insertion through the same path as composer typing. */
	onRegisterInsertText?: (fn: (text: string) => void) => void;
	/** Routes keybar Tab through the same completion request as keyboard Tab. */
	onRegisterTab?: (fn: () => void) => void;
}

export function CommandInput(props: CommandInputProps) {
	const [value, setValue] = createSignal("");
	const [submitting, setSubmitting] = createSignal(false);
	const [uploading, setUploading] = createSignal<string | null>(null);
	const [choiceSending, setChoiceSending] = createSignal(false);
	const [codexNotesMode, setCodexNotesMode] = createSignal(false);
	const atomicReply = () =>
		props.managedSession && props.awaitingInput && !props.choicePrompt && !props.codexQuestionOpen && !codexNotesMode();
	createEffect(() => {
		if (!props.choicePrompt) setChoiceSending(false);
	});
	let textareaEl: HTMLTextAreaElement | undefined;
	let fileInput: HTMLInputElement | undefined;
	// What we last sent to PTY — used to compute deltas and to gate which
	// PTY echoes we accept (only strict extensions — see sync effect below).
	let syncedText = "";
	let completionRequested = false;
	// Timestamp of the last Enter (send()). Within POST_SEND_GUARD_MS, all
	// incoming ptyInputLine updates are ignored to prevent a lagging echo of
	// the just-sent command from flashing back into the cleared textarea
	// before the shell advances the prompt.
	let lastSendAt = 0;
	let lastInputWrite: Promise<unknown> = Promise.resolve();

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
	//      only after Tab, if it extends a nonempty syncedText.
	// Everything else (prompt redraws, lagging echoes over slow links,
	// history-nav replacements, automated voice pastes) is ignored so the
	// textarea can't be clobbered or resurrect a command already sent elsewhere.
	createEffect(() => {
		if (atomicReply()) return;
		const text = props.ptyInputLine ?? "";
		if (!completionRequested) return;
		if (isPostSendGuardActive(Date.now(), lastSendAt)) return;
		if (!isSupersetEcho(text, syncedText)) return;
		completionRequested = false;
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
		lastInputWrite = rpc("write_pty", { sessionId: props.sessionId, data }).catch((err: unknown) => {
			appLogger.warn("network", "Failed to write to PTY", { error: err });
		});
	}

	/** Send character deltas to PTY so the remote input stays in sync.
	 *  Uses a minimal end-anchored delta (computeInputDelta) — a mid-line edit
	 *  backspaces only the divergent tail instead of nuking and retyping the
	 *  whole line, which previously caused a keystroke storm and visible mess. */
	function syncDelta(newText: string) {
		completionRequested = false;
		if (atomicReply() || props.sessionExists === false) return;
		const delta = computeInputDelta(syncedText, newText);
		if (delta) writePty(delta);
		syncedText = newText;
	}

	function updateInput(text: string) {
		setValue(text);
		autoResize();
		syncDelta(text);
	}

	function handleInput(e: InputEvent & { currentTarget: HTMLTextAreaElement }) {
		updateInput(e.currentTarget.value);
	}

	function insertText(text: string) {
		if (props.sessionExists === false || !textareaEl) return;
		textareaEl.setRangeText(text, textareaEl.selectionStart, textareaEl.selectionEnd, "end");
		updateInput(textareaEl.value);
		textareaEl.focus();
	}

	async function attachFile(file: File): Promise<void> {
		setUploading(file.name);
		try {
			const receipt = await uploadAttachment(file, { kind: "pty", id: props.sessionId });
			const current = textareaEl?.value ?? value();
			const next = `${current}${current && !/\s$/.test(current) ? " " : ""}@${receipt.path}`;
			setValue(next);
			syncDelta(next);
			if (textareaEl) {
				textareaEl.value = next;
				textareaEl.focus();
				autoResize();
			}
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err);
			toastsStore.add("Attachment failed", message, "error", true);
		} finally {
			setUploading(null);
			if (fileInput) fileInput.value = "";
		}
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

	function closeSlashMenu() {
		setValue("");
		if (textareaEl) textareaEl.value = "";
		syncDelta("");
	}

	createEffect(() => {
		if (textareaEl) props.onRegisterInsertText?.(insertText);
		props.onRegisterTab?.(requestCompletion);
	});

	function requestCompletion() {
		if (atomicReply() || props.sessionExists === false) return;
		completionRequested = syncedText.length > 0;
		writePty("\t");
	}

	async function send() {
		const text = (textareaEl?.value ?? value()).trim();
		if (!text || props.sessionExists === false || submitting()) return;

		if (atomicReply()) {
			setSubmitting(true);
			try {
				const receipt = await rpc<{
					status: string;
					submitted: boolean;
					acknowledged: boolean;
					reason?: string;
				}>("submit_agent_reply", { sessionId: props.sessionId, input: text });
				if (!receipt.submitted) {
					toastsStore.add("Reply not sent", receipt.reason ?? "The session is not ready for a reply", "error", true);
					return;
				}
				if (!receipt.acknowledged) {
					toastsStore.add(
						"Reply sent",
						"The agent did not acknowledge it yet. Check the session before retrying.",
						"warn",
						true,
					);
				}
				setValue("");
				if (textareaEl) textareaEl.value = "";
			} catch (err) {
				let msg = err instanceof Error ? err.message : String(err);
				if (err instanceof HttpRpcError) {
					try {
						const result: unknown = JSON.parse(err.body);
						const reason =
							typeof result === "object" && result !== null && "reason" in result ? result.reason : undefined;
						if (reason === "session_not_found") msg = "This session has ended";
						else if (reason === "agent_not_ready") msg = "The agent is busy. Check the session before retrying.";
						else if (reason === "partial_composer")
							msg = "The agent already has a draft. Check the session before retrying.";
						else if (err.status === 409) msg = "The session is not ready. Check it before retrying.";
					} catch {
						/* Keep the server error for a malformed response. */
					}
				}
				toastsStore.add("Reply not sent", msg, "error", true);
			} finally {
				setSubmitting(false);
			}
			return;
		}

		lastSendAt = Date.now();
		completionRequested = false;
		syncedText = "";
		setValue("");
		if (textareaEl) {
			textareaEl.value = "";
			textareaEl.style.height = "auto";
		}
		try {
			// Let the final live delta reach the PTY, then leave the agent's
			// paste-burst window before pressing Enter.
			await lastInputWrite;
			await waitForAgentEnterGap(props.agentType);
			await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data: "\r" }));
			setCodexNotesMode(false);
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.error("network", `Failed to send command after retries: ${msg}`);
			toastsStore.add("Send failed", `Could not send command: ${msg}`, "error", true);
		}
	}

	function handleKeyDown(e: KeyboardEvent) {
		if (props.sessionExists === false) {
			e.preventDefault();
			return;
		}
		if (atomicReply() && e.key === "Tab") return;
		if (atomicReply() && e.key === "Escape") {
			e.preventDefault();
			setValue("");
			if (textareaEl) textareaEl.value = "";
			return;
		}
		if (e.key === "Tab") {
			e.preventDefault();
			requestCompletion();
			return;
		}
		if (e.key === "Enter" && !e.shiftKey) {
			e.preventDefault();
			send();
		}
		if (e.key === "Escape") {
			completionRequested = false;
			writePty("\x1b");
			syncedText = "";
			setValue("");
			if (textareaEl) {
				textareaEl.value = "";
			}
		}
	}

	const slashItems = () => props.slashItems ?? [];
	const showDropup = () => props.sessionExists !== false && value().includes("/") && slashItems().length > 0;
	const showChoicePrompt = () => !!props.choicePrompt && !codexNotesMode();

	async function handleChoiceSelect(key: string) {
		if (choiceSending() || props.sessionExists === false) return;
		setChoiceSending(true);
		try {
			const write = (data: string) => rpc<void>("write_pty", { sessionId: props.sessionId, data });
			const options = props.choicePrompt?.options ?? [];
			const selectedIndex = options.findIndex((option) => option.highlighted);
			const choiceIndex = options.findIndex((option) => option.key === key);
			const label = options[choiceIndex]?.label;
			if (props.choicePrompt?.selection_mode === "navigate-enter") {
				const steps = choiceIndex - Math.max(selectedIndex, 0);
				const arrow = steps < 0 ? "\x1b[A" : "\x1b[B";
				for (let i = 0; i < Math.abs(steps); i++) await sendPtyKey(write, arrow);
				await sendPtyKey(write, "\r");
				return;
			}
			if (props.agentType === "codex" && (label === "Other" || label === "None of the above")) {
				const steps = (choiceIndex - Math.max(selectedIndex, 0) + options.length) % options.length;
				for (let i = 0; i < steps; i++) await sendPtyKey(write, "\x1b[B");
				await sendPtyKey(write, "\t");
				setCodexNotesMode(true);
				textareaEl?.focus();
				return;
			}
			await sendPtyKey(write, key);
			// Most raw-mode prompts submit on the numeric key.
			if (!props.choicePrompt?.dismiss_key) {
				await write("\r");
			}
		} catch (err) {
			setChoiceSending(false);
			appLogger.warn("terminal", "ChoicePrompt sendPtyKey failed", { error: err });
			const msg = err instanceof Error ? err.message : String(err);
			toastsStore.add("Send failed", `Could not send choice: ${msg}`, "error", true);
		}
	}

	return (
		<div class={styles.form} style={{ position: "relative" }}>
			<Show when={uploading()}>
				{(name) => (
					<div class={styles.uploading} role="status">
						Uploading {name()}…
					</div>
				)}
			</Show>
			<Show when={showChoicePrompt()}>
				<ChoicePromptOverlay prompt={props.choicePrompt!} onSelect={handleChoiceSelect} />
			</Show>
			<Show when={showDropup() && !showChoicePrompt()}>
				<SlashMenuOverlay
					items={slashItems()}
					sessionId={props.sessionId}
					onSelect={handleSlashSelect}
					onClose={closeSlashMenu}
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
				disabled={props.sessionExists === false}
			/>
			<input
				ref={fileInput}
				type="file"
				accept="image/*,application/pdf,text/*"
				aria-label="Choose attachment"
				hidden
				onChange={(event) => {
					const file = event.currentTarget.files?.[0];
					if (file) void attachFile(file);
				}}
			/>
			<button
				class={styles.attach}
				type="button"
				aria-label="Attach file"
				disabled={!!uploading() || props.sessionExists === false || atomicReply()}
				onClick={() => fileInput?.click()}
			>
				<svg
					width="18"
					height="18"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					aria-hidden="true"
				>
					<path d="M20 11.5l-8.6 8.6a5 5 0 0 1-7.1-7.1L13 4.3a3.5 3.5 0 0 1 5 5l-8.7 8.7a2 2 0 0 1-2.8-2.8l8-8" />
				</svg>
			</button>
			<button
				class={styles.send}
				type="button"
				aria-label="Send"
				disabled={props.sessionExists === false}
				onClick={send}
			>
				<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
					<path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
				</svg>
			</button>
		</div>
	);
}
