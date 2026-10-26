import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { AGENT_DISPLAY, type AgentType } from "../../agents";
import { Button, Input } from "../../components/ui";
import { AgentIcon } from "../../components/ui/AgentIcon";
import { appLogger } from "../../stores/appLogger";
import type { ChatViewWake } from "../../stores/chatView";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import { writeClipboard } from "../../utils/clipboard";
import { CommandInput } from "../components/CommandInput";
import { CommandWidget } from "../components/CommandWidget";
import { IdeasOverlay } from "../components/IdeasOverlay";
import { MobileJumpBottom } from "../components/MobileJumpBottom";
import { createOpenMarkdownTabs, OpenMarkdownMenu } from "../components/OpenMarkdownMenu";
import { OutputView } from "../components/OutputView";
import { SessionHeaderOverlay, type SessionHeaderPanel } from "../components/SessionHeaderOverlay";
import { SessionUsageBlock } from "../components/SessionUsageBlock";
import { SuggestChips } from "../components/SuggestChips";
import type { ComposerInputKey } from "../components/syncGuards";
import { TerminalKeybar } from "../components/TerminalKeybar";
import { useSessionChatView } from "../components/useSessionChatView";

import { getAgentCommands } from "../config/agentCommands";
import { useMobileVoice } from "../useMobileVoice";
import type { SessionInfo, SessionState } from "../useSessions";
import { formatRetryCountdown } from "../utils/formatRetryCountdown";
import { isKnownAgentType } from "../utils/sessionKind";
import { useDebouncedStatus } from "../utils/useDebouncedStatus";
import styles from "./SessionDetailScreen.module.css";

interface SessionDetailScreenProps {
	onConnectionError?: (error: string | null) => void;
	onRegisterRetry?: (retry: (() => void) | undefined) => void;
	session: SessionInfo;
	subscribeChatWake: ChatViewWake;
	sessionExists: boolean;
	onBack: () => void;
	onOpenFiles: () => void;
	onOpenFileLink?: (candidate: string, line?: number, root?: string, openerTuicSession?: string) => void;
}

function projectName(cwd: string | null): string {
	if (!cwd) return "unknown";
	const parts = cwd.replaceAll("\\", "/").split("/");
	return parts.at(-1)! || "unknown";
}

function elapsedTime(ms: number): string {
	const minutes = Math.max(0, Math.floor((Date.now() - ms) / 60_000));
	if (minutes < 1) return "now";
	if (minutes < 60) return `${minutes}m`;
	if (minutes < 1_440) return `${Math.floor(minutes / 60)}h`;
	return `${Math.floor(minutes / 1_440)}d`;
}

/** Both responsive call actions use the same handset, not the dictation mic. */
function VoiceCallIcon() {
	return (
		<svg
			width="20"
			height="20"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			stroke-width="2"
			stroke-linecap="round"
			aria-hidden="true"
		>
			<path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72c.13.96.36 1.9.69 2.79a2 2 0 0 1-.45 2.11L8.09 9.89a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45c.89.33 1.83.56 2.79.69A2 2 0 0 1 22 16.92z" />
		</svg>
	);
}

export function SessionDetailScreen(props: SessionDetailScreenProps) {
	// Both transports send complete snapshots, with cleared optional fields omitted.
	// Replace rather than merge, and let a fresh poll recover a paused/stale socket.
	const [sessionState, setSessionState] = createSignal(props.session.state);
	createEffect(() => {
		const snapshot = props.session.state;
		setSessionState((current) => {
			if (snapshot && current && snapshot.last_activity_ms < current.last_activity_ms) return current;
			return snapshot;
		});
	});
	const status = useDebouncedStatus(() => ({ ...props.session, state: sessionState() }));
	// Native dialogs own their keys; otherwise report suggestions replace generic confirmations.
	const showSuggestions = () => !sessionState()?.choice_prompt && !!sessionState()?.suggested_actions?.length;
	const [moreOpen, setMoreOpen] = createSignal(false);
	const [wideMarkdown, setWideMarkdown] = createSignal(window.innerWidth >= 768);
	const [markdownOpen, setMarkdownOpen] = createSignal(false);
	const markdownTabs = createOpenMarkdownTabs(() => props.session.worktree_path || props.session.cwd);
	const resizeMarkdown = () => {
		const wide = window.innerWidth >= 768;
		if (wide === wideMarkdown()) return;
		setWideMarkdown(wide);
		setMarkdownOpen(false);
		setMoreOpen(false);
	};
	window.addEventListener("resize", resizeMarkdown);
	onCleanup(() => window.removeEventListener("resize", resizeMarkdown));
	const openMarkdown: Parameters<typeof OpenMarkdownMenu>[0]["onOpen"] = (tab) => {
		setMarkdownOpen(false);
		setMoreOpen(false);
		props.onOpenFileLink?.(tab.absolute_path, undefined, tab.fs_root, tab.opener_tuic_session);
	};

	const [headerPanel, setHeaderPanel] = createSignal<SessionHeaderPanel | null>(null);
	const agentType = () => sessionState()?.agent_type;
	const chat = useSessionChatView({
		sessionId: () => props.session.session_id,
		agentType,
		sessionExists: () => props.sessionExists,
		subscribeWake: props.subscribeChatWake,
	});
	const [cliAtBottom, setCliAtBottom] = createSignal(true);
	let jumpCli: (() => void) | undefined;
	const [browserDictating, setBrowserDictating] = createSignal(false);
	const voice = useMobileVoice(
		() => props.session.session_id,
		() => !!agentType() && props.sessionExists && !browserDictating(),
	);
	const statusText = () =>
		({
			idle: "Idle",
			busy: "Working",
			"sub-tasks": "Sub-tasks",
			question: "Input",
			error: "Error",
			"rate-limited": "Rate limited",
			unseen: "Finished",
		})[status()];
	const hasCommands = () => {
		const commands = getAgentCommands(agentType());
		return commands.commands.length > 0 || !!commands.models?.length || !!commands.permissionToggleSeq;
	};

	async function copySessionId() {
		setMoreOpen(false);
		try {
			await writeClipboard(props.session.session_id);
			toastsStore.add(
				"Session ID copied",
				props.session.session_id,
				"info",
				false,
				undefined,
				undefined,
				undefined,
				undefined,
				false,
			);
		} catch (error) {
			toastsStore.add(
				"Could not copy session ID",
				String(error),
				"error",
				false,
				undefined,
				undefined,
				undefined,
				undefined,
				false,
			);
		}
	}

	async function terminateSession() {
		setMoreOpen(false);
		if (!window.confirm("Kill this session?")) return;
		try {
			await rpc("close_pty", { sessionId: props.session.session_id });
		} catch (error) {
			appLogger.warn("network", `Failed to kill session: ${String(error)}`);
			toastsStore.add(
				"Could not terminate session",
				String(error),
				"error",
				false,
				undefined,
				undefined,
				undefined,
				undefined,
				false,
			);
		}
	}

	// Search filter
	const [searchOpen, setSearchOpen] = createSignal(false);
	const [searchQuery, setSearchQuery] = createSignal("");

	// Command widget overlay toggle
	const [commandWidgetOpen, setCommandWidgetOpen] = createSignal(false);

	// Ideas overlay toggle
	const [ideasOpen, setIdeasOpen] = createSignal(false);

	const [openingQuestion, setOpeningQuestion] = createSignal(false);
	const [codexQuestionOpen, setCodexQuestionOpen] = createSignal(false);
	const [egoQuestionOpen, setEgoQuestionOpen] = createSignal(false);
	let visibleEgoQuestion: string | undefined;
	createEffect(() => {
		const state = sessionState();
		const prompt = state?.agent_type === "ego" && state.awaiting_input ? state.choice_prompt : undefined;
		// Highlight changes and identical polling snapshots must not undo a manual
		// close. A new native selector, including one already waiting on entry, opens.
		const signature = prompt
			? JSON.stringify([
					props.session.session_id,
					prompt.title,
					prompt.options.map((option) => [option.key, option.label]),
				])
			: undefined;
		if (signature && signature !== visibleEgoQuestion) setEgoQuestionOpen(true);
		visibleEgoQuestion = signature;
		if (!state?.awaiting_input) {
			setCodexQuestionOpen(false);
			setEgoQuestionOpen(false);
		}
	});
	const egoQuestionWaiting = () =>
		sessionState()?.agent_type === "ego" &&
		sessionState()?.awaiting_input &&
		(!!sessionState()?.choice_prompt || (!!sessionState()?.question_confident && !!sessionState()?.question_text)) &&
		props.sessionExists;
	const codexQuestionWaiting = () =>
		sessionState()?.agent_type === "codex" &&
		sessionState()?.awaiting_input &&
		sessionState()?.question_confident &&
		!sessionState()?.choice_prompt &&
		props.sessionExists;

	async function openCodexQuestion() {
		if (openingQuestion()) return;
		setOpeningQuestion(true);
		try {
			await rpc("write_pty", { sessionId: props.session.session_id, data: "\x1b[1;2D" });
			setCodexQuestionOpen(true);
		} catch (err) {
			appLogger.warn("network", "Could not open Codex question", { error: err });
			toastsStore.add("Question not opened", "Could not open the Codex question", "error", true);
		} finally {
			setOpeningQuestion(false);
		}
	}
	function openQuestion() {
		if (egoQuestionWaiting()) setEgoQuestionOpen((open) => !open);
		else void openCodexQuestion();
	}
	function revealRequiredSelection(sessionId: string) {
		// A refusal may arrive after navigation. Never open another session's card
		// or replay the refused text into a native selector.
		if (sessionId !== props.session.session_id || !props.sessionExists) return;
		if (egoQuestionWaiting()) setEgoQuestionOpen(true);
		else if (codexQuestionWaiting()) void openCodexQuestion();
	}

	// Prefill value for CommandInput (set by slash menu selection).
	const [inputPrefill] = createSignal<{ text: string; seq: number }>({ text: "", seq: 0 });

	// PTY input line synced from WebSocket (what's on the terminal prompt)
	const [ptyInputLine, setPtyInputLine] = createSignal<string | null>(null, { equals: false });

	// Registered by CommandInput so TerminalKeybar can trigger slash mode
	let insertComposerText: ((text: string) => void) | undefined;
	let requestComposerInputKey: ((key: ComposerInputKey) => void) | undefined;

	// Live countdown for rate limit retry_after_ms
	const [retryRemaining, setRetryRemaining] = createSignal(0);

	createEffect(() => {
		const ms = sessionState()?.retry_after_ms;
		if (!ms || !sessionState()?.rate_limited) {
			setRetryRemaining(0);
			return;
		}
		setRetryRemaining(ms);
		const interval = setInterval(() => {
			setRetryRemaining((prev) => {
				const next = prev - 1000;
				if (next <= 0) {
					clearInterval(interval);
					return 0;
				}
				return next;
			});
		}, 1000);
		onCleanup(() => clearInterval(interval));
	});

	return (
		<div class={styles.screen}>
			<header class={styles.header}>
				<Button
					shape="icon"
					variant="ghost"
					class={styles.backBtn}
					onClick={props.onBack}
					aria-label="Back to sessions"
				>
					<svg
						width="20"
						height="20"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						aria-hidden="true"
					>
						<path d="M15 18l-6-6 6-6" />
					</svg>
				</Button>
				<span
					class={styles.logo}
					role="img"
					aria-label={`${agentType() ? agentType()![0].toUpperCase() + agentType()!.slice(1) : "Terminal"} logo`}
					style={{
						color: isKnownAgentType(agentType()) ? AGENT_DISPLAY[agentType() as AgentType].color : "var(--fg-muted)",
					}}
				>
					<Show when={isKnownAgentType(agentType())} fallback={<span aria-hidden="true">›_</span>}>
						<AgentIcon agent={agentType() as AgentType} size={24} />
					</Show>
					<span class={styles.stateDot} data-state={status()} />
				</span>
				<button
					data-control-layout="row"
					type="button"
					class={styles.headerInfo}
					aria-label={props.session.display_name || agentType() || "Terminal"}
					onClick={() => setHeaderPanel("details")}
				>
					<span class={styles.agentName}>{props.session.display_name || agentType() || "Terminal"}</span>
					<span class={styles.project}>
						{projectName(props.session.worktree_path ?? props.session.cwd)}
						<Show when={props.session.worktree_branch}> · {props.session.worktree_branch}</Show> · {statusText()}{" "}
						{sessionState()?.last_activity_ms ? elapsedTime(sessionState()!.last_activity_ms) : ""}
						<Show when={voice.phase()}> · Voice {voice.phase()}</Show>
					</span>
				</button>
				<Show when={wideMarkdown()}>
					<div class={styles.markdownDropdown}>
						<Button
							variant="ghost"
							type="button"
							class={styles.markdownTrigger}
							aria-expanded={markdownOpen()}
							onClick={() => {
								setMoreOpen(false);
								setMarkdownOpen((open) => !open);
							}}
						>
							Markdown
							<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
								<path d="m2 4 4 4 4-4z" />
							</svg>
						</Button>
						<Show when={markdownOpen()}>
							<div class={`${styles.overflow} ${styles.markdownPopover}`}>
								<OpenMarkdownMenu
									cwd={props.session.worktree_path || props.session.cwd}
									source={markdownTabs}
									onOpen={openMarkdown}
								/>
							</div>
						</Show>
					</div>
				</Show>
				<chat.Toggle />
				<Show when={voice.available()}>
					<Button
						shape="icon"
						variant="ghost"
						type="button"
						class={`${styles.headerAction} ${styles.secondaryAction}`}
						classList={{ [styles.voiceActive]: voice.armed() }}
						aria-label={voice.armed() ? "Stop voice call" : "Start voice call"}
						aria-pressed={voice.armed()}
						disabled={browserDictating()}
						onClick={() => void voice.toggle()}
					>
						<VoiceCallIcon />
					</Button>
				</Show>
				<Button
					shape="icon"
					variant="ghost"
					type="button"
					class={`${styles.headerAction} ${styles.secondaryAction}`}
					aria-label={`Session tasks, ${sessionState()?.active_sub_tasks ?? 0} active`}
					onClick={() => setHeaderPanel("tasks")}
				>
					<svg
						width="20"
						height="20"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						aria-hidden="true"
					>
						<path d="M5 6h14M5 12h14M5 18h14" />
					</svg>
					<Show when={(sessionState()?.active_sub_tasks ?? 0) > 0}>
						<span class={styles.taskCount}>{sessionState()!.active_sub_tasks}</span>
					</Show>
				</Button>
				<Show when={codexQuestionWaiting() || egoQuestionWaiting()}>
					<Button
						shape="icon"
						variant="ghost"
						type="button"
						class={styles.headerAction}
						onClick={openQuestion}
						disabled={openingQuestion()}
						aria-label={egoQuestionWaiting() ? "Open ego question" : "Open Codex question"}
						title={egoQuestionWaiting() ? "Open ego question" : "Open Codex question"}
						aria-expanded={egoQuestionWaiting() ? egoQuestionOpen() : codexQuestionOpen()}
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
							<circle cx="12" cy="12" r="9" />
							<path d="M9.5 9a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4" />
							<circle cx="12" cy="17" r=".6" fill="currentColor" stroke="none" />
						</svg>
					</Button>
				</Show>
				<Button
					shape="icon"
					variant="ghost"
					type="button"
					class={styles.headerAction}
					aria-label="More session actions"
					aria-expanded={moreOpen()}
					onClick={() => {
						setMarkdownOpen(false);
						setMoreOpen((open) => !open);
					}}
				>
					<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
						<circle cx="12" cy="5" r="1.8" />
						<circle cx="12" cy="12" r="1.8" />
						<circle cx="12" cy="19" r="1.8" />
					</svg>
				</Button>
			</header>
			<Show when={moreOpen()}>
				<div class={styles.overflow}>
					<Show when={voice.available()}>
						<Button
							variant="ghost"
							type="button"
							class={styles.phoneAction}
							aria-pressed={voice.armed()}
							disabled={browserDictating()}
							onClick={() => {
								setMoreOpen(false);
								void voice.toggle();
							}}
						>
							<VoiceCallIcon />
							{voice.armed() ? "Stop voice call" : "Start voice call"}
						</Button>
					</Show>
					<Button
						variant="ghost"
						type="button"
						class={styles.phoneAction}
						aria-label={`Tasks, ${sessionState()?.active_sub_tasks ?? 0} active`}
						onClick={() => {
							setMoreOpen(false);
							setHeaderPanel("tasks");
						}}
					>
						Tasks <span>{sessionState()?.active_sub_tasks ?? 0}</span>
					</Button>
					<Button
						variant="ghost"
						type="button"
						onClick={() => {
							setMoreOpen(false);
							setHeaderPanel("progress");
						}}
					>
						Progress <span>{sessionState()?.progress == null ? "—" : `${sessionState()!.progress}%`}</span>
					</Button>
					<Button
						variant="ghost"
						type="button"
						onClick={() => {
							setMoreOpen(false);
							props.onOpenFiles();
						}}
					>
						Files
					</Button>
					<Button
						variant="ghost"
						type="button"
						onClick={() => {
							setMoreOpen(false);
							setSearchOpen(true);
						}}
					>
						Search output
					</Button>
					<Button
						variant="ghost"
						type="button"
						onClick={() => {
							setMoreOpen(false);
							setIdeasOpen(true);
						}}
					>
						Ideas
					</Button>
					<Show when={!wideMarkdown()}>
						<OpenMarkdownMenu
							cwd={props.session.worktree_path || props.session.cwd}
							source={markdownTabs}
							onOpen={openMarkdown}
						/>
					</Show>
					<Show when={hasCommands()}>
						<Button
							variant="ghost"
							type="button"
							onClick={() => {
								setMoreOpen(false);
								setCommandWidgetOpen(true);
							}}
						>
							Commands
						</Button>
					</Show>
					<SessionUsageBlock sessionId={props.session.session_id} />
					<Button variant="ghost" type="button" onClick={() => void copySessionId()}>
						Copy session ID
					</Button>
					<Button variant="ghost" type="button" class={styles.dangerAction} onClick={() => void terminateSession()}>
						Terminate session
					</Button>
				</div>
			</Show>

			<Show when={searchOpen()}>
				<div class={styles.searchBar}>
					<Input
						class={styles.searchInput}
						type="text"
						placeholder="Filter output..."
						value={searchQuery()}
						onInput={(e) => setSearchQuery(e.currentTarget.value)}
						autofocus
					/>
					<Show when={searchQuery()}>
						<Button
							shape="icon"
							aria-label="Clear output filter"
							variant="ghost"
							class={styles.searchClear}
							onClick={() => setSearchQuery("")}
						>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
								<line x1="18" y1="6" x2="6" y2="18" />
								<line x1="6" y1="6" x2="18" y2="18" />
							</svg>
						</Button>
					</Show>
				</div>
			</Show>

			<Show when={sessionState()?.last_error}>
				<div class={styles.errorBar}>{sessionState()!.last_error}</div>
			</Show>

			<Show when={sessionState()?.rate_limited}>
				<div class={styles.rateLimitBar}>
					<span>Rate limited</span>
					<Show when={retryRemaining() > 0}>
						<span class={styles.rateLimitCountdown}>{formatRetryCountdown(retryRemaining())}</span>
					</Show>
				</div>
			</Show>

			<div class={styles.outputArea}>
				<chat.View />
				<OutputView
					sessionExists={props.sessionExists}
					onConnectionError={props.onConnectionError}
					onRegisterRetry={props.onRegisterRetry}
					visible={!chat.active()}
					onAtBottomChange={setCliAtBottom}
					onRegisterScrollToBottom={(fn) => {
						jumpCli = fn;
					}}
					sessionId={props.session.session_id}
					onOpenFileLink={props.onOpenFileLink}
					onStateChange={(state) => setSessionState(state as unknown as SessionState)}
					onInputLine={setPtyInputLine}
					searchQuery={searchQuery()}
				/>
				<Show when={!props.sessionExists}>
					<div class={styles.endedOverlay}>
						<span class={styles.endedText}>Session ended</span>
						<Button class={styles.endedBackBtn} onClick={props.onBack}>
							Back
						</Button>
					</div>
				</Show>
				<MobileJumpBottom
					visible={chat.active() ? !chat.atBottom() : !cliAtBottom()}
					onScrollToBottom={() => (chat.active() ? chat.scrollToBottom() : jumpCli?.())}
				/>
			</div>
			<Show when={showSuggestions()}>
				<SuggestChips
					sessionId={props.session.session_id}
					items={sessionState()!.suggested_actions!}
					agentType={sessionState()?.agent_type as string | null | undefined}
				/>
			</Show>
			<div class={styles.controls}>
				<TerminalKeybar
					sessionId={props.session.session_id}
					agentType={sessionState()?.agent_type as string | null | undefined}
					awaitingInput={sessionState()?.awaiting_input && !showSuggestions()}
					choicePromptOpen={!!sessionState()?.choice_prompt}
					questionConfident={sessionState()?.question_confident}
					sessionExists={props.sessionExists}
					onCommandWidgetOpen={() => setCommandWidgetOpen(true)}
					onSlashRequest={() => insertComposerText?.("/")}
					onInputKeyRequest={(key) => requestComposerInputKey?.(key)}
				/>
				<CommandInput
					serverVoiceRecording={voice.recording()}
					onBrowserDictating={setBrowserDictating}
					sessionId={props.session.session_id}
					managedSession={!!sessionState()?.agent_type}
					awaitingInput={sessionState()?.awaiting_input}
					sessionExists={props.sessionExists}
					prefillValue={inputPrefill()}
					ptyInputLine={ptyInputLine()}
					agentType={sessionState()?.agent_type ?? null}
					slashItems={sessionState()?.slash_menu_items}
					choicePrompt={agentType() === "ego" && !egoQuestionOpen() ? undefined : sessionState()?.choice_prompt}
					questionText={egoQuestionOpen() && !sessionState()?.choice_prompt ? sessionState()?.question_text : undefined}
					codexQuestionOpen={codexQuestionOpen()}
					onSelectionRequired={revealRequiredSelection}
					onRegisterInsertText={(fn) => {
						insertComposerText = fn;
					}}
					onRegisterInputKey={(fn) => {
						requestComposerInputKey = fn;
					}}
				/>
			</div>
			<Show when={commandWidgetOpen()}>
				<CommandWidget
					sessionId={props.session.session_id}
					agentType={sessionState()?.agent_type as string | null | undefined}
					onDismiss={() => setCommandWidgetOpen(false)}
				/>
			</Show>
			<Show when={ideasOpen()}>
				<IdeasOverlay
					sessionId={props.session.session_id}
					repoPath={props.session.cwd}
					microphoneBlocked={voice.recording() || browserDictating()}
					onDismiss={() => setIdeasOpen(false)}
				/>
			</Show>
			<Show when={headerPanel()}>
				{(panel) => (
					<SessionHeaderOverlay
						mode={panel()}
						session={props.session}
						state={sessionState()}
						onClose={() => setHeaderPanel(null)}
					/>
				)}
			</Show>
		</div>
	);
}
