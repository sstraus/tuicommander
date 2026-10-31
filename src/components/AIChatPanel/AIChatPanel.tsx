import { emitTo } from "@tauri-apps/api/event";
import { type Component, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { useAgentDetection } from "../../hooks/useAgentDetection";
import { invoke } from "../../invoke";
import { stopChatVoice } from "../../services/chatVoice";
import { acpTranscript } from "../../stores/acpTranscript";
import { appLogger } from "../../stores/appLogger";
import { settingsStore } from "../../stores/settings";
import { terminalsStore } from "../../stores/terminals";
import { uiStore } from "../../stores/ui";
import { isTauri } from "../../transport";
import { cx } from "../../utils";
import { isBackendVersionMismatch } from "../../utils/backendMismatch";
import { openFileLinkHere, resolveFileLink } from "../../utils/openFileLink";
import { SETTINGS_SECTION_EGO } from "../SettingsPanel/sections";
import { Button } from "../ui/Button";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { PanelWindowControls } from "../ui/PanelWindowControls";
import s from "./AIChatPanel.module.css";
import { Composer } from "./Composer";
import { aiChatDraft } from "./draft";
import { Interactions } from "./Interactions";
import { NewConversationDialog } from "./NewConversationDialog";
import { RemoteInteractions } from "./RemoteInteractions";
import { conversationLabel, SessionControls, SessionSettingsDialog } from "./SessionControls";
import { Transcript, type TranscriptSearchRef } from "./Transcript";
import { openChatTableDocument } from "./tableDocument";
import { trackPanelWidth } from "./trackPanelWidth";
import { createAcpChat } from "./useAcpChat";

const isPanelMode = () => new URLSearchParams(window.location.search).get("mode") === "panel";

export interface AIChatPanelProps {
	visible: boolean;
	onClose: () => void;
	onOpenSettings?: (tab: string, section?: string) => void;
	/** The repository on screen: a hint sent with each prompt, not the chat's scope. */
	repoPath: string | null;
	/** Effective filesystem root — the worktree path when on a linked worktree. */
	fsRoot?: string | null;
}

/**
 * AI Chat, running on ego over ACP.
 *
 * One chat for the whole app, across repositories: switching repository keeps
 * the same tabs and conversation and starts nothing, and ego is launched by the
 * panel opening. It does not bind to a terminal, and there is no per-terminal
 * lock: a turn ego runs outlives any tab and may touch files no tab is showing.
 * The panel is a control plane over an agent that lives outside it — see
 * `docs/user-guide/ai-chat.md`.
 *
 * Nothing in here interprets a frame or holds a cursor. `acpTranscript` folds
 * the stream into something a person reads, `acpStore` holds what is true about
 * the connection, and `createAcpChat` owns which of those this panel is looking
 * at.
 */
export const AIChatPanel: Component<AIChatPanelProps> = (props) => {
	// The worktree path where there is one: the hint names the files the user is
	// looking at, not the repository's main checkout.
	const viewed = createMemo(() => props.fsRoot || props.repoPath || null);
	const chat = createAcpChat(viewed, () => props.visible);
	// Detaching transfers the view, not the audio binding. Only the last view
	// closing revokes hands-free, whether it was docked or in its own window.
	let hadView = false;
	createEffect(() => {
		const open = props.visible || uiStore.isDetached("ai-chat");
		if (hadView && !open) void stopChatVoice();
		hadView = open;
	});

	// The orchestrator unmounts the docked panel when it closes or detaches.
	onCleanup(() => {
		if (!uiStore.isDetached("ai-chat")) void stopChatVoice();
	});

	const [newOptionsOpen, setNewOptionsOpen] = createSignal(false);
	const [sessionSettingsOpen, setSessionSettingsOpen] = createSignal(false);
	// Toasts keep clear of this panel's rendered width (see ToastContainer).
	let panelEl!: HTMLDivElement;
	onMount(() => onCleanup(trackPanelWidth(panelEl, (width) => uiStore.setAiChatPanelMeasuredWidth(width))));
	createEffect(() => aiChatDraft.activate(chat.sessionId() ?? ""));
	let transcriptSearch: TranscriptSearchRef | undefined;
	const keyDown = (event: KeyboardEvent) => {
		if (event.key === "Escape" && !event.defaultPrevented && !event.metaKey && !event.ctrlKey && !event.altKey) {
			const searchClosed = transcriptSearch?.close();
			if (!searchClosed && !chat.busy()) return;
			event.preventDefault();
			event.stopPropagation();
			if (!searchClosed) void chat.cancel();
			return;
		}
		if (!(event.metaKey || event.ctrlKey) || event.shiftKey) return;
		const key = event.key.toLowerCase();
		if (key === "t" && (isTauri() || event.altKey)) {
			event.preventDefault();
			event.stopPropagation();
			void chat.startSession();
		} else if (key === "w" && (isTauri() || event.altKey) && chat.sessionId()) {
			event.preventDefault();
			event.stopPropagation();
			void chat.closeTab(chat.sessionId() as string);
		}
	};
	const openFile = async (href: string) => {
		const cwd = chat.root() ?? viewed();
		if (!cwd) return;
		try {
			const target = await resolveFileLink(cwd, href);
			if (!target) return;
			if (!(isPanelMode() && isTauri())) {
				openFileLinkHere(target);
				return;
			}
			await emitTo("main", "panel-action", {
				panelId: "ai-chat",
				action: target.isDirectory ? "open-directory" : "open-file",
				data: target.isDirectory
					? { path: target.absolutePath }
					: {
							path: target.absolutePath,
							...(target.line ? { line: target.line } : {}),
							...(target.col ? { col: target.col } : {}),
						},
			});
			await invoke("focus_main_window");
		} catch (error) {
			appLogger.error("ai-chat", "Failed to resolve file link", error);
		}
	};

	const detection = useAgentDetection();
	createEffect(() => {
		if (props.visible && chat.phase() === "unconfigured") void detection.detectAll();
	});
	const detectedEgo = () => detection.getDetection("ego")?.path?.trim();

	const configureEgo = async () => {
		if (isPanelMode() && isTauri()) {
			await emitTo("main", "panel-action", { panelId: "ai-chat", action: "configure-ego", data: {} });
			await invoke("focus_main_window");
		} else props.onOpenSettings?.("general", SETTINGS_SECTION_EGO);
	};

	const emptyMessage = () => {
		switch (chat.phase()) {
			case "unconfigured":
				return "AI Chat is inactive because the ego executable is not configured.";
			case "starting":
				return "Connecting conversation…";
			case "failed":
				return isBackendVersionMismatch(chat.error())
					? "The running backend must be restarted before this conversation can be used."
					: "The conversation could not be opened. Use Retry to try again.";
			default:
				if (chat.phase() === "ready" && chat.sessionId())
					return "Conversation disconnected. Send a message to reconnect.";
				return "Ask ego about any repository. The one on screen is sent as context.";
		}
	};

	return (
		<div id="ai-chat-panel" ref={panelEl} class={cx(s.panel, !props.visible && s.hidden)} onKeyDown={keyDown}>
			<PanelResizeHandle panelId="ai-chat-panel" minWidth={300} maxWidth={700} />

			<div class={s.tidyHeader}>
				<SessionControls
					chat={chat}
					detached={isPanelMode()}
					onSessionSettings={() => setSessionSettingsOpen(true)}
					onNewOptions={() => {
						chat.clearError();
						setNewOptionsOpen(true);
					}}
				/>
				<div class={s.headerActions}>
					<PanelWindowControls
						panelId="ai-chat"
						mode={isPanelMode() ? "detached" : "inline"}
						onInlineClose={props.onClose}
					/>
				</div>
			</div>
			{/* Shown before ego runs too: "+" is one of the two ways a chat starts. */}
			<Show when={chat.tabs().length > 1}>
				<div class={s.chatTabs} role="tablist" aria-label="AI Chat tabs">
					<For each={chat.tabs()}>
						{(session) => (
							<div
								class={cx(s.chatTab, chat.sessionId() === session && s.chatTabActive)}
								onAuxClick={(event) => {
									if (event.button !== 1) return;
									event.preventDefault();
									event.stopPropagation();
									void chat.closeTab(session);
								}}
							>
								<Button
									type="button"
									variant="ghost"
									role="tab"
									data-chat-session={session}
									title={session}
									aria-selected={chat.sessionId() === session}
									onClick={() => void chat.selectSession(session)}
								>
									{conversationLabel(
										chat.sessions().find((item) => item.sessionId === session) ?? { sessionId: session },
									)}
								</Button>
								<Show when={chat.tabs().length > 1}>
									<Button
										type="button"
										variant="ghost"
										shape="icon"
										aria-label={`Close chat tab ${session}`}
										onClick={() => void chat.closeTab(session)}
									>
										<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
											<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
										</svg>
									</Button>
								</Show>
							</div>
						)}
					</For>
				</div>
			</Show>
			<Show when={newOptionsOpen()}>
				<NewConversationDialog chat={chat} onClose={() => setNewOptionsOpen(false)} />
			</Show>

			{/* A gap is not a transport hiccup: the journal no longer holds the
			    sequence this window asked for, so the conversation on screen has a
			    hole in it. Saying so and offering the one recovery there is — a
			    fresh process replaying the history — is the whole point of
			    surfacing it rather than skipping ahead in silence. */}
			<Show when={chat.gap()}>
				{(gap) => (
					<div class={s.errorBanner}>
						<span class={s.errorText}>Missed part of this conversation: {gap().message}</span>
						<Button type="button" variant="danger" class={s.retryBtn} onClick={() => void chat.recover()}>
							Recover
						</Button>
					</div>
				)}
			</Show>

			<Show when={chat.error()}>
				{(message) => (
					<div class={s.errorBanner}>
						<span class={s.errorText}>{message()}</span>
						<Show when={!isBackendVersionMismatch(message())}>
							<Button type="button" variant="danger" class={s.retryBtn} onClick={() => void chat.recover()}>
								Retry
							</Button>
						</Show>
					</div>
				)}
			</Show>

			{/* A connection that is up but has nobody reading its journal is not a
			    live panel. Say so rather than showing a conversation that has
			    quietly stopped moving. */}
			<Show when={chat.phase() === "live" && !chat.isStreaming()}>
				<div class={s.frozenBanner}>Not receiving updates.</div>
			</Show>

			<Show when={chat.phase() === "starting"}>
				<div class={s.frozenBanner} role="status">
					Connecting conversation…
				</div>
			</Show>

			<Transcript
				fontSize={terminalsStore.getActive()?.fontSize}
				onSearchRef={(ref) => {
					transcriptSearch = ref;
				}}
				canForkAtMessage={() => chat.capabilities()?.forkAtMessage === true}
				onFork={(messageId) => void chat.fork(messageId)}
				observeToolDuration={false}
				entries={chat.entries}
				busy={chat.busy}
				retry={chat.retry}
				muteIntermediate={settingsStore.state.chatMuteIntermediateText}
				emptyMessage={emptyMessage()}
				onSuggestion={(text) => void chat.send(text)}
				onOpenFile={(href) => void openFile(href)}
				onOpenTable={(source) => {
					if (isPanelMode() && isTauri()) {
						void emitTo("main", "panel-action", { panelId: "ai-chat", action: "open-table", data: { source } })
							.then(() => invoke("focus_main_window"))
							.catch((error) => appLogger.error("ai-chat", "Failed to open table document", error));
					} else openChatTableDocument(source);
				}}
				onNoticeAction={(action) => {
					if (action.kind === "open_result") void openFile(action.path);
				}}
				onClear={() => {
					const session = chat.sessionId();
					if (session) acpTranscript.clear(session);
				}}
			>
				<RemoteInteractions />
				<Interactions
					interactions={chat.interactions}
					onPermission={(requestId, optionId) => void chat.answerPermission(requestId, optionId)}
					onPermissionDismissed={(requestId) => void chat.cancelPermission(requestId)}
					onElicitationAccepted={(requestId, content) =>
						void chat.answerElicitation(requestId, { action: "accept", content })
					}
					onElicitationDeclined={(requestId) => void chat.answerElicitation(requestId, { action: "decline" })}
					onElicitationCancelled={(requestId) => void chat.answerElicitation(requestId, { action: "cancel" })}
				/>
			</Transcript>

			<Show when={chat.phase() === "unconfigured"}>
				<Show when={detectedEgo()}>
					{(path) => (
						<div class={s.detectedEgo}>
							<span>
								Detected ego: <code>{path()}</code>
							</span>
							<Button
								type="button"
								variant="primary"
								onClick={() => {
									if (!settingsStore.isAcpConfigured()) settingsStore.setEgoExecutable(path());
								}}
							>
								Use this
							</Button>
						</div>
					)}
				</Show>
				<div class={s.setupActions}>
					<Button
						type="button"
						variant="primary"
						onClick={() =>
							void configureEgo().catch((error) => appLogger.error("ai-chat", "Failed to open ego settings", error))
						}
					>
						Configure ego
					</Button>
				</div>
			</Show>

			<Show when={chat.phase() !== "unconfigured" && chat.phase() !== "starting"}>
				<Composer chat={chat} onSessionSettings={() => setSessionSettingsOpen(true)} />
			</Show>
			<Show when={sessionSettingsOpen()}>
				<SessionSettingsDialog chat={chat} onClose={() => setSessionSettingsOpen(false)} />
			</Show>
		</div>
	);
};

export default AIChatPanel;
