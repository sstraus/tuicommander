import { createEffect, createSignal, Show } from "solid-js";
import type { AgentType } from "../../agents";
import { TerminalChatTranscript } from "../../components/Terminal/TerminalChatTranscript";
import { Button } from "../../components/ui";
import { ButtonGroup } from "../../components/ui/ButtonGroup";
import { type ChatViewWake, chatViewStore, supportsTerminalChatView } from "../../stores/chatView";
import { mobileChatReadingStyle } from "../mobileFontSize";
import styles from "./SessionChatView.module.css";

/** Mode belongs to a PTY, not to its polled SessionInfo object. */
const modes = new Map<string, "cli" | "chat">();
export function useSessionChatView(props: {
	sessionId: () => string;
	agentType: () => string | undefined;
	sessionExists: () => boolean;
	subscribeWake: ChatViewWake;
}) {
	const [atBottom, setAtBottom] = createSignal(true);
	let jump: (() => void) | undefined;
	const [mode, setMode] = createSignal<"cli" | "chat">(modes.get(props.sessionId()) ?? "cli");
	createEffect(() => setMode(modes.get(props.sessionId()) ?? "cli"));
	const select = (next: "cli" | "chat") => {
		modes.set(props.sessionId(), next);
		setMode(next);
	};
	// SessionInfo has no transcript binding: chat_view_snapshot remains the authority.
	const available = () => supportsTerminalChatView(props.agentType()) && props.sessionExists();
	createEffect(() => {
		if (mode() === "chat" && !available()) {
			chatViewStore.markUnavailable(props.sessionId(), "No agent is running in this terminal");
			select("cli");
		}
	});
	const active = () => mode() === "chat" && available();
	const openChat = () => {
		chatViewStore.clearUnavailable(props.sessionId());
		select("chat");
	};
	const Toggle = () => (
		<Show when={available()}>
			<ButtonGroup class={styles.toggle} aria-label="Terminal view">
				<Button type="button" aria-pressed={!active()} onClick={() => select("cli")}>
					CLI
				</Button>
				<Button type="button" aria-pressed={active()} onClick={openChat}>
					Chat
				</Button>
			</ButtonGroup>
		</Show>
	);
	const View = () => (
		<>
			<Show when={!active() && chatViewStore.unavailableReason(props.sessionId())}>
				{(reason) => (
					<div class={styles.notice} role="status">
						Chat view closed: {reason()}
					</div>
				)}
			</Show>
			<Show when={active() && props.sessionId()} keyed>
				{(sessionId) => (
					<div class={styles.reading} data-mobile-reading style={mobileChatReadingStyle()}>
						<TerminalChatTranscript
							sessionId={sessionId}
							terminalAgent={props.agentType() as AgentType}
							mobile
							onAtBottomChange={setAtBottom}
							onRegisterScrollToBottom={(fn) => {
								jump = fn;
								setAtBottom(true);
							}}
							subscribeWake={props.subscribeWake}
							onUnavailable={() => select("cli")}
						/>
					</div>
				)}
			</Show>
		</>
	);
	return { active, available, openChat, Toggle, View, atBottom, scrollToBottom: () => jump?.() };
}
