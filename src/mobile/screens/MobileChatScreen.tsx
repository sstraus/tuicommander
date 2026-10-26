import { createEffect, createSignal, For, onMount, Show } from "solid-js";
import { Composer } from "../../components/AIChatPanel/Composer";
import { chatErrorView } from "../../components/AIChatPanel/chatError";
import { Interactions } from "../../components/AIChatPanel/Interactions";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { createAcpChat } from "../../components/AIChatPanel/useAcpChat";
import { Button } from "../../components/ui";
import { ContentRenderer } from "../../components/ui/ContentRenderer";
import { Select } from "../../components/ui/Select";
import { acpTranscript } from "../../stores/acpTranscript";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import { settingsStore } from "../../stores/settings";
import type { AcpHostRequestId } from "../../types/acp";
import { writeClipboard } from "../../utils/clipboard";
import { mobileChatReadingStyle } from "../mobileFontSize";
import styles from "./MobileChatScreen.module.css";

export function MobileChatScreen(props: {
	recoveryRevision?: number;
	onOpenFile?: (candidate: string, cwd: string) => void;
}) {
	const linkedRepository = new URLSearchParams(location.search).get("repo");
	const linkedSession = new URLSearchParams(location.search).get("session");
	const [tableDocument, setTableDocument] = createSignal<string | null>(null);
	const [sharedFileError, setSharedFileError] = createSignal<string | null>(null);
	const [linkError, setLinkError] = createSignal<string | null>(null);
	const [sharedFile, setSharedFile] = createSignal<File | null>(null);
	const chat = createAcpChat(
		() => linkedRepository,
		() => true,
	);
	const answering = new Set<AcpHostRequestId>();
	let linkHandled = false;
	const [settingsReady, setSettingsReady] = createSignal(false);
	createEffect(() => {
		props.recoveryRevision;
		if (!settingsReady())
			void settingsStore.hydrate({ notifyOnError: false }).then((loaded) => setSettingsReady(loaded !== false));
	});

	// The chat is global and ego starts on the first message, so a push link
	// selects its conversation at once; sending is what loads it.
	createEffect(() => {
		if (linkHandled || !linkedSession) return;
		if (chat.phase() === "unconfigured" || chat.phase() === "starting") return;
		linkHandled = true;
		if (chat.sessionId() !== linkedSession) void chat.selectSession(linkedSession);
	});

	onMount(() => {
		const sharedKey = new URLSearchParams(location.search).get("shared");
		if (sharedKey && /^[a-zA-Z0-9-]+$/.test(sharedKey)) {
			void (async () => {
				try {
					const cache = await caches.open("tuic-shell-v2");
					const key = `/_shared/${sharedKey}`;
					const response = await cache.match(key);
					if (!response) throw new Error("Shared file is no longer available.");
					const name = response.headers.get("x-file-name") || "shared-file";
					setSharedFile(
						new File([await response.blob()], name, {
							type: response.headers.get("content-type") || "application/octet-stream",
						}),
					);
					await cache.delete(key);
					const nextUrl = new URL(location.href);
					nextUrl.searchParams.delete("shared");
					history.replaceState(null, "", nextUrl);
				} catch (error) {
					setSharedFileError(error instanceof Error ? error.message : "Could not read shared file.");
				}
			})();
		}
	});

	async function answerOnce(requestId: AcpHostRequestId, action: () => Promise<void>): Promise<void> {
		if (answering.has(requestId)) return;
		answering.add(requestId);
		await action();
		if (chat.error()) answering.delete(requestId);
	}
	const openFile = (candidate: string) => {
		const cwd = chat.root();
		if (!cwd || !props.onOpenFile) {
			setLinkError("AI Chat workspace is unavailable right now.");
			return;
		}
		setLinkError(null);
		props.onOpenFile(candidate, cwd);
	};

	async function startNewChat(): Promise<void> {
		await chat.closeFailed();
		await chat.startSession();
	}

	return (
		<section class={styles.screen} aria-label="AI Chat">
			<header class={styles.header}>
				<strong>AI Chat</strong>
			</header>
			<Show when={sharedFileError()}>
				{(message) => (
					<div class={styles.banner} role="alert">
						{message()}
					</div>
				)}
			</Show>
			<Show when={linkError()}>
				{(message) => (
					<div class={styles.banner} role="alert">
						{message()}
					</div>
				)}
			</Show>
			<Show when={chat.gap()}>
				{(gap) => (
					<div class={styles.banner}>
						Missed part of this conversation: {gap().message}{" "}
						<Button variant="ghost" onClick={() => void chat.recover()}>
							Recover
						</Button>
					</div>
				)}
			</Show>
			<Show when={chat.error()}>
				{(message) => {
					const view = () => chatErrorView(message());
					return (
						<div class={styles.banner} role="alert">
							{view().text}{" "}
							<Show when={view().retryable}>
								<Button variant="ghost" onClick={() => void chat.recover()}>
									Retry
								</Button>
							</Show>
							<Show when={view().startNew}>
								<Button variant="ghost" onClick={() => void startNewChat()}>
									Start new chat
								</Button>
							</Show>
						</div>
					);
				}}
			</Show>
			<Show when={chat.phase() === "live" && !chat.isStreaming()}>
				<div class={styles.banner}>Not receiving updates.</div>
			</Show>
			<Show when={chat.phase() === "live"}>
				<div class={styles.controls}>
					<label>
						Conversation
						<Select
							aria-label="Conversation"
							value={chat.sessionId() ?? ""}
							onChange={(event) => void chat.selectSession(event.currentTarget.value)}
						>
							<For each={chat.sessions()}>
								{(session) => (
									<option value={session.sessionId}>
										{acpTranscript.title(session.sessionId) || session.title || session.sessionId}
									</option>
								)}
							</For>
						</Select>
					</label>
					<Button variant="ghost" onClick={() => void chat.startSession()}>
						New
					</Button>
				</div>
			</Show>
			<div class={styles.reading} data-mobile-reading style={mobileChatReadingStyle()}>
				<Transcript
					mobile
					entries={chat.entries}
					muteIntermediate={settingsStore.state.chatMuteIntermediateText}
					busy={chat.busy}
					onOpenFile={openFile}
					observeToolDuration={false}
					onOpenTable={setTableDocument}
					onSuggestion={(text) => void chat.send(text)}
					emptyMessage={
						!settingsReady()
							? "Chat settings are unavailable. Waiting for the server…"
							: chat.phase() === "unconfigured"
								? "Configure ego in desktop Settings to start a conversation."
								: "Ask ego about any repository."
					}
				>
					<Interactions
						interactions={chat.interactions}
						onPermission={(requestId, optionId) =>
							void answerOnce(requestId, () => chat.answerPermission(requestId, optionId))
						}
						onPermissionDismissed={(requestId) => void answerOnce(requestId, () => chat.cancelPermission(requestId))}
						onElicitationAccepted={(requestId, content) =>
							void answerOnce(requestId, () => chat.answerElicitation(requestId, { action: "accept", content }))
						}
						onElicitationDeclined={(requestId) =>
							void answerOnce(requestId, () => chat.answerElicitation(requestId, { action: "decline" }))
						}
						onElicitationCancelled={(requestId) =>
							void answerOnce(requestId, () => chat.answerElicitation(requestId, { action: "cancel" }))
						}
					/>
				</Transcript>
			</div>
			<Show when={chat.phase() !== "unconfigured" && chat.phase() !== "starting"}>
				<Composer
					chat={chat}
					mobileAttachments
					sharedFile={sharedFile()}
					onSharedFileConsumed={() => setSharedFile(null)}
				/>
			</Show>
			<Show when={tableDocument()}>
				{(source) => {
					registerModal(() => setTableDocument(null));
					return (
						<section class={styles.tableDocument} role="dialog" aria-modal="true" aria-label="AI Chat table">
							<header class={styles.header}>
								<strong>AI Chat table</strong>
								<Button variant="ghost" onClick={() => setTableDocument(null)}>
									Back to Chat
								</Button>
							</header>
							<ContentRenderer
								content={source()}
								tableActions={{
									copy: (text) =>
										void writeClipboard(text).catch((error) => appLogger.error("ai-chat", "Copy failed", error)),
								}}
							/>
						</section>
					);
				}}
			</Show>
		</section>
	);
}
