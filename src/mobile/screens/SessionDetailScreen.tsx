import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { AGENT_DISPLAY, type AgentType } from "../../agents";
import { AgentIcon } from "../../components/ui/AgentIcon";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import { CommandInput } from "../components/CommandInput";
import { CommandWidget } from "../components/CommandWidget";
import { IdeasOverlay } from "../components/IdeasOverlay";
import { OutputView } from "../components/OutputView";
import { SessionHeaderOverlay, type SessionHeaderPanel } from "../components/SessionHeaderOverlay";
import { SuggestChips } from "../components/SuggestChips";
import type { ComposerInputKey } from "../components/syncGuards";
import { TerminalKeybar } from "../components/TerminalKeybar";

import { getAgentCommands } from "../config/agentCommands";
import { useMobileVoice } from "../useMobileVoice";
import type { SessionInfo } from "../useSessions";
import { formatRetryCountdown } from "../utils/formatRetryCountdown";
import { isKnownAgentType } from "../utils/sessionKind";
import { useDebouncedStatus } from "../utils/useDebouncedStatus";
import styles from "./SessionDetailScreen.module.css";

interface SessionDetailScreenProps {
	session: SessionInfo;
	sessionExists: boolean;
	onBack: () => void;
	onOpenFiles: () => void;
	onOpenFileLink?: (candidate: string, line?: number) => void;
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

export function SessionDetailScreen(props: SessionDetailScreenProps) {
	// Merge polled state with real-time WebSocket state pushes.
	// WS state arrives instantly; poll state arrives every 3s as fallback.
	const [wsState, setWsState] = createSignal<Record<string, unknown> | null>(null);
	const sessionState = () => {
		const ws = wsState();
		const poll = props.session.state;
		if (!ws) return poll;
		// WS state is authoritative when present; poll state fills gaps on reconnect.
		return { ...poll, ...ws } as typeof poll;
	};
	const status = useDebouncedStatus(() => ({ ...props.session, state: sessionState() }));
	const [moreOpen, setMoreOpen] = createSignal(false);
	const [headerPanel, setHeaderPanel] = createSignal<SessionHeaderPanel | null>(null);
	const agentType = () => sessionState()?.agent_type;
	const voice = useMobileVoice(
		() => props.session.session_id,
		() => !!agentType() && props.sessionExists,
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
			await navigator.clipboard.writeText(props.session.session_id);
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
	createEffect(() => {
		if (!sessionState()?.awaiting_input) setCodexQuestionOpen(false);
	});
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
			await rpc("write_pty", { sessionId: props.session.session_id, data: "\x1b[1;3A" });
			setCodexQuestionOpen(true);
		} catch (err) {
			appLogger.warn("network", "Could not open Codex question", { error: err });
			toastsStore.add("Question not opened", "Could not open the Codex question", "error", true);
		} finally {
			setOpeningQuestion(false);
		}
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
				<button class={styles.backBtn} onClick={props.onBack} aria-label="Back to sessions">
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
				</button>
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
				<Show when={voice.available()}>
					<button
						type="button"
						class={styles.headerAction}
						classList={{ [styles.voiceActive]: voice.armed() }}
						aria-label={voice.armed() ? "Stop voice conversation" : "Start voice conversation"}
						aria-pressed={voice.armed()}
						onClick={() => void voice.toggle()}
					>
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
							<rect x="9" y="3" width="6" height="11" rx="3" />
							<path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
						</svg>
					</button>
				</Show>
				<button
					type="button"
					class={styles.headerAction}
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
				</button>
				<Show when={codexQuestionWaiting()}>
					<button
						type="button"
						class={styles.headerAction}
						onClick={openCodexQuestion}
						disabled={openingQuestion()}
						aria-label="Open Codex question"
						title="Open Codex question"
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
					</button>
				</Show>
				<button
					type="button"
					class={styles.headerAction}
					aria-label="More session actions"
					aria-expanded={moreOpen()}
					onClick={() => setMoreOpen((open) => !open)}
				>
					<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
						<circle cx="12" cy="5" r="1.8" />
						<circle cx="12" cy="12" r="1.8" />
						<circle cx="12" cy="19" r="1.8" />
					</svg>
				</button>
			</header>
			<Show when={voice.available()}>
				<label class={styles.spokenReplies}>
					<span>Spoken replies</span>
					<input
						type="checkbox"
						checked={voice.spokenReplies()}
						onChange={(e) => voice.setSpokenReplies(e.currentTarget.checked)}
					/>
				</label>
			</Show>
			<Show when={moreOpen()}>
				<div class={styles.overflow}>
					<button
						type="button"
						onClick={() => {
							setMoreOpen(false);
							setHeaderPanel("progress");
						}}
					>
						Progress <span>{sessionState()?.progress == null ? "—" : `${sessionState()!.progress}%`}</span>
					</button>
					<button
						type="button"
						onClick={() => {
							setMoreOpen(false);
							props.onOpenFiles();
						}}
					>
						Files
					</button>
					<button
						type="button"
						onClick={() => {
							setMoreOpen(false);
							setSearchOpen(true);
						}}
					>
						Search output
					</button>
					<button
						type="button"
						onClick={() => {
							setMoreOpen(false);
							setIdeasOpen(true);
						}}
					>
						Ideas
					</button>
					<Show when={hasCommands()}>
						<button
							type="button"
							onClick={() => {
								setMoreOpen(false);
								setCommandWidgetOpen(true);
							}}
						>
							Commands
						</button>
					</Show>
					<Show when={sessionState()?.usage_limit_pct != null}>
						<div class={styles.overflowInfo}>Usage {sessionState()!.usage_limit_pct}%</div>
					</Show>
					<button type="button" onClick={() => void copySessionId()}>
						Copy session ID
					</button>
					<button type="button" class={styles.dangerAction} onClick={() => void terminateSession()}>
						Terminate session
					</button>
				</div>
			</Show>

			<Show when={searchOpen()}>
				<div class={styles.searchBar}>
					<input
						class={styles.searchInput}
						type="text"
						placeholder="Filter output..."
						value={searchQuery()}
						onInput={(e) => setSearchQuery(e.currentTarget.value)}
						autofocus
					/>
					<Show when={searchQuery()}>
						<button class={styles.searchClear} onClick={() => setSearchQuery("")}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
								<line x1="18" y1="6" x2="6" y2="18" />
								<line x1="6" y1="6" x2="18" y2="18" />
							</svg>
						</button>
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
				<OutputView
					sessionId={props.session.session_id}
					onOpenFileLink={props.onOpenFileLink}
					onStateChange={setWsState}
					onInputLine={setPtyInputLine}
					searchQuery={searchQuery()}
				/>
				<Show when={!props.sessionExists}>
					<div class={styles.endedOverlay}>
						<span class={styles.endedText}>Session ended</span>
						<button class={styles.endedBackBtn} onClick={props.onBack}>
							Back
						</button>
					</div>
				</Show>
			</div>
			<Show when={sessionState()?.suggested_actions?.length}>
				<SuggestChips
					sessionId={props.session.session_id}
					items={sessionState()!.suggested_actions!}
					agentType={sessionState()?.agent_type as string | null | undefined}
				/>
			</Show>
			<TerminalKeybar
				sessionId={props.session.session_id}
				agentType={sessionState()?.agent_type as string | null | undefined}
				awaitingInput={sessionState()?.awaiting_input}
				choicePromptOpen={!!sessionState()?.choice_prompt}
				questionConfident={sessionState()?.question_confident}
				sessionExists={props.sessionExists}
				onCommandWidgetOpen={() => setCommandWidgetOpen(true)}
				onSlashRequest={() => insertComposerText?.("/")}
				onInputKeyRequest={(key) => requestComposerInputKey?.(key)}
			/>
			<CommandInput
				sessionId={props.session.session_id}
				managedSession={!!sessionState()?.agent_type}
				awaitingInput={sessionState()?.awaiting_input}
				sessionExists={props.sessionExists}
				prefillValue={inputPrefill()}
				ptyInputLine={ptyInputLine()}
				agentType={sessionState()?.agent_type ?? null}
				slashItems={sessionState()?.slash_menu_items}
				choicePrompt={sessionState()?.choice_prompt}
				codexQuestionOpen={codexQuestionOpen()}
				onRegisterInsertText={(fn) => {
					insertComposerText = fn;
				}}
				onRegisterInputKey={(fn) => {
					requestComposerInputKey = fn;
				}}
			/>
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
