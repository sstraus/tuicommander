import { createEffect, createMemo, createSignal, onCleanup, onMount, Show } from "solid-js";
import { Button } from "../../components/ui";
import { uploadAttachment } from "../../services/uploadAttachment";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { HttpRpcError, rpc } from "../../transport";
import { waitForAgentEnterGap } from "../../utils/sendCommand";
import { sendNativeChoice } from "../../utils/sendNativeChoice";
import { useBrowserDictation } from "../useBrowserDictation";
import { useKeyboardCorrection } from "../useKeyboardCorrection";
import type { ChoicePrompt, SlashMenuItem } from "../useSessions";
import { clearDictationPartial, loadDictationPartial, saveDictationPartial } from "../utils/dictationPartial";
import { retryWrite } from "../utils/retryWrite";
import { ChoicePromptOverlay } from "./ChoicePromptOverlay";
import styles from "./CommandInput.module.css";
import { MobileComposerMenu } from "./MobileComposerMenu";
import { MobileDictationButton } from "./MobileDictationButton";
import { SlashMenuOverlay } from "./SlashMenuOverlay";
import { type ComposerInputKey, computeInputDelta, isPostSendGuardActive, isSupersetEcho } from "./syncGuards";

interface CommandInputProps {
	sessionId: string;
	serverVoiceRecording?: boolean;
	onBrowserDictating?: (recording: boolean) => void;
	/** Reveal the native answer controls after an authoritative composer refusal. */
	onSelectionRequired?: (sessionId: string) => void;
	/** When set, prefills the textarea and focuses it. Seq counter ensures re-fire on same text. */
	prefillValue?: { text: string; seq: number };
	/** Current PTY input line text (synced from terminal prompt via WebSocket). */
	ptyInputLine?: string | null;
	/** Detected agent type (e.g. "claude", "codex"). */
	agentType?: string | null;
	/** Slash menu items from session state (populated by backend parser). */
	slashItems?: SlashMenuItem[];
	/** Active numbered choice dialog parsed from agent output. */
	choicePrompt?: ChoicePrompt;
	/** Native free-text question, opened explicitly by the user. */
	questionText?: string;
	/** The Codex question panel has been opened with Shift+Left in this view. */
	codexQuestionOpen?: boolean;
	/** Managed agent waiting for a free-text answer. */
	managedSession?: boolean;
	awaitingInput?: boolean;
	sessionExists?: boolean;
	/** Registers character insertion through the same path as composer typing. */
	onRegisterInsertText?: (fn: (text: string) => void) => void;
	/** Routes keybar completion/history through the composer input request. */
	onRegisterInputKey?: (fn: (key: ComposerInputKey) => void) => void;
}

export function CommandInput(props: CommandInputProps) {
	const keyboardCorrection = useKeyboardCorrection();
	// Offer a restored dictated draft without writing on mount. The PTY may have
	// changed while this client was away; reconcile it on the first explicit edit/send.
	const restoredPartial = loadDictationPartial(props.sessionId);
	let savedPartial = restoredPartial;
	const [value, setValue] = createSignal(restoredPartial);
	const [submitting, setSubmitting] = createSignal(false);
	const [queueMenu, setQueueMenu] = createSignal(false);
	let pressTimer: ReturnType<typeof setTimeout> | undefined;
	let held = false;
	function cancelPress() {
		clearTimeout(pressTimer);
		pressTimer = undefined;
	}
	onCleanup(cancelPress);
	function startPress(event: PointerEvent) {
		if (event.button !== 0 || submitting()) return;
		cancelPress();
		held = false;
		pressTimer = setTimeout(() => {
			held = true;
			setQueueMenu(true);
		}, 500);
	}
	async function queueDraft() {
		const draft = textareaEl?.value ?? value();
		const text = draft.trim();
		if (!text || submitting() || uploading() || props.sessionExists === false) return;
		const sessionId = props.sessionId;
		cancelPress();
		setQueueMenu(false);
		browserDictation.cancel();
		setSubmitting(true);
		try {
			await lastInputWrite;
			const result = await rpc<{ accepted: boolean; typed: boolean }>("enqueue_agent_command", {
				sessionId,
				text,
				expectedDraft: draft,
			});
			if (!result.accepted) throw new Error("Queue was not accepted");
			syncedText = "";
			inputRequest = null;
			lastSendAt = Date.now();
			setValue("");
			if (textareaEl) {
				textareaEl.value = "";
				textareaEl.style.height = "auto";
			}
			toastsStore.add(
				"Prompt queued",
				result.typed ? "Submitted at the ready window" : "Waiting for the next ready window",
				"info",
			);
		} catch (error) {
			toastsStore.add(
				"Queue failed",
				`Queue state uncertain, check the session. ${error instanceof Error ? error.message : String(error)}`,
				"error",
				true,
			);
		} finally {
			setSubmitting(false);
		}
	}
	const [uploading, setUploading] = createSignal<string | null>(null);
	const [choiceSending, setChoiceSending] = createSignal(false);
	const [codexNotesMode, setCodexNotesMode] = createSignal(false);
	const atomicReply = () =>
		props.managedSession && props.awaitingInput && !props.choicePrompt && !props.codexQuestionOpen && !codexNotesMode();
	// A multi-question form replaces its prompt without an empty frame between
	// the answered question and the next one, so a new prompt re-arms the buttons too.
	// Keyed on content, not object identity: an identical snapshot redelivered
	// while an answer is in flight must not re-arm the buttons.
	const choiceSignature = createMemo(() => {
		const prompt = props.choicePrompt;
		if (!prompt) return null;
		return JSON.stringify([prompt.title, prompt.options.map((option) => [option.key, option.label])]);
	});
	createEffect(() => {
		choiceSignature();
		setChoiceSending(false);
	});
	let textareaEl: HTMLTextAreaElement | undefined;
	// The local draft baseline, including explicitly requested PTY completions
	// and history replacements. Every edit computes its delta from this text.
	let syncedText: string | null = restoredPartial ? null : "";
	let inputRequest: { kind: "completion" | "history"; armed: boolean } | null = null;
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
	//      only after Tab, if it extends a nonempty syncedText. Explicit
	//      Up/Down requests instead accept one history replacement, including empty.
	// Everything else (prompt redraws, lagging echoes over slow links,
	// unsolicited history replacements, automated voice pastes) is ignored so the
	// textarea can't be clobbered or resurrect a command already sent elsewhere.
	createEffect(() => {
		if (atomicReply()) return;
		const text = props.ptyInputLine ?? "";
		if (!inputRequest?.armed || props.ptyInputLine == null) return;
		if (inputRequest.kind === "completion") {
			if (isPostSendGuardActive(Date.now(), lastSendAt)) return;
			if (!isSupersetEcho(text, syncedText ?? restoredPartial)) return;
		}
		inputRequest = null;
		syncedText = text;
		setValue(text);
		if (textareaEl) {
			textareaEl.value = text;
			autoResize();
		}
	});

	onMount(() => {
		if (!restoredPartial || !textareaEl) return;
		textareaEl.value = restoredPartial;
		autoResize();
	});
	// Any draft other than the one dictation saved (an edit, a send, Escape, history) retires the partial.
	createEffect(() => {
		if (value() !== savedPartial) clearDictationPartial(props.sessionId);
	});

	function autoResize() {
		if (!textareaEl) return;
		textareaEl.style.height = "auto";
		// Native field sizing follows wrapping without pinning the first paint's height.
		if (globalThis.CSS?.supports?.("field-sizing", "content")) return;
		textareaEl.style.height = Math.min(textareaEl.scrollHeight, 120) + "px";
	}

	function writePty(data: string) {
		const write = rpc("write_pty", { sessionId: props.sessionId, data });
		lastInputWrite = write.catch((err: unknown) => {
			appLogger.warn("network", "Failed to write to PTY", { error: err });
		});
		return write;
	}

	/** Send character deltas to PTY so the remote input stays in sync.
	 *  Uses a minimal end-anchored delta (computeInputDelta) — a mid-line edit
	 *  backspaces only the divergent tail instead of nuking and retyping the
	 *  whole line, which previously caused a keystroke storm and visible mess. */
	function syncDelta(newText: string) {
		inputRequest = null;
		if (atomicReply() || props.sessionExists === false) return;
		const delta = computeInputDelta(syncedText ?? props.ptyInputLine ?? restoredPartial, newText);
		if (delta) writePty(delta);
		syncedText = newText;
	}

	function updateInput(text: string) {
		setValue(text);
		autoResize();
		syncDelta(text);
	}

	const browserDictation = useBrowserDictation({
		sessionId: () => props.sessionId,
		blocked: () => submitting() || !!props.serverVoiceRecording || props.sessionExists === false,
		getDraft: () => textareaEl?.value ?? value(),
		setDraft: (text) => {
			if (textareaEl) textareaEl.value = text;
			savedPartial = text;
			saveDictationPartial(props.sessionId, text);
			updateInput(text);
		},
		onRecording: (active) => props.onBrowserDictating?.(active),
	});

	function handleInput(e: InputEvent & { currentTarget: HTMLTextAreaElement }) {
		browserDictation.cancel();
		updateInput(e.currentTarget.value);
	}

	function insertText(text: string) {
		if (props.sessionExists === false || submitting() || !textareaEl) return;
		browserDictation.cancel();
		textareaEl.setRangeText(text, textareaEl.selectionStart, textareaEl.selectionEnd, "end");
		updateInput(textareaEl.value);
		textareaEl.focus();
	}

	async function attachFile(file: File): Promise<void> {
		setUploading(file.name);
		try {
			const receipt = await uploadAttachment(file, { kind: "pty", id: props.sessionId });
			if (receipt.notice) toastsStore.add("Attachment access", receipt.notice, "warn", true);
			// DEFERRED (2026-10-19) — upload completion still edits the live draft;
			// changing sessions during upload needs a separate draft-ownership contract.
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
		props.onRegisterInputKey?.(requestInputKey);
	});

	function requestInputKey(key: ComposerInputKey) {
		if (props.sessionExists === false) return;
		const data = key === "Tab" ? "\t" : key === "ArrowUp" ? "\x1b[A" : "\x1b[B";
		// A waiting agent can own a picker without a parsed choice_prompt. Keep
		// navigation in the PTY, not in the free-text answer or shell history.
		const request = atomicReply()
			? null
			: key === "Tab"
				? (syncedText ?? restoredPartial).length > 0
					? { kind: "completion" as const, armed: true }
					: null
				: { kind: "history" as const, armed: false };
		inputRequest = request;
		void writePty(data).then(
			() => {
				if (request && inputRequest === request) {
					// DEFERRED (2026-10-10): input_line has no causal request ID.
					// A history response before the HTTP write ack is dropped (missed recall);
					// a delayed unrelated snapshot after the ack can still be mistaken for history.
					request.armed = true;
				}
			},
			() => {
				if (inputRequest === request) inputRequest = null;
			},
		);
	}

	async function send() {
		const text = (textareaEl?.value ?? value()).trim();
		if (!text || props.sessionExists === false || submitting()) return;
		const sessionId = props.sessionId;
		const agentType = props.agentType;

		browserDictation.cancel();

		if (atomicReply()) {
			setSubmitting(true);
			try {
				const receipt = await rpc<{
					status: string;
					submitted: boolean;
					acknowledged: boolean;
					reason?: string;
					detail?: string;
				}>("submit_agent_reply", { sessionId, input: text });
				if (!receipt.submitted) {
					if (receipt.reason === "selection_required") props.onSelectionRequired?.(sessionId);
					toastsStore.add(
						"Reply not sent",
						receipt.detail ?? receipt.reason ?? "The session is not ready for a reply",
						"error",
						true,
					);
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
						const detail =
							typeof result === "object" && result !== null && "detail" in result ? result.detail : undefined;
						if (reason === "selection_required") props.onSelectionRequired?.(sessionId);
						if (typeof detail === "string" && detail.trim()) msg = detail;
						else if (reason === "session_not_found") msg = "This session has ended";
						else if (reason === "agent_not_ready")
							msg = props.awaitingInput
								? "The agent is waiting for input. Check its question before retrying."
								: "The agent is busy. Check the session before retrying.";
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

		if (syncedText === null) syncDelta(textareaEl?.value ?? value());
		lastSendAt = Date.now();
		inputRequest = null;
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
			await waitForAgentEnterGap(agentType);
			await retryWrite(() => rpc("write_pty", { sessionId, data: "\r" }));
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
			requestInputKey("Tab");
			return;
		}
		if (e.key === "Enter" && !e.shiftKey) {
			e.preventDefault();
			send();
		}
		if (e.key === "Escape") {
			inputRequest = null;
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
		// Acknowledgements can outlive the detail screen. Keep the whole answer
		// bound to the session and dialog that owned the tap, not live props.
		const sessionId = props.sessionId;
		const prompt = props.choicePrompt;
		const agentType = props.agentType;
		setChoiceSending(true);
		try {
			const write = (data: string) => rpc<void>("write_pty", { sessionId, data });
			await sendNativeChoice(write, prompt, agentType, key, () => {
				setCodexNotesMode(true);
				textareaEl?.focus();
			});
			// Navigation reveals native rows; it does not submit the question.
			if (prompt?.selection_mode === "navigate-only" && key !== "↵") setChoiceSending(false);
		} catch (err) {
			setChoiceSending(false);
			appLogger.warn("terminal", "ChoicePrompt sendPtyKey failed", { error: err });
			const msg = err instanceof Error ? err.message : String(err);
			toastsStore.add("Send failed", `Could not send choice: ${msg}`, "error", true);
		}
	}

	return (
		<div class={styles.form}>
			<Show when={uploading()}>
				{(name) => (
					<div class={styles.uploading} role="status">
						Uploading {name()}…
					</div>
				)}
			</Show>
			<Show when={props.questionText && !showChoicePrompt()}>
				<div class={styles.question} role="status">
					{props.questionText}
				</div>
			</Show>
			<Show when={showChoicePrompt()}>
				<ChoicePromptOverlay prompt={props.choicePrompt!} disabled={choiceSending()} onSelect={handleChoiceSelect} />
			</Show>
			<Show when={showDropup() && !showChoicePrompt()}>
				<SlashMenuOverlay
					items={slashItems()}
					sessionId={props.sessionId}
					onSelect={handleSlashSelect}
					onClose={closeSlashMenu}
				/>
			</Show>
			<MobileComposerMenu
				disabled={submitting() || props.sessionExists === false}
				uploading={!!uploading()}
				onAttach={atomicReply() ? undefined : (file) => void attachFile(file)}
				onQueue={() => void queueDraft()}
				queueDisabled={!value().trim()}
				queueOnly={queueMenu()}
				onClose={() => setQueueMenu(false)}
				onCommands={!atomicReply() && (props.agentType || slashItems().length > 0) ? () => insertText("/") : undefined}
			/>
			{/* DEFERRED (2026-10-14) — aiChatDraft parking is an ACP singleton, not an independent PTY draft store. */}

			<div class={styles.composer}>
				<textarea
					data-control-layout="composer"
					ref={textareaEl}
					class={styles.input}
					placeholder="Type a command..."
					onInput={handleInput}
					onKeyDown={handleKeyDown}
					autocomplete="off"
					autocorrect={keyboardCorrection.enabled() ? "on" : "off"}
					spellcheck={keyboardCorrection.enabled()}
					autocapitalize={keyboardCorrection.enabled() ? "sentences" : "off"}
					inputmode="text"
					rows={1}
					disabled={props.sessionExists === false || submitting()}
				/>
				<MobileDictationButton
					dictation={browserDictation}
					disabled={submitting() || !!props.serverVoiceRecording || props.sessionExists === false}
				/>
			</div>
			<Button
				shape="icon"
				variant="primary"
				circular
				class={styles.send}
				type="button"
				aria-label="Send"
				disabled={props.sessionExists === false || submitting()}
				aria-haspopup="menu"
				aria-expanded={queueMenu()}
				onPointerDown={startPress}
				onPointerUp={cancelPress}
				onPointerLeave={cancelPress}
				onPointerCancel={() => {
					cancelPress();
					held = true;
				}}
				onContextMenu={(event) => event.preventDefault()}
				onClick={() => {
					if (held) {
						held = false;
						return;
					}
					setQueueMenu(false);
					void send();
				}}
			>
				<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
					<path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
				</svg>
			</Button>
		</div>
	);
}
