import { type Component, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import type { AgentType } from "../../agents";
import { invoke, listen } from "../../invoke";
import { createVisibilityInterval } from "../../mobile/utils/visibilityInterval";
import { type ChatViewWake, chatViewStore } from "../../stores/chatView";
import { settingsStore } from "../../stores/settings";
import { HttpRpcError } from "../../transport/http";
import { Transcript, type TranscriptSearchRef } from "../AIChatPanel/Transcript";
import { Button } from "../ui/Button";
import s from "./TerminalChatView.module.css";

/** Shared lifecycle and renderer; hosts own mode, input and navigation. */
export const TerminalChatTranscript: Component<{
	sessionId: string;
	/** Existing host session state; this view never detects agent activity. */
	busy?: boolean;
	awaitingInput?: boolean;
	/** Changes on existing shell-state/output evidence, including an idle receipt. */
	activityRevision?: string | number | null;
	terminalAgent?: AgentType;
	/** Desktop session zoom; absent for hosts with their own typography. */
	fontSize?: number;
	mobile?: boolean;
	subscribeWake?: ChatViewWake;
	onUnavailable: (reason: string) => void;
	onAtBottomChange?: (atBottom: boolean) => void;
	onNewTextBelowChange?: (visible: boolean) => void;
	onRegisterScrollToBottom?: (jump: (() => void) | undefined) => void;
	onSearchRef?: (ref: TranscriptSearchRef | undefined) => void;
	/** Host composer: quote reply and response feedback are drafted into it, never sent. */
	onQuote?: (text: string) => void;
	onOpenFile?: (href: string) => void | Promise<boolean | undefined> | Promise<void>;
}> = (props) => {
	// A submission bridges only the gap before existing state/output catches up.
	// Do not subscribe to the bridge itself here: a retained idle snapshot at Send
	// is not new evidence that the submitted turn already completed.
	const activity = createMemo(() => {
		const cursor = chatViewStore.state.cursors[props.sessionId];
		return JSON.stringify([
			props.sessionId,
			props.busy,
			props.awaitingInput,
			props.activityRevision,
			cursor?.epoch,
			cursor?.nextSeq,
		]);
	});
	createEffect(on(activity, () => chatViewStore.clearSubmission(props.sessionId)));
	let disposed = false;
	let ready = false;
	let unwatch: (() => void) | undefined;
	const [pendingAction, setPendingAction] = createSignal<number | null>(null);
	const [actionError, setActionError] = createSignal<string | null>(null);
	async function actOnQueued(commandId: number, sendNow: boolean) {
		if (pendingAction() !== null) return;
		const sessionId = props.sessionId;
		setPendingAction(commandId);
		setActionError(null);
		try {
			await invoke(sendNow ? "send_queued_agent_command_now" : "remove_queued_agent_command", { sessionId, commandId });
		} catch (error) {
			// The backend reason ("Steer refused (idle_timeout)…") reads better than the transport wrapper.
			if (!disposed)
				setActionError(
					error instanceof HttpRpcError ? error.detail : error instanceof Error ? error.message : String(error),
				);
		} finally {
			await chatViewStore.refreshQueue(sessionId).catch(() => {});
			if (!disposed) setPendingAction(null);
		}
	}
	const [fullText, setFullText] = createSignal<string | null>(null);
	async function showFullText(messageId: string) {
		setActionError(null);
		try {
			const result = await invoke<{ text: string }>("chat_view_full_text", {
				sessionId: props.sessionId,
				messageId,
			});
			if (!disposed) setFullText(result.text);
		} catch (error) {
			if (!disposed)
				setActionError(
					error instanceof HttpRpcError ? error.detail : error instanceof Error ? error.message : String(error),
				);
		}
	}
	const refresh = () => {
		if (ready && !disposed && document.visibilityState !== "hidden") {
			void chatViewStore.refresh(props.sessionId).catch(() => {});
			void chatViewStore.refreshQueue(props.sessionId).catch(() => {});
		}
	};
	createEffect(() => {
		const reason = chatViewStore.unavailableReason(props.sessionId);
		if (reason !== null) props.onUnavailable(reason);
	});
	createVisibilityInterval(refresh, 5_000);
	onMount(() => {
		// Even injected wakes pass through visibility gating: SSE may remain live while hidden.
		const subscribe: ChatViewWake = (id, _wake) =>
			props.subscribeWake
				? props.subscribeWake(id, refresh)
				: listen<{ session_id: string }>("chat-view-changed", (event) => {
						if (event.payload.session_id === id) refresh();
					});
		void chatViewStore.watch(props.sessionId, subscribe).then((dispose) => {
			if (disposed) {
				dispose();
				return;
			}
			unwatch = dispose;
			ready = true;
			refresh();
		});
	});
	onCleanup(() => {
		disposed = true;
		unwatch?.();
	});
	createEffect(() => {
		props.busy;
		props.activityRevision;
		if (ready && !disposed) void chatViewStore.refreshQueue(props.sessionId).catch(() => {});
	});
	return (
		<div class={s.chatView} data-testid="terminal-chat-view">
			{actionError() && (
				<div role="alert" class={s.fallbackBanner}>
					{actionError()}
				</div>
			)}
			<Transcript
				fontSize={props.fontSize}
				terminalAgent={props.terminalAgent}
				mobile={props.mobile}
				onAtBottomChange={props.onAtBottomChange}
				onNewTextBelowChange={props.onNewTextBelowChange}
				onRegisterScrollToBottom={props.onRegisterScrollToBottom}
				onSearchRef={props.onSearchRef}
				onOpenFile={props.onOpenFile}
				onQuote={props.onQuote}
				onShowFullText={(id) => void showFullText(id)}
				onSendPendingNow={!props.awaitingInput ? (id) => void actOnQueued(id, true) : undefined}
				onCancelPending={(id) => void actOnQueued(id, false)}
				pendingAction={pendingAction() !== null}
				entries={() => chatViewStore.entries(props.sessionId)}
				busy={() => !props.awaitingInput && (props.busy === true || !!chatViewStore.state.submissions[props.sessionId])}
				busyLabel="Working…"
				observeToolDuration={false}
				muteIntermediate={settingsStore.state.chatMuteIntermediateText}
				emptyMessage={
					chatViewStore.state.cursors[props.sessionId]?.epoch == null
						? "Loading conversation…"
						: "No messages in this conversation yet."
				}
			/>
			{fullText() !== null && (
				<div class={s.fullText} role="dialog" aria-label="Full message">
					<div class={s.fullTextBar}>
						<Button type="button" variant="ghost" size="compact" onClick={() => setFullText(null)}>
							Close
						</Button>
					</div>
					<pre class={s.fullTextBody} data-testid="chat-full-text">
						{fullText()}
					</pre>
				</div>
			)}
		</div>
	);
};
