import { type Component, createEffect, onCleanup, onMount, Show } from "solid-js";
import { acpTranscript } from "../../stores/acpTranscript";
import { appLogger } from "../../stores/appLogger";
import { chatViewAvailability, chatViewKey, chatViewStore } from "../../stores/chatView";
import { terminalsStore } from "../../stores/terminals";
import { Transcript, type TranscriptSearchRef } from "../AIChatPanel/Transcript";
import s from "./TerminalChatView.module.css";

/** A read keeps the backend tail alive; events alone would let it lapse while the agent is idle. */
const KEEPALIVE_MS = 5_000;

/**
 * Read-only transcript of a Claude terminal, shown in place of the grid.
 *
 * Mounted only while the terminal is in chat mode; the grid stays mounted and
 * hidden by the caller. Falls back to the grid on its own when the agent goes
 * away or the backend reports the terminal as not bound, and says why.
 */
export const TerminalChatView: Component<{
	terminalId: string;
	sessionId: string;
	onSearchRef?: (ref: TranscriptSearchRef | undefined) => void;
}> = (props) => {
	createEffect(() => {
		const availability = chatViewAvailability(terminalsStore.get(props.terminalId));
		const reason = availability.available ? chatViewStore.unavailableReason(props.sessionId) : availability.reason;
		if (reason === null) return;
		chatViewStore.markUnavailable(props.sessionId, reason);
		terminalsStore.setViewMode(props.terminalId, "cli");
	});

	let disposed = false;
	let unwatch: (() => void) | undefined;
	let keepalive: ReturnType<typeof setInterval> | undefined;
	const refresh = () =>
		chatViewStore
			.refresh(props.sessionId)
			.catch((error) => appLogger.error("terminal", "Chat view refresh failed", { sessionId: props.sessionId, error }));

	onMount(() => {
		void chatViewStore.watch(props.sessionId).then((dispose) => {
			if (disposed) {
				dispose();
				return;
			}
			unwatch = dispose;
			void refresh();
			keepalive = setInterval(() => void refresh(), KEEPALIVE_MS);
		});
	});
	onCleanup(() => {
		disposed = true;
		clearInterval(keepalive);
		unwatch?.();
	});

	return (
		<div class={s.chatView} data-testid="terminal-chat-view">
			<Transcript
				onSearchRef={props.onSearchRef}
				entries={() => acpTranscript.entries(chatViewKey(props.sessionId))}
				busy={() => false}
				observeToolDuration={false}
				emptyMessage="No messages in this conversation yet."
			/>
			<div class={s.hint}>Reply below with Compose. Switch to CLI to answer permission prompts.</div>
		</div>
	);
};

/** The CLI | Chat switch of a terminal that runs an agent. */
export const ViewModeToggle: Component<{ terminalId: string }> = (props) => {
	const term = () => terminalsStore.get(props.terminalId);
	const availability = () => chatViewAvailability(term());
	const mode = () => term()?.viewMode ?? "cli";
	const reason = () => {
		const a = availability();
		return a.available ? undefined : a.reason;
	};
	return (
		<Show when={term()?.agentType}>
			<div class={s.toggle} role="group" aria-label="Terminal view">
				<button
					type="button"
					class={s.toggleButton}
					aria-pressed={mode() === "cli"}
					onClick={() => terminalsStore.setViewMode(props.terminalId, "cli")}
				>
					CLI
				</button>
				<button
					type="button"
					class={s.toggleButton}
					aria-pressed={mode() === "chat"}
					disabled={!availability().available}
					title={reason()}
					onClick={() => {
						const sessionId = term()?.sessionId;
						if (sessionId) chatViewStore.clearUnavailable(sessionId);
						terminalsStore.setViewMode(props.terminalId, "chat");
					}}
				>
					Chat
				</button>
			</div>
		</Show>
	);
};

/** One line left behind when the chat view closed itself. */
export const ChatViewFallbackBanner: Component<{ terminalId: string }> = (props) => {
	const term = () => terminalsStore.get(props.terminalId);
	const reason = () => {
		const sessionId = term()?.sessionId;
		return sessionId && term()?.viewMode === "cli" ? chatViewStore.unavailableReason(sessionId) : null;
	};
	return <Show when={reason()}>{(text) => <div class={s.fallbackBanner}>Chat view closed: {text()}</div>}</Show>;
};
