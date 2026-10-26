import { type Component, createEffect, onCleanup, onMount } from "solid-js";
import type { AgentType } from "../../agents";
import { listen } from "../../invoke";
import { createVisibilityInterval } from "../../mobile/utils/visibilityInterval";
import { acpTranscript } from "../../stores/acpTranscript";
import { type ChatViewWake, chatViewKey, chatViewStore } from "../../stores/chatView";
import { settingsStore } from "../../stores/settings";
import { Transcript, type TranscriptSearchRef } from "../AIChatPanel/Transcript";
import s from "./TerminalChatView.module.css";

/** Shared lifecycle and renderer; hosts own mode, input and navigation. */
export const TerminalChatTranscript: Component<{
	sessionId: string;
	terminalAgent?: AgentType;
	/** Desktop session zoom; absent for hosts with their own typography. */
	fontSize?: number;
	mobile?: boolean;
	subscribeWake?: ChatViewWake;
	onUnavailable: (reason: string) => void;
	onAtBottomChange?: (atBottom: boolean) => void;
	onRegisterScrollToBottom?: (jump: (() => void) | undefined) => void;
	onSearchRef?: (ref: TranscriptSearchRef | undefined) => void;
	onOpenFile?: (href: string) => void | Promise<boolean | undefined> | Promise<void>;
}> = (props) => {
	let disposed = false;
	let ready = false;
	let unwatch: (() => void) | undefined;
	const refresh = () => {
		if (ready && !disposed && document.visibilityState !== "hidden") {
			void chatViewStore.refresh(props.sessionId).catch(() => {});
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
	return (
		<div
			class={s.chatView}
			data-testid="terminal-chat-view"
			data-session-zoom={props.fontSize !== undefined ? "true" : undefined}
			style={{ "--font-base": props.fontSize !== undefined ? `${props.fontSize}px` : undefined }}
		>
			<Transcript
				terminalAgent={props.terminalAgent}
				mobile={props.mobile}
				onAtBottomChange={props.onAtBottomChange}
				onRegisterScrollToBottom={props.onRegisterScrollToBottom}
				onSearchRef={props.onSearchRef}
				onOpenFile={props.onOpenFile}
				entries={() => acpTranscript.entries(chatViewKey(props.sessionId))}
				busy={() => false}
				observeToolDuration={false}
				muteIntermediate={settingsStore.state.chatMuteIntermediateText}
				emptyMessage="No messages in this conversation yet."
			/>
		</div>
	);
};
