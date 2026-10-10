import { emitTo } from "@tauri-apps/api/event";
import { type Component, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { acpTranscript } from "../../stores/acpTranscript";
import { appLogger } from "../../stores/appLogger";
import { uiStore } from "../../stores/ui";
import { isTauri } from "../../transport";
import { cx } from "../../utils";
import { openTerminalFilePath } from "../../utils/filePreview";
import { SETTINGS_SECTION_EGO } from "../SettingsPanel/sections";
import p from "../shared/panel.module.css";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { PanelWindowControls } from "../ui/PanelWindowControls";
import s from "./AIChatPanel.module.css";
import { Composer } from "./Composer";
import { aiChatDraft } from "./draft";
import { Interactions } from "./Interactions";
import { NewConversationDialog } from "./NewConversationDialog";
import { RemoteInteractions } from "./RemoteInteractions";
import { conversationLabel, SessionControls } from "./SessionControls";
import { Transcript } from "./Transcript";
import { trackPanelWidth } from "./trackPanelWidth";
import { createAcpChat } from "./useAcpChat";

const isPanelMode = () => new URLSearchParams(window.location.search).get("mode") === "panel";

/** The last segment of a path, for the header. */
function basename(path: string): string {
	const parts = path.split(/[/\\]/).filter(Boolean);
	return parts.at(-1) ?? path;
}

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
 * first message. It does not bind to a terminal, and there is no per-terminal
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
	const [newOptionsOpen, setNewOptionsOpen] = createSignal(false);
	// Toasts keep clear of this panel's rendered width (see ToastContainer).
	let panelEl!: HTMLDivElement;
	onMount(() => onCleanup(trackPanelWidth(panelEl, (width) => uiStore.setAiChatPanelMeasuredWidth(width))));
	createEffect(() => aiChatDraft.activate(chat.sessionId() ?? ""));
	const keyDown = (event: KeyboardEvent) => {
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
			const resolved = await invoke<{ absolute_path: string; is_directory: boolean } | null>("resolve_terminal_path", {
				cwd,
				candidate: href,
			});
			if (resolved) {
				if (resolved.is_directory) {
					if (isPanelMode() && isTauri()) {
						await emitTo("main", "panel-action", {
							panelId: "ai-chat",
							action: "open-directory",
							data: { path: resolved.absolute_path },
						});
						await invoke("focus_main_window");
					} else {
						uiStore.setFileBrowserExternalRoot(resolved.absolute_path);
						uiStore.setFileBrowserPanelVisible(true);
					}
					return;
				}
				const position = href.match(/:(\d+)(?::(\d+))?$/);
				const line = position ? Number(position[1]) : undefined;
				const col = position?.[2] ? Number(position[2]) : undefined;
				if (isPanelMode() && isTauri()) {
					await emitTo("main", "panel-action", {
						panelId: "ai-chat",
						action: "open-file",
						data: { path: resolved.absolute_path, ...(line ? { line } : {}), ...(col ? { col } : {}) },
					});
					await invoke("focus_main_window");
				} else if (line) openTerminalFilePath(resolved.absolute_path, undefined, line, col);
				else openTerminalFilePath(resolved.absolute_path);
			}
		} catch (error) {
			appLogger.error("ai-chat", "Failed to resolve file link", error);
		}
	};

	const configureEgo = async () => {
		if (isPanelMode() && isTauri()) {
			await emitTo("main", "panel-action", { panelId: "ai-chat", action: "configure-ego", data: {} });
			await invoke("focus_main_window");
		} else props.onOpenSettings?.("general", SETTINGS_SECTION_EGO);
	};

	const emptyMessage = () => {
		switch (chat.phase()) {
			case "unconfigured":
				return "AI Chat is inactive because the ego executable is not configured. Select it in Settings → General, then configure your provider and model in Settings → AI Chat.";
			case "starting":
				return "Connecting conversation…";
			case "failed":
				return "The conversation could not be opened. Use Retry to try again.";
			default:
				if (chat.phase() === "ready" && chat.sessionId()) return "Select a saved tab to load its conversation.";
				return "Ask ego about any repository. The one on screen is sent as context.";
		}
	};

	return (
		<div id="ai-chat-panel" ref={panelEl} class={cx(s.panel, !props.visible && s.hidden)} onKeyDown={keyDown}>
			<PanelResizeHandle panelId="ai-chat-panel" minWidth={300} maxWidth={700} />

			<div class={p.header}>
				<div class={p.headerLeft}>
					<span class={p.title}>
						<svg
							width="14"
							height="14"
							viewBox="0 0 14 14"
							fill="currentColor"
							style={{ "vertical-align": "-2px", "margin-right": "4px" }}
						>
							<path
								d="M2 2.5A1.5 1.5 0 013.5 1h7A1.5 1.5 0 0112 2.5v6A1.5 1.5 0 0110.5 10H5l-3 2.5V10A1.5 1.5 0 010.5 8.5v-6z"
								transform="translate(1 0.5)"
							/>
						</svg>
						AI Chat
					</span>
					<Show when={viewed()}>{(path) => <span class={s.terminalName}>{basename(path())}</span>}</Show>
					<Show when={chat.title()}>{(title) => <span class={s.terminalName}>{title()}</span>}</Show>
					<Show when={chat.launch()}>
						{(launch) => (
							<span class={s.terminalName} title={`Ego: ${launch().executable} · Workspace: ${launch().workspace}`}>
								{launch().profile || "Default profile"} · {launch().workspace} · {launch().executable}
							</span>
						)}
					</Show>
				</div>
				<div class={s.headerActions}>
					<PanelWindowControls
						panelId="ai-chat"
						mode={isPanelMode() ? "detached" : "inline"}
						onInlineClose={props.onClose}
					/>
				</div>
			</div>
			{/* Shown before ego runs too: "+" is one of the two ways a chat starts. */}
			<div class={s.chatTabs} role="tablist" aria-label="AI Chat tabs">
				<For each={chat.tabs()}>
					{(session) => (
						<div class={cx(s.chatTab, chat.sessionId() === session && s.chatTabActive)}>
							<button
								type="button"
								role="tab"
								data-chat-session={session}
								title={session}
								aria-selected={chat.sessionId() === session}
								onClick={() => void chat.selectSession(session)}
							>
								{conversationLabel(
									chat.sessions().find((item) => item.sessionId === session) ?? { sessionId: session },
								)}
							</button>
							<Show when={chat.tabs().length > 1}>
								<button
									type="button"
									aria-label={`Close chat tab ${session}`}
									onClick={() => void chat.closeTab(session)}
								>
									<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
										<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
									</svg>
								</button>
							</Show>
						</div>
					)}
				</For>
				<button
					type="button"
					class={s.newChatTab}
					aria-label="New chat tab"
					title="New chat tab (⌘T)"
					disabled={chat.phase() === "unconfigured"}
					onClick={() => void chat.startSession()}
				>
					<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true">
						<path d="M6.4 1h1.2v5.4H13v1.2H7.6V13H6.4V7.6H1V6.4h5.4z" />
					</svg>
				</button>
				<button
					type="button"
					class={s.newChatTab}
					aria-label="New conversation with options"
					title="New conversation with options"
					onClick={() => {
						chat.clearError();
						setNewOptionsOpen(true);
					}}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
						<path d="M2 3h12v1H2zm0 4h12v1H2zm0 4h12v1H2zM5 1h2v5H5zm4 4h2v5H9zm-5 4h2v5H4z" />
					</svg>
				</button>
			</div>
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
						<button type="button" class={s.retryBtn} onClick={() => void chat.recover()}>
							Recover
						</button>
					</div>
				)}
			</Show>

			<Show when={chat.error()}>
				{(message) => (
					<div class={s.errorBanner}>
						<span class={s.errorText}>{message()}</span>
						<button type="button" class={s.retryBtn} onClick={() => void chat.recover()}>
							Retry
						</button>
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

			<Show when={chat.sessionId()}>
				<SessionControls chat={chat} />
			</Show>

			<Transcript
				canForkAtMessage={() => chat.capabilities()?.forkAtMessage === true}
				onFork={(messageId) => void chat.fork(messageId)}
				entries={chat.entries}
				busy={chat.busy}
				retry={chat.retry}
				emptyMessage={emptyMessage()}
				onSuggestion={(text) => void chat.send(text)}
				onOpenFile={(href) => void openFile(href)}
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
				<div class={s.setupActions}>
					<button
						type="button"
						onClick={() =>
							void configureEgo().catch((error) => appLogger.error("ai-chat", "Failed to open ego settings", error))
						}
					>
						Configure ego
					</button>
				</div>
			</Show>

			<Show when={chat.phase() !== "unconfigured" && chat.phase() !== "starting"}>
				<Composer chat={chat} />
			</Show>
			<Show when={chat.usage()}>
				{(usage) => (
					<div class={s.usageFooter}>
						<span>Context {Math.round((usage().used / usage().size) * 100)}%</span>
						<Show when={usage().cost}>
							{(cost) => (
								<span>
									{cost().currency} {cost().amount}
								</span>
							)}
						</Show>
					</div>
				)}
			</Show>
		</div>
	);
};

export default AIChatPanel;
