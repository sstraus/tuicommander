import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { getCtrlEnterSequence } from "../../agents";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import { retryWrite } from "../utils/retryWrite";
import styles from "./TerminalKeybar.module.css";

interface TerminalKeybarProps {
	sessionId: string;
	agentType?: string | null;
	awaitingInput?: boolean;
	choicePromptOpen?: boolean;
	/** True when the question was detected with high confidence (Ink menu footer) */
	questionConfident?: boolean;
	sessionExists?: boolean;
	onCommandWidgetOpen?: () => void;
	/** Insert "/" at the composer selection through its normal input path. */
	onSlashRequest?: () => void;
}

interface KeyDef {
	label: string;
	seq: string;
	danger?: boolean;
	autoEnter?: boolean;
	confirm?: boolean;
}

const STANDARD_KEYS: KeyDef[] = [
	{ label: "Tab", seq: "\t" },
	{ label: "Esc", seq: "\x1b" },
	{ label: "\u2191", seq: "\x1b[A" },
	{ label: "\u2193", seq: "\x1b[B" },
	{ label: "\u2190", seq: "\x1b[D" },
	{ label: "\u2192", seq: "\x1b[C" },
	{ label: "\u21B5", seq: "\r" },
];

const CONTROL_KEYS: KeyDef[] = [
	{ label: "Ctrl+C", seq: "\x03", danger: true },
	{ label: "Ctrl+B", seq: "\x02" },
	{ label: "Ctrl+D", seq: "\x04", danger: true },
	{ label: "Ctrl+Enter", seq: "\x1b[13;5u" },
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
	const [ctrlOpen, setCtrlOpen] = createSignal(false);
	let ctrlButton: HTMLButtonElement | undefined;
	let ctrlMenu: HTMLDivElement | undefined;

	createEffect(() => {
		if (!ctrlOpen()) return;
		const dismiss = (event: MouseEvent) => {
			if (!ctrlButton?.contains(event.target as Node) && !ctrlMenu?.contains(event.target as Node)) setCtrlOpen(false);
		};
		const dismissOnEscape = (event: KeyboardEvent) => {
			if (event.key === "Escape") {
				event.preventDefault();
				setCtrlOpen(false);
			}
		};
		document.addEventListener("click", dismiss);
		document.addEventListener("keydown", dismissOnEscape);
		onCleanup(() => {
			document.removeEventListener("click", dismiss);
			document.removeEventListener("keydown", dismissOnEscape);
		});
	});
	createEffect(() => {
		if (props.sessionExists === false) setCtrlOpen(false);
	});

	async function send(seq: string, autoEnter?: boolean) {
		if (props.sessionExists === false) return;
		const data = autoEnter ? seq + "\r" : seq;
		const label = seq.length <= 3 ? JSON.stringify(seq) : `${seq.length}b`;
		appLogger.debug("terminal", `TerminalKeybar send: ${label} to ${props.sessionId}`);
		setSending(true);
		try {
			await retryWrite(() => rpc("write_pty", { sessionId: props.sessionId, data }));
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.error("network", `Key send failed after retries: ${msg}`);
			toastsStore.add("Send failed", `Could not send key: ${msg}`, "error", true);
		} finally {
			setSending(false);
		}
	}

	function handleSlash() {
		if (props.sessionExists === false) return;
		props.onSlashRequest?.();
	}

	const confirmKeys = () => getConfirmKeys(props.agentType, props.questionConfident);

	return (
		<div class={styles.root}>
			<Show when={ctrlOpen()}>
				<div ref={ctrlMenu} class={styles.ctrlMenu} role="menu" aria-label="Control keys">
					<For each={CONTROL_KEYS}>
						{(k) => (
							<button
								role="menuitem"
								class={styles.key}
								classList={{ [styles.danger]: !!k.danger }}
								disabled={sending() || props.sessionExists === false}
								onMouseDown={(event) => event.preventDefault()}
								onClick={() => {
									setCtrlOpen(false);
									void send(k.label === "Ctrl+Enter" ? getCtrlEnterSequence(props.agentType) : k.seq);
								}}
							>
								{k.label}
							</button>
						)}
					</For>
				</div>
			</Show>
			<div class={styles.bar}>
				<Show when={props.awaitingInput && !props.choicePromptOpen}>
					<For each={confirmKeys()}>
						{(k) => (
							<button
								class={`${styles.key} ${styles.confirm}`}
								classList={{ [styles.sending]: sending() }}
								disabled={sending() || props.sessionExists === false}
								onClick={() => send(k.seq, k.autoEnter)}
							>
								{k.label}
							</button>
						)}
					</For>
					<div class={styles.divider} />
				</Show>
				<button class={`${styles.key} ${styles.accent}`} disabled={props.sessionExists === false} onClick={handleSlash}>
					/
				</button>
				<button
					class={styles.key}
					ref={ctrlButton}
					aria-haspopup="menu"
					aria-expanded={ctrlOpen()}
					disabled={props.sessionExists === false}
					onMouseDown={(event) => event.preventDefault()}
					onClick={() => setCtrlOpen(!ctrlOpen())}
				>
					Ctrl
				</button>
				<For each={STANDARD_KEYS}>
					{(k) => (
						<button
							class={styles.key}
							classList={{ [styles.danger]: !!k.danger }}
							disabled={props.sessionExists === false}
							onClick={() => send(k.seq)}
						>
							{k.label}
						</button>
					)}
				</For>
			</div>
		</div>
	);
}
