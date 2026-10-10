import {
	type Component,
	createEffect,
	createSignal,
	lazy,
	on,
	onCleanup,
	onMount,
	Show,
	Suspense,
	untrack,
} from "solid-js";
import { applySessionStateEvent, detectAgentForTerminal } from "../../hooks/useAgentPolling";
import { browserCreatedSessions } from "../../hooks/useAppInit";
import { usePty } from "../../hooks/usePty";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { agentConfigsForRepo, ensureAgentConfigsForRepo } from "../../stores/agentConfigs";
import { appLogger } from "../../stores/appLogger";
import { notificationsStore } from "../../stores/notifications";
import { paneLayoutStore } from "../../stores/paneLayout";
import { rateLimitStore } from "../../stores/ratelimit";
import { settingsStore } from "../../stores/settings";
import { type AwaitingInputType, isShellState, terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { HttpRpcError, isTauri, subscribePty, type Unsubscribe } from "../../transport";
import { getSessionConnection } from "../../transportRuntime";
import { onClickKeyDown } from "../../utils/a11y";
import { writeClipboard } from "../../utils/clipboard";
import { keyFor } from "../../utils/hotkey";
import { isPerfDebug } from "../../utils/perfDebug";
import { safeUnlisten } from "../../utils/safeUnlisten";
import { isSuspendingOrSuspended, resumeTerminal } from "../../utils/suspendTerminal";
import type { TranscriptSearchRef } from "../AIChatPanel/Transcript";
import { createSearchVisibility } from "../shared/SearchBar";
import { handleAgentExitCompletion } from "./agentExitCompletion";
import { getAwaitingInputSound } from "./awaitingInputSound";
import CanvasTerminal, { type CanvasTerminalRef } from "./CanvasTerminal";
import { gridDimsForBox, snapLineHeight } from "./canvasTerminalUtils";
import { submitCompose } from "./composeSubmit";
import { focusIsInsideOwnInput } from "./focusGuards";
import { getSharedMetrics } from "./glyphCache";
import { handleIntentEvent, shouldApplyOscTitle } from "./intentTitle";
import { LastPromptBar } from "./LastPromptBar";
import { trackQuestionReminder } from "./questionReminder";
import s from "./Terminal.module.css";
import { ChatViewFallbackBanner, TerminalChatView, ViewModeToggle } from "./TerminalChatView";
import { TerminalSearch } from "./TerminalSearch";
import {
	REATTACH_PHASE_INITIAL,
	type ReattachPhase,
	retryUntilSized,
	SIZE_RETRY_MAX_FRAMES,
	stepReattachPhase,
} from "./visibilityLifecycle";

const ComposePanel = lazy(() =>
	import("../ComposePanel/ComposePanel").then((module) => ({ default: module.ComposePanel })),
);

/** Trim trailing whitespace from each line of a terminal selection. */
export function trimSelection(text: string): string {
	return text
		.split("\n")
		.map((line) => line.trimEnd())
		.join("\n");
}

/** Structured events parsed by Rust OutputParser, received via pty-parsed-{sessionId} */
type ParsedEvent =
	| { type: "rate-limit"; pattern_name: string; matched_text: string; retry_after_ms: number | null }
	| { type: "status-line"; task_name: string; full_line: string; time_info: string | null; token_info: string | null }
	| { type: "progress"; state: number; value: number }
	| { type: "question"; prompt_text: string; confident: boolean }
	| { type: "question-cleared" }
	| { type: "choice-cleared" }
	| { type: "usage-limit"; percentage: number; limit_type: string }
	| { type: "usage-exhausted"; reset_time: string | null }
	| { type: "plan-file"; path: string }
	| { type: "user-input"; content: string; line: number }
	| { type: "api-error"; pattern_name: string; matched_text: string; error_kind: string }
	| { type: "tool-error"; matched_text: string }
	| { type: "intent"; text: string; title?: string }
	| { type: "suggest"; items: string[] }
	| { type: "slash-menu"; items: Array<{ command: string; description: string; highlighted: boolean }> }
	| {
			type: "choice-prompt";
			title: string;
			options: Array<{ key: string; label: string; highlighted: boolean; destructive: boolean; hint?: string }>;
			dismiss_key?: string;
			amend_key?: string;
	  }
	| { type: "active-subtasks"; count: number; task_type: string }
	| { type: "shell-state"; state: "busy" | "idle" }
	| { type: "agent-session-conflict"; matched_text: string; kind: "in-use" | "not-found" }
	| { type: "agent-block"; action: "start" | "end"; line: number; exit_code?: number };

export interface TerminalProps {
	id: string;
	cwd?: string | null;
	onFocus?: (id: string) => void;
	onSessionCreated?: (id: string, sessionId: string) => void;
	onSessionExit?: (id: string) => void;
	onRateLimit?: (id: string, sessionId: string, retryAfterMs: number | null) => void;
	/** Called when a file path is clicked in terminal output */
	onOpenFilePath?: (absolutePath: string, line?: number, col?: number) => void;
	/** When false, disables left-Option-as-Meta key sequences (macOS only). Default: true */
	metaHotkeys?: boolean;
	/** When true, terminal initializes immediately without requiring activeId match */
	alwaysVisible?: boolean;
	/** Called when the shell reports a working directory change via OSC 7 */
	onCwdChange?: (id: string, cwd: string) => void;
}

/** Strip common shell prompt patterns from a raw terminal line. */
function stripPrompt(line: string): string {
	return line.replace(/^.*[$%#❯→>]\s/, "").trimEnd();
}

/** Shell control flow pattern — titles containing these are cryptic scripts, not useful names */
const SHELL_SCRIPT_RE =
	/;|&&|\|\||\$\(|\bif\b|\bthen\b|\belse\b|\belif\b|\bfi\b|\bfor\b|\bwhile\b|\bdo\b|\bdone\b|\bcase\b|\besac\b/;

/** Clean an OSC 0/2 title: strip user@host prefix, env var assignments, and command args.
 *  Returns empty string if the title is only a user@host pattern (no useful info),
 *  or if it looks like a shell script (compound commands, control flow). */
export function cleanOscTitle(title: string): string {
	// Reject titles that look like shell scripts before any processing
	if (SHELL_SCRIPT_RE.test(title)) return "";

	// Strip leading spinner/symbol noise: *, middle dots, bullets, braille patterns,
	// dingbats, geometric shapes, and other non-alphanumeric decorators agents prepend.
	// The dingbat range starts at U+2713 (\u2713\u2714\u2715\u2716\u2717\u2718) rather than U+2720 so completion and
	// error indicators are stripped too \u2014 pi ends a turn with "\u2713 | \u03C0 | repo" and a failure
	// with "\u2717 | \u03C0 | repo", which would otherwise leak a status glyph into the tab name.
	let cleaned = title.replace(
		/^[\s*\u00B7\u2022\u2219\u22C5\u2027\u25A0-\u25FF\u2800-\u28FF\u2713-\u273F\u2580-\u259F]+/,
		"",
	);
	// Then drop the separator the indicator was attached to, so a status-prefixed title
	// (pi emits "\u2826 | \u03C0 | repo", "\u25CB | \u03C0 | repo", "\u2713 | \u03C0 | repo") does not leave a dangling
	// "| " once the animated glyph is gone.
	cleaned = cleaned.replace(/^[|\u2502\u00B7\-\u2013\u2014:]+\s*/, "");
	// Strip "user@host:" or bare "user@host" prefix
	cleaned = cleaned.replace(/^[^@\s]+@[^:\s]+(:\s*)?/, "");
	// Strip leading env var assignments (KEY=value pairs, including empty values)
	cleaned = cleaned.replace(/^(\s*\w+=\S*\s+)+/, "");
	cleaned = cleaned.trim();
	// Paths: shell is just reporting CWD (idle prompt) — not useful as a tab title
	// since the status bar already shows the full path. Return empty to keep original name.
	if (/^(\/|~|[A-Za-z]:[\\/]|\\\\)/.test(cleaned)) {
		return "";
	}
	// Strip flags and their values, keep command + subcommands (bare words before first flag)
	if (cleaned) {
		const words = cleaned.split(/\s+/);
		const kept: string[] = [];
		for (const w of words) {
			if (w.startsWith("-")) break;
			kept.push(w);
		}
		cleaned = kept.join(" ");
	}
	return cleaned;
}

/** Get initial terminal dimensions from container size + font metrics.
 *
 *  Mirrors CanvasTerminal's remeasure EXACTLY — shared `gridDimsForBox` formula
 *  (gutter + scrollbar subtracted) and the per-terminal font-size override — so
 *  the reconnect-path resize lands on the same dims the canvas will compute and
 *  the backend no-ops it instead of firing a spurious SIGWINCH (each SIGWINCH
 *  makes Ink TUIs clear+reprint their full frame into scrollback). */
function calcGridSize(container: HTMLElement, terminalId: string): { rows: number; cols: number } {
	const fontSize = terminalsStore.state.terminals[terminalId]?.fontSize ?? settingsStore.state.defaultFontSize;
	const fontFamily = settingsStore.getFontFamily();
	const fontWeight = settingsStore.state.fontWeight;
	const dpr = window.devicePixelRatio || 1;
	const m = getSharedMetrics(fontSize, fontFamily, dpr, snapLineHeight(fontSize), fontWeight);
	const rect = container.getBoundingClientRect();
	const d = gridDimsForBox(rect.width, rect.height, m.cellWidth, m.cellHeight);
	return { rows: Math.max(2, d.rows), cols: Math.max(2, d.cols) };
}

export const Terminal: Component<TerminalProps> = (props) => {
	let containerRef: HTMLDivElement | undefined;
	let sessionId: string | null = null;
	const [_currentSessionId, setCurrentSessionId] = createSignal<string | null>(null);
	// Without a session there is nothing to read, so the grid (and its exit notice) stays on screen.
	const chatActive = () => terminalsStore.get(props.id)?.viewMode === "chat" && _currentSessionId() !== null;
	const [spawnError, setSpawnError] = createSignal<string | null>(null);

	const [canvasTerminalRef, setCanvasTerminalRef] = createSignal<CanvasTerminalRef | undefined>();
	let pendingCanvasFocus = false;
	let chatSearchRef: TranscriptSearchRef | undefined;

	const {
		visible: searchVisible,
		focusToken: searchFocusToken,
		open: openSearchBar,
		close: closeSearchBar,
	} = createSearchVisibility();
	const [composeOpen, setComposeOpen] = createSignal(false);
	const [pendingComposeText, setPendingComposeText] = createSignal("");
	// Per tab, session-only: a pinned composer replaces the agent's own input box
	// for this terminal, so send and Esc leave it open.
	const [composePinned, setComposePinned] = createSignal(false);
	const [composeFocusRequest, setComposeFocusRequest] = createSignal(0);
	const [composeTextRequest, setComposeTextRequest] = createSignal(0);
	// Chat owns its input slot without changing this tab's CLI Compose preferences.
	const composeVisible = () => chatActive() || composeOpen();
	const composeDocked = () => chatActive() || composePinned();
	createEffect(
		on(chatActive, (active) => {
			closeSearchBar();
			chatSearchRef?.close();
			if (active) setComposeFocusRequest((n) => n + 1);
		}),
	);
	/** After a send, an unpinned composer is done; a pinned one keeps the caret. */
	const finishCompose = (remainingDraft: string) => {
		setPendingComposeText(remainingDraft);
		// A different Compose instance may already be showing the same draft.
		setComposeTextRequest((n) => n + 1);
		if (composeDocked()) return;
		setComposeOpen(false);
		canvasTerminalRef()?.focus();
	};
	const [reconnecting, setReconnecting] = createSignal<{ attempt: number; max: number } | null>(null);
	let sessionInitialized = false;
	let disposed = false;
	let unsubscribePty: Unsubscribe | undefined;
	// Tauri's `listen()` resolves to `async () => _unlisten(...)`: calling these
	// returns a PROMISE that rejects when the listener is already gone. Typed as
	// such so every teardown goes through `safeUnlisten` instead of a
	// synchronous try/catch that cannot see the rejection.
	let unlistenParsed: (() => unknown) | undefined;
	let unlistenKitty: (() => unknown) | undefined;
	let unlistenClipboardStore: (() => unknown) | undefined;

	let kittyFlags = 0;

	/** Stop listening to the current session; the three teardowns (reconnect failure,
	 *  suspend, unmount) share it. */
	const detachSessionListeners = () => {
		safeUnlisten(unsubscribePty);
		unsubscribePty = undefined;
		safeUnlisten(unlistenParsed);
		unlistenParsed = undefined;
		safeUnlisten(unlistenKitty);
		unlistenKitty = undefined;
		safeUnlisten(unlistenClipboardStore);
		unlistenClipboardStore = undefined;
	};

	const RETRY_DELAYS = [5_000, 15_000, 30_000];
	let retryCount = 0;
	let retryTimer: ReturnType<typeof setTimeout> | undefined;
	let agentDetectTimer: ReturnType<typeof setTimeout> | undefined;
	let rafHandle = 0;
	/** Disposer for the bounded zero-size retry, so it cannot outlive the effect run. */
	let cancelSizeRetry: (() => void) | null = null;

	let activityFlagged = false;
	const mountedAt = performance.now();
	let lastDataAtTimestamp = 0;
	let planFileNotified = false;

	createEffect(() => {
		if (terminalsStore.state.activeId === props.id) {
			activityFlagged = false;
		}
	});

	// Edge-detection for notification sounds: play once per awaitingInput transition.
	// Event handlers set state idempotently; this effect handles the one-shot sound.
	// Low-confidence questions are debounced to avoid phantom notifications when the
	// user is already typing (awaitingInput set then cleared within milliseconds).
	let prevAwaitingInput: AwaitingInputType = null;
	let questionDebounceTimer = 0;
	createEffect(() => {
		const term = terminalsStore.get(props.id);
		const current = term?.awaitingInput ?? null;
		const confident = term?.awaitingInputConfident ?? false;
		const sound = getAwaitingInputSound(prevAwaitingInput, current);
		prevAwaitingInput = current;

		// Clear any pending debounced question notification on state change
		if (questionDebounceTimer) {
			clearTimeout(questionDebounceTimer);
			questionDebounceTimer = 0;
		}

		if (sound === "error") {
			appLogger.info("terminal", `[Notify] ${props.id} error — awaitingInput transition`);
			notificationsStore.playError(props.id);
		} else if (sound === "question") {
			if (confident) {
				appLogger.info("terminal", `[Notify] ${props.id} question — awaitingInput transition`);
				notificationsStore.playQuestion(props.id);
			} else {
				// Debounce low-confidence questions: if cleared within 500ms, skip notification
				questionDebounceTimer = setTimeout(() => {
					questionDebounceTimer = 0;
					if (terminalsStore.get(props.id)?.awaitingInput === "question") {
						appLogger.info("terminal", `[Notify] ${props.id} question — awaitingInput transition (debounced)`);
						notificationsStore.playQuestion(props.id);
					}
				}, 500) as unknown as number;
			}
		}
	});

	trackQuestionReminder(props.id);

	// Original tab name before any OSC title overwrote it
	let originalName: string | null = null;

	const pty = usePty();

	// Launch readiness can arrive before or after command preparation. State
	// snapshots also carry idle when the first prompt predates subscription.
	createEffect(() => {
		const terminal = terminalsStore.get(props.id);
		const targetSessionId = terminal?.sessionId;
		const command = terminal?.pendingInitCommand;
		if (disposed || !targetSessionId || !command || terminal.shellState !== "idle") return;
		untrack(() => {
			terminalsStore.update(props.id, { pendingInitCommand: null });
			pty
				.sendCommand(targetSessionId, command, null)
				.catch((error) => appLogger.error("terminal", "Failed to write init command", { error: String(error) }));
		});
	});

	/** Track PTY activity for the activity dashboard. CanvasTerminal handles
	 *  rendering and plugin dispatch; this callback only updates store metadata.
	 *
	 *  Driven by the backend's payload-free activity pulse, not by output bytes:
	 *  it never needed the data, and on desktop no output crosses IPC at all.
	 *  Deriving this from grid frames instead would not work — CanvasTerminal
	 *  stops acking frames while a terminal is hidden (the IntersectionObserver
	 *  flow control), which is exactly the background-tab case the unread flag
	 *  exists to report. */
	const handlePtyActivity = () => {
		if (disposed) return;
		const now = Date.now();
		// The producer already throttles to ~1/s; this guard keeps the store
		// write bounded regardless of what that window is set to.
		if (!lastDataAtTimestamp || now - lastDataAtTimestamp > 1000) {
			lastDataAtTimestamp = now;
			terminalsStore.touchLastDataAt(props.id, now);
		}
		if (terminalsStore.state.activeId !== props.id && !activityFlagged) {
			activityFlagged = true;
			terminalsStore.update(props.id, { activity: true });
		}
	};

	const handlePtyTitle = (title: string) => {
		if (disposed) return;
		const term = terminalsStore.get(props.id);
		if (
			!term ||
			!shouldApplyOscTitle({
				nameIsCustom: term.nameIsCustom,
				nameFromSpawn: term.nameFromSpawn,
				agentIntent: term.agentIntent,
				intentTabTitle: settingsStore.state.intentTabTitle,
			})
		)
			return;
		if (!title) {
			if (originalName) terminalsStore.update(props.id, { name: originalName });
		} else {
			const cleaned = cleanOscTitle(title);
			if (cleaned) {
				if (!originalName) originalName = terminalsStore.get(props.id)?.name || null;
				terminalsStore.update(props.id, { name: cleaned });
			} else if (originalName) {
				terminalsStore.update(props.id, { name: originalName });
			}
		}
	};

	/** Set up event listeners for a known session ID */
	const attachSessionListeners = async (targetSessionId: string) => {
		// Shared handler for structured events from Rust OutputParser.
		// Used by both Tauri (listen) and browser (WebSocket onParsed) modes.
		const handleParsedEvent = (parsed: ParsedEvent) => {
			if (disposed) return;
			switch (parsed.type) {
				case "progress": {
					if (parsed.state === 0) {
						terminalsStore.update(props.id, { progress: null });
					} else if (parsed.state === 1 || parsed.state === 2 || parsed.state === 3) {
						terminalsStore.update(props.id, { progress: Math.min(100, Math.max(0, parsed.value)) });
					}
					break;
				}
				case "status-line": {
					retryCount = 0;
					terminalsStore.update(props.id, {
						currentTask: parsed.task_name,
					});
					break;
				}
				case "active-subtasks": {
					terminalsStore.update(props.id, { activeSubTasks: parsed.count });
					break;
				}
				case "rate-limit": {
					const terminal = terminalsStore.get(props.id);
					const detectedAgent = terminal?.agentType;
					appLogger.debug(
						"terminal",
						`[RateLimit] pattern=${parsed.pattern_name} matched="${parsed.matched_text}" agent=${detectedAgent ?? "none"} shellState=${terminal?.shellState} sessionId=${targetSessionId}`,
					);
					if (terminal?.shellState === "busy") {
						appLogger.debug(
							"terminal",
							`[RateLimit] IGNORED (shellState=busy, likely false positive) pattern=${parsed.pattern_name}`,
						);
						break;
					}
					const existing = rateLimitStore.getRateLimitInfo(targetSessionId);
					const recentlyDetected = existing && Date.now() - existing.detectedAt < 5000;
					if (detectedAgent && !recentlyDetected) {
						const info = {
							agentType: detectedAgent,
							sessionId: targetSessionId,
							retryAfterMs: parsed.retry_after_ms,
							message: `Rate limit detected (${parsed.pattern_name}): ${parsed.matched_text}`,
							detectedAt: Date.now(),
						};
						rateLimitStore.addRateLimit(info);
						props.onRateLimit?.(props.id, targetSessionId, parsed.retry_after_ms);
						appLogger.info(
							"terminal",
							`[Notify] ${props.id} warning — rate-limit pattern=${parsed.pattern_name} matched="${parsed.matched_text}"`,
						);
						notificationsStore.playWarning();
					}
					break;
				}
				case "question": {
					// One-shot effects/plugins consume parsed events; the backend
					// SessionState snapshot is the only durable awaiting authority.
					break;
				}
				case "question-cleared": {
					// Snapshot reconciliation clears the durable badge.
					break;
				}
				case "usage-limit": {
					const current = terminalsStore.get(props.id)?.usageLimit;
					if (current?.percentage !== parsed.percentage || current?.limitType !== parsed.limit_type) {
						terminalsStore.update(props.id, {
							usageLimit: { percentage: parsed.percentage, limitType: parsed.limit_type },
						});
					}
					break;
				}
				case "usage-exhausted": {
					appLogger.warn("terminal", `[UsageExhausted] ${props.id} reset_time=${parsed.reset_time ?? "unknown"}`);
					terminalsStore.setAwaitingInput(props.id, "error");
					break;
				}
				case "plan-file":
					if (terminalsStore.state.activeId !== props.id && !planFileNotified) {
						planFileNotified = true;
						appLogger.info("terminal", `[Notify] ${props.id} info — plan-file path="${parsed.path}" (background tab)`);
						notificationsStore.playInfo();
					}
					break;
				case "user-input":
					planFileNotified = false;
					// Record the prompt row for the green scrollbar marker. line < 0 (the
					// keystroke-reconstructed path) is ignored by the store.
					terminalsStore.addUserPromptLine(props.id, parsed.line);
					// Clear suggest bar, pending buffer, and mark dismissed
					terminalsStore.update(props.id, { suggestDismissed: true, suggestedActions: null, activeSubTasks: 0 });
					// User resumed typing — clear any stale error/question badge. See comment
					// in stores/terminals.ts handleShellStateChange: error state should be
					// cleared on explicit agent activity, user-input, or process exit.
					terminalsStore.clearAwaitingInput(props.id);
					invoke<string | null>("get_last_prompt", { sessionId: targetSessionId })
						.then((prompt) => {
							if (prompt !== null) terminalsStore.setLastPrompt(props.id, prompt);
						})
						.catch((err) => {
							appLogger.debug("terminal", "get_last_prompt failed", { sessionId: targetSessionId, error: String(err) });
						});
					break;
				case "api-error": {
					const agent = terminalsStore.get(props.id)?.agentType;
					const { error_kind: kind, pattern_name: patternName, matched_text: matchedText } = parsed;
					appLogger.debug(
						"terminal",
						`[ApiError] ${props.id} pattern=${patternName} kind=${kind} agent=${agent ?? "none"} matched="${matchedText}"`,
					);
					const label = agent ?? "Agent";
					const kindLabel = kind === "server" ? "server error" : kind === "auth" ? "auth failure" : "API error";
					appLogger.error("terminal", `${label}: ${kindLabel} (${patternName})`);

					const willAutoRetry =
						kind === "server" &&
						agent &&
						agentConfigsForRepo(props.cwd).isAutoRetryEnabled(agent) &&
						!retryTimer &&
						retryCount < RETRY_DELAYS.length;

					if (willAutoRetry) {
						const delay = RETRY_DELAYS[retryCount];
						const attempt = retryCount + 1;
						appLogger.info(
							"terminal",
							`[AutoRetry] ${label}: attempt ${attempt}/${RETRY_DELAYS.length} in ${delay / 1000}s`,
						);
						retryTimer = setTimeout(() => {
							retryTimer = undefined;
							const current = terminalsStore.get(props.id);
							if (sessionId && current?.shellState !== "busy") {
								appLogger.info("terminal", `[AutoRetry] ${label}: injecting "continue" (attempt ${attempt})`);
								// Route through sendCommand (never raw text+\r): Ink-based agents in
								// raw mode drop the Enter when bundled with text, defeating the retry.
								pty
									.sendCommand(sessionId, "continue", current?.agentType)
									.catch((err) => appLogger.error("terminal", "[AutoRetry] Failed to write", { error: String(err) }));
							}
						}, delay);
						retryCount++;
					} else {
						terminalsStore.setAwaitingInput(props.id, "error");
						if (
							kind === "server" &&
							agent &&
							agentConfigsForRepo(props.cwd).isAutoRetryEnabled(agent) &&
							retryCount >= RETRY_DELAYS.length
						) {
							appLogger.warn(
								"terminal",
								`[AutoRetry] ${label}: exhausted ${RETRY_DELAYS.length} retries, manual intervention needed`,
							);
						}
					}
					break;
				}
				case "tool-error":
					terminalsStore.setAwaitingInput(props.id, "error");
					break;
				case "agent-session-conflict": {
					const oldUuid = terminalsStore.get(props.id)?.tuicSession;
					const newUuid = crypto.randomUUID();
					terminalsStore.update(props.id, { tuicSession: newUuid });
					appLogger.warn(
						"terminal",
						`[AgentSessionConflict] ${props.id} kind=${parsed.kind} — tuicSession regenerated ${oldUuid} → ${newUuid}`,
					);
					break;
				}
				case "intent": {
					retryCount = 0;
					const term = terminalsStore.get(props.id);
					const agentType = term?.agentType;
					const perAgentEnabled = agentType
						? (agentConfigsForRepo(props.cwd).getIntentTabTitle(agentType) ?? true)
						: true;
					handleIntentEvent({
						terminalId: props.id,
						text: parsed.text,
						title: parsed.title,
						globalEnabled: settingsStore.state.intentTabTitle,
						perAgentEnabled,
					});
					// Intent/suggest row overlays handled by installRenderObserver
					break;
				}
				case "suggest":
					// Backend guarantees `suggest` events only arrive once the shell has
					// transitioned to IDLE (see `drain_pending_suggest` in pty.rs, gated
					// on `SHELL_IDLE`). No frontend buffering needed: if the user hasn't
					// dismissed the previous cycle's chips, show the new set directly.
					if (settingsStore.state.suggestFollowups && parsed.items?.length) {
						const t = terminalsStore.get(props.id);
						if (t && !t.suggestDismissed) {
							terminalsStore.setSuggestedActions(props.id, parsed.items);
						} else if (t?.suggestDismissed) {
							appLogger.debug("terminal", `[Suggest] ${props.id} → dropped (dismissed)`);
						}
					}
					break;
				case "shell-state": {
					if (parsed.state !== "idle") {
						// Shell goes busy: clear stale suggest from previous cycle and reset
						// `suggestDismissed` so the next turn's suggestions (delivered by
						// the backend once shell returns to idle) can show.
						terminalsStore.update(props.id, {
							shellState: parsed.state,
							suggestedActions: null,
							suggestDismissed: false,
						});
					}
					if (parsed.state === "idle") {
						// Idle arrives first; any parked `suggest:` items follow on a later
						// silence-timer tick (backend-gated). No promotion logic needed here.
						terminalsStore.update(props.id, { shellState: parsed.state });
						// Idle: detect agent immediately — only idle can clear a detected agent
						detectAgentForTerminal(props.id, "idle").catch((err) =>
							appLogger.warn("terminal", "[AgentDetect] unexpected error", { error: String(err), termId: props.id }),
						);
					} else {
						// Busy: detect agent after 500ms debounce (can discover, never clear)
						clearTimeout(agentDetectTimer);
						agentDetectTimer = setTimeout(() => {
							detectAgentForTerminal(props.id, "busy").catch((err) =>
								appLogger.warn("terminal", "[AgentDetect] unexpected error", { error: String(err), termId: props.id }),
							);
						}, 500);
					}
					break;
				}
				case "slash-menu":
					// Desktop renders the TUI natively — no overlay needed. Plugins
					// still receive the event via dispatchStructuredEvent below.
					break;
				case "choice-prompt": {
					const isActive = terminalsStore.state.activeId === props.id;
					appLogger.info(
						"terminal",
						`[ChoicePrompt] ${props.id} title="${parsed.title}" options=${parsed.options.length}${isActive ? "" : " (background)"}`,
					);
					if (!isActive) {
						notificationsStore.playWarning();
					}
					break;
				}
				case "choice-cleared":
					// Durable choice state comes from the backend snapshot; plugins
					// still receive this one-shot lifecycle event below.
					break;
				case "agent-block": {
					if (parsed.action === "start") {
						terminalsStore.handleOsc133(props.id, "A", parsed.line);
					} else if (parsed.action === "end") {
						terminalsStore.handleOsc133(props.id, "D", parsed.line, parsed.exit_code ?? undefined);
					}
					break;
				}
			}

			pluginRegistry.dispatchStructuredEvent(parsed.type, parsed, targetSessionId);
		};

		// PTY activity + exit via transport abstraction (Tauri listen or WebSocket).
		// The output bytes are ignored: browser mode still streams them over this
		// socket for other consumers, desktop sends none, and neither is what this
		// component needs — see handlePtyActivity.
		unsubscribePty = await subscribePty(
			targetSessionId,
			() => {},
			() => {
				if (disposed) return;
				// Suspend closes the PTY on purpose and keeps the tab as it is.
				if (isSuspendingOrSuspended(props.id)) return;
				// Guard: terminal may have been removed from the store already
				// (e.g. pane closed). Updating a removed entry would recreate it as a ghost.
				const stillExists = terminalsStore.get(props.id);
				const hadAgent = stillExists?.agentType != null;
				const notifyOnExit = handleAgentExitCompletion(props.id);
				if (stillExists) {
					// Restore original tab name if it was overwritten by OSC title
					if (originalName && !stillExists.nameIsCustom) {
						terminalsStore.update(props.id, { name: originalName });
					}
					if (hadAgent) {
						// Agent finished: keep the tab with a grey "exited" dot so the user
						// can read the final output and re-launch. Without this a dead session
						// keeps its last shellState forever — a ghost tab that looks idle.
						//
						// DEFERRED (2026-09-02) — this does not actually deliver either half of
						// that promise. `sessionId: null` collapses the <Show> around
						// CanvasTerminal, so the final output is erased at the moment of exit;
						// `agentType: null` then discards what would have to be re-launched. The
						// content area now says so (see the exited fallback below) instead of
						// going black, which was the visible bug. Keeping the output needs
						// CanvasTerminal mounted against a dead sid — its ~35 async IPC handlers
						// would keep polling a session the backend reaps after TOMBSTONE_TTL_MS
						// (5 min), so it is a real design change, not a tweak. Needs Boss.
						terminalsStore.update(props.id, {
							shellState: "exited",
							sessionId: null,
							...(notifyOnExit ? { completionNotified: true } : {}),
							currentTask: null,
							agentType: null,
							agentSessionId: null,
						});
						terminalsStore.clearAwaitingInput(props.id);
						pluginRegistry.notifyStateChange({
							type: "agent-stopped",
							sessionId: targetSessionId,
							terminalId: props.id,
						});
					} else {
						// Plain interactive shell exited: drop the session refs and let the
						// app close the tab (see notifyShellExit below) — a dead shell must
						// not linger as a grey ghost dot.
						terminalsStore.update(props.id, { sessionId: null });
						terminalsStore.clearAwaitingInput(props.id);
					}
				}
				sessionId = null;
				setCurrentSessionId(null);
				props.onSessionExit?.(props.id);
				// Completion chime only for an agent finishing in a background tab.
				// A plain shell exit closes its tab, so it stays silent.
				if (!notifyOnExit && hadAgent && terminalsStore.state.activeId !== props.id) {
					appLogger.debug("terminal", `[Notify] ${props.id} completion SUPPRESSED — cycle already notified`);
				}
				// Plain shell exit: close the tab via the app-level onShellExit handler.
				// This is the single owner of local session-exit handling; the global
				// session-closed listener deliberately stays remote-only.
				if (stillExists && !hadAgent) {
					terminalsStore.notifyShellExit(props.id);
				}
			},
			{
				onActivity: handlePtyActivity,
				onTitle: handlePtyTitle,
				onReconnecting: (attempt, max) => setReconnecting({ attempt, max }),
				onReconnected: () => setReconnecting(null),
				// Browser mode: receive parsed events via WebSocket JSON frames
				onParsed: (frame) => {
					if (frame.type === "parsed" && frame.event) {
						handleParsedEvent(frame.event as ParsedEvent);
					}
				},
				onStateChange: (snapshot) => {
					if (disposed) return;
					applySessionStateEvent({ session_id: targetSessionId, state: snapshot });
				},
			},
		);

		// Tauri-only listeners (kitty keyboard, shell state sync)
		if (isTauri() && !getSessionConnection(targetSessionId)) {
			const { listen } = await import("@tauri-apps/api/event");
			unlistenParsed = await listen<ParsedEvent>(`pty-parsed-${targetSessionId}`, (event) => {
				handleParsedEvent(event.payload);
			});
			if (disposed) {
				safeUnlisten(unlistenParsed);
				unlistenParsed = undefined;
				return;
			}

			// Listen for kitty keyboard protocol flag changes from Rust
			unlistenKitty = await listen<number>(`kitty-keyboard-${targetSessionId}`, (event) => {
				kittyFlags = event.payload;
			});
			if (disposed) {
				safeUnlisten(unlistenKitty);
				unlistenKitty = undefined;
				return;
			}

			// No `pty-osc133` listener here: CanvasTerminal subscribes to the same
			// event through the transport, and is the only subscriber.
			// A second desktop delivery would reach a sink that is not idempotent
			// for the `A` marker — `handleOsc133` finalizes the block the first
			// delivery had just installed, inventing one empty command block per
			// prompt. `agent-block` above still feeds the same sink, but that is a
			// different source for agents with no shell integration, not a copy.

			// Listen for OSC 52 clipboard store from Rust (native renderer).
			// OSC 52 is honored from anywhere in the byte stream, so a displayed file/log
			// can overwrite the clipboard. Surface a non-blocking notice on every write and
			// let the user disable OSC 52 entirely via settings. (Gated here, off the
			// per-byte parse hot path — this fires once per actual OSC 52 sequence.)
			unlistenClipboardStore = await listen<string>(`pty-clipboard-store-${targetSessionId}`, (event) => {
				if (!settingsStore.state.osc52Clipboard) return;
				writeClipboard(event.payload).catch(() => {});
				const name = terminalsStore.get(props.id)?.name || "terminal";
				toastsStore.add("Clipboard updated", `by ${name}`, "info");
			});
			if (disposed) {
				safeUnlisten(unlistenClipboardStore);
				unlistenClipboardStore = undefined;
				return;
			}

			// Sync initial kitty flags — the push event may have fired before listener attached.
			// Only apply if the listener hasn't already updated kittyFlags (race guard).
			const preListenFlags = kittyFlags;
			const flags = await pty.getKittyFlags(targetSessionId);
			if (flags > 0 && kittyFlags === preListenFlags) {
				kittyFlags = flags;
			}

			// Sync shell state from Rust — covers events missed while unsubscribed
			// (e.g. tab switch, branch switch, component remount).
			invoke<string | null>("get_shell_state", { sessionId: targetSessionId })
				.then((rustState) => {
					if (rustState) {
						const current = terminalsStore.get(props.id)?.shellState;
						if (current !== rustState) {
							appLogger.debug("terminal", `[ShellState] ${props.id} sync from Rust: "${current}" → "${rustState}"`);
							if (isShellState(rustState)) {
								terminalsStore.update(props.id, { shellState: rustState });
							}
						}
					}
				})
				.catch((err) => {
					appLogger.debug("terminal", "Shell state sync failed", { sessionId, error: String(err) });
				});
		}
	};

	// Eagerly attach listeners for existing sessions (before terminal.open())
	// This prevents data loss when PTY output arrives while terminal is on a background tab
	const existingSessionId = terminalsStore.get(props.id)?.sessionId;
	if (existingSessionId) {
		sessionId = existingSessionId;
		setCurrentSessionId(existingSessionId);
		// attachSessionListeners syncs shell state from Rust via get_shell_state
		attachSessionListeners(existingSessionId).catch((err) =>
			appLogger.error("terminal", "Failed to attach session listeners", err),
		);
	}

	/** Initialize PTY session and event listeners */
	const initSession = async () => {
		if (sessionInitialized || !containerRef) return;
		// A suspended tab has no PTY on purpose; resuming clears the flag and re-runs this.
		if (terminalsStore.get(props.id)?.suspended) return;
		sessionInitialized = true;
		if (isPerfDebug()) {
			const spawnMs = Math.round(performance.now() - mountedAt);
			appLogger.info(
				"terminal",
				`initSession(${props.id}) — existing sessionId=${sessionId ?? "null"} spawnDelay=${spawnMs}ms`,
			);
		}

		const grid = calcGridSize(containerRef, props.id);

		try {
			let reconnected = false;
			if (sessionId) {
				try {
					await pty.resize(sessionId, grid.rows, grid.cols);
					reconnected = true;
					appLogger.info("terminal", `initSession(${props.id}) — reconnected to ${sessionId}`);
					detectAgentForTerminal(props.id, "idle").catch(() => {});
				} catch {
					// If we unmounted during the resize await, onCleanup already tore
					// down listeners; setCurrentSessionId below would recompute a
					// disposed <Show> and crash the SolidJS root (UI freeze). Bail.
					if (disposed) return;
					appLogger.warn(
						"terminal",
						`initSession(${props.id}) — resize failed for ${sessionId}, creating FRESH session`,
					);
					sessionId = null;
					setCurrentSessionId(null);
					detachSessionListeners();
				}
			}
			if (!reconnected) {
				appLogger.debug("terminal", `initSession(${props.id}) — creating FRESH PTY session (no prior sessionId)`);
				const termData = terminalsStore.get(props.id);
				// The env flags describe the agent binary on the machine that will run
				// it. Local resolves without a round trip — it is loaded at boot.
				const machineConfigs = await ensureAgentConfigsForRepo(props.cwd);
				sessionId = await pty.createSession({
					rows: grid.rows,
					cols: grid.cols,
					shell: settingsStore.state.shell ?? null,
					cwd: props.cwd || null,
					tuic_session: termData?.tuicSession ?? null,
					alias: termData?.alias ?? null,
					env: machineConfigs.getEnvFlags("claude"),
					agent_type: termData?.pendingInitCommand ? (termData.agentType ?? null) : null,
				});
				// The component can unmount during the await above (tab churn while
				// an agent like grok rapidly toggles visibility). setCurrentSessionId
				// then recomputes the now-disposed <Show when={_currentSessionId()}>
				// and throws "stale value from <Show>" — unhandled, it kills the
				// SolidJS root and the whole UI freezes while the backend stays alive.
				// Persist the new id (plain store write) so a remount reconnects
				// instead of leaking a duplicate PTY, then bail before any reactive write.
				if (disposed) {
					if (sessionId && terminalsStore.get(props.id)) {
						terminalsStore.setSessionId(props.id, sessionId);
					}
					return;
				}
				// CanvasTerminal chooses its transport at mount. Register ownership before
				// mounting it, or a fresh remote PTY is attached to local Tauri IPC.
				terminalsStore.setSessionId(props.id, sessionId);
				setCurrentSessionId(sessionId);
				if (sessionId) {
					if (!isTauri()) {
						browserCreatedSessions.add(sessionId);
					}
					await attachSessionListeners(sessionId);
					if (disposed) {
						terminalsStore.setSessionId(props.id, sessionId);
						return;
					}
				}
			}

			if (sessionId) {
				terminalsStore.setSessionId(props.id, sessionId);
				props.onSessionCreated?.(props.id, sessionId);
			}
		} catch (err) {
			appLogger.error("terminal", `Failed to create PTY: ${err}`);
			if (!disposed)
				setSpawnError(err instanceof HttpRpcError ? err.detail : err instanceof Error ? err.message : String(err));
		}
	};

	// Check if this terminal is visible. A terminal in a pane-tree group is only
	// visible when it's the active tab of its group — otherwise every tab in the
	// group runs xterm/ResizeObserver/ScrollTracker/PTY-subscribe concurrently,
	// which multiplies background work by the number of tabs per pane.
	const isActiveInPaneGroup = () => {
		if (!paneLayoutStore.isSplit()) return false;
		const groupId = paneLayoutStore.getGroupForTab(props.id);
		if (!groupId) return false;
		return paneLayoutStore.state.groups[groupId]?.activeTabId === props.id;
	};
	const isVisible = () =>
		props.alwaysVisible ||
		(terminalsStore.state.activeId === props.id && !terminalsStore.isDetached(props.id)) ||
		isActiveInPaneGroup();

	// When this terminal becomes visible: init PTY session and auto-focus.
	// CanvasTerminal handles its own rendering lifecycle.
	//
	// `on(isVisible, …)` so the body runs ONLY on a visibility transition. A bare
	// createEffect re-ran on every tracked-store write and each re-run scheduled
	// another `canvasTerminalRef().focus()`, which yanked the caret out of the
	// search bar (and the compose panel) while the user was still typing in it.
	createEffect(
		on(isVisible, (visible) => {
			if (!visible) return;
			const containerIsSized = () => !!containerRef && containerRef.offsetWidth > 0 && containerRef.offsetHeight > 0;

			rafHandle = requestAnimationFrame(() => {
				rafHandle = 0;
				if (!containerIsSized()) {
					// Container not ready — retry on the next frames, but bounded and
					// cancellable: a pane the user leaves collapsed never gets a size.
					cancelSizeRetry = retryUntilSized(
						containerIsSized,
						() =>
							initSession().catch((e) =>
								appLogger.error("terminal", "initSession failed after size retry", { error: String(e) }),
							),
						() =>
							appLogger.warn(
								"terminal",
								`Container stayed zero-size for ${SIZE_RETRY_MAX_FRAMES} frames — giving up on init for ${props.id}`,
							),
					);
					return;
				}
				initSession().catch((e) => appLogger.error("terminal", "initSession failed", { error: String(e) }));

				// Never steal the caret from a field the user is typing in — the search
				// bar and the compose panel live inside this same terminal wrapper.
				if (terminalsStore.state.activeId === props.id && !focusIsInsideOwnInput(document.activeElement, props.id)) {
					if (chatActive()) setComposeFocusRequest((n) => n + 1);
					else canvasTerminalRef()?.focus();
				}
			});

			onCleanup(() => {
				if (rafHandle) cancelAnimationFrame(rafHandle);
				rafHandle = 0;
				cancelSizeRetry?.();
				cancelSizeRetry = null;
			});
		}),
	);

	// Suspend ends the PTY and keeps the tab; resume opens a new PTY through initSession.
	createEffect(
		on(
			() => terminalsStore.get(props.id)?.suspended ?? false,
			(suspended) => {
				if (suspended) {
					if (sessionId) pluginRegistry.removeSession(sessionId);
					detachSessionListeners();
					kittyFlags = 0;
					sessionId = null;
					setCurrentSessionId(null);
					sessionInitialized = false;
				} else if (isVisible()) {
					initSession().catch((e) =>
						appLogger.error("terminal", "initSession failed after resume", { error: String(e) }),
					);
				}
			},
			{ defer: true },
		),
	);

	// Alt-screen recovery: when an agent exits without leaving alt-screen,
	// inject exit sequences directly into the terminal grid (display side only).
	// Never writes to PTY stdin — that would leak as shell input.
	let lastAgentType: string | null = null;
	createEffect(() => {
		const curr = terminalsStore.get(props.id)?.agentType ?? null;
		if (lastAgentType !== null && curr === null && sessionId) {
			invoke<boolean>("terminal_exit_alt_screen", { sessionId })
				.then((wasAlt) => {
					if (wasAlt) {
						appLogger.debug(
							"terminal",
							`[Recovery] ${props.id} exited alt-screen after agent "${lastAgentType}" → null`,
						);
					}
				})
				.catch(() => {});
		}
		lastAgentType = curr;
	});

	onCleanup(() => {
		disposed = true;
		clearTimeout(retryTimer);
		clearTimeout(agentDetectTimer);
		clearTimeout(questionDebounceTimer);
		detachSessionListeners();
		kittyFlags = 0;

		if (sessionId) pluginRegistry.removeSession(sessionId);
	});

	const refMethods = {
		fit: () => canvasTerminalRef()?.refresh(),
		write: (data: string) => {
			if (sessionId)
				pty.write(sessionId, data).catch((err) => appLogger.error("terminal", "Failed to write to PTY", err));
		},
		writeln: (data: string) => {
			// PTY injection rule: writeln submits a line, so route through
			// sendCommand (agent-aware Enter; Ink raw-mode ignores a bare \n and
			// Windows needs \r\n). `write`/`input` below stay raw on purpose — they
			// are low-level byte escapes for plugins.
			if (sessionId)
				pty
					.sendCommand(sessionId, data, terminalsStore.get(props.id)?.agentType ?? null)
					.catch((err) => appLogger.error("terminal", "writeln failed", err));
		},
		input: (data: string) => {
			if (sessionId) pty.write(sessionId, data).catch((err) => appLogger.error("terminal", "input failed", err));
		},
		clear: () => {
			if (sessionId)
				pty.write(sessionId, "\x1b[2J\x1b[H\x1b[3J").catch((err) => appLogger.error("terminal", "clear failed", err));
		},
		refresh: () => canvasTerminalRef()?.refresh(),
		focus: () => {
			if (chatActive()) {
				setComposeFocusRequest((n) => n + 1);
				return;
			}
			const ref = canvasTerminalRef();
			if (ref) ref.focus();
			else pendingCanvasFocus = true;
		},
		getSessionId: () => sessionId,
		openSearch: () => {
			if (chatActive()) chatSearchRef?.open();
			else openSearchBar();
		},
		closeSearch: () => {
			closeSearchBar();
			chatSearchRef?.close();
		},
		toggleCompose: () => {
			if (chatActive()) {
				setComposeFocusRequest((n) => n + 1);
				return;
			}
			if (composePinned()) {
				// Pinned stays open: the shortcut moves the caret between the two inputs.
				if (focusIsInsideOwnInput(document.activeElement, props.id)) canvasTerminalRef()?.focus();
				else setComposeFocusRequest((n) => n + 1);
				return;
			}
			if (composeOpen()) {
				setComposeOpen(false);
				canvasTerminalRef()?.focus();
				return;
			}
			if (sessionId && !pendingComposeText()) {
				invoke("terminal_get_cursor_line", { sessionId })
					.then((raw) => {
						setPendingComposeText(stripPrompt(raw as string));
						setComposeOpen(true);
					})
					.catch(() => setComposeOpen(true));
			} else {
				setComposeOpen(true);
			}
		},
		openComposeWithText: (text: string) => {
			setPendingComposeText(text);
			if (!chatActive()) setComposeOpen(true);
			setComposeTextRequest((n) => n + 1);
		},
		searchBuffer: (query: string) => {
			if (!sessionId) return [];
			type RustMatch = { line_index: number; line_text: string; match_start: number; match_end: number };
			return invoke("terminal_search_buffer", { sessionId, query }).then((raw) => {
				const name = terminalsStore.get(props.id)?.name ?? props.id;
				return (raw as RustMatch[]).map((m) => ({
					terminalId: props.id,
					terminalName: name,
					lineIndex: m.line_index,
					lineText: m.line_text,
					matchStart: m.match_start,
					matchEnd: m.match_end,
				}));
			});
		},
		scrollToLine: (lineIndex: number) => {
			if (sessionId) invoke("terminal_scroll_to", { sessionId, line: lineIndex }).catch(() => {});
		},
		scrollToTop: () => {
			if (sessionId) {
				invoke("terminal_scroll_info", { sessionId })
					.then((info) => {
						const [, total] = info as [number, number, number];
						invoke("terminal_scroll", { sessionId, delta: total }).catch(() => {});
					})
					.catch(() => {});
			}
		},
		scrollToBottom: () => {
			if (sessionId) {
				invoke("terminal_scroll_info", { sessionId })
					.then((info) => {
						const [offset] = info as [number, number, number];
						if (offset > 0) invoke("terminal_scroll", { sessionId, delta: -offset }).catch(() => {});
					})
					.catch(() => {});
			}
		},
		scrollPages: (pages: number) => {
			if (sessionId) {
				invoke("terminal_scroll_info", { sessionId })
					.then((info) => {
						const [, , screenLines] = info as [number, number, number];
						const rows = screenLines || 24;
						invoke("terminal_scroll", { sessionId, delta: -(pages * rows) }).catch(() => {});
					})
					.catch(() => {});
			}
		},
		getSelection: () => canvasTerminalRef()?.getSelectionText() ?? "",
		getBufferLines: (startLine: number, endLine: number) => {
			if (!sessionId) return [];
			return invoke("terminal_get_lines", { sessionId, start: startLine, end: endLine }) as Promise<string[]>;
		},
		paste: (text: string) => canvasTerminalRef()?.paste(text),
	};

	onMount(() => {
		terminalsStore.update(props.id, { ref: refMethods });
	});

	// Re-register ref and resubscribe to grid channel after a REAL reattach: the
	// tab was detached into a floating window whose Terminal overwrote the grid
	// channel subscription and the ref in the store.
	//
	// Keyed on the detach, not on visibility alone. A plain tab switch also flips
	// visibility false→true, and resubscribing there wiped the grid on every
	// switch — `refresh()` drops the current frame before the replacement arrives,
	// so the user saw paint → wipe → paint.
	createEffect((prev: ReattachPhase) => {
		const { phase, resubscribe } = stepReattachPhase(prev, isVisible(), terminalsStore.isDetached(props.id));
		if (resubscribe) {
			terminalsStore.update(props.id, { ref: refMethods });
			requestAnimationFrame(() => {
				requestAnimationFrame(() => {
					const ref = canvasTerminalRef();
					if (!ref) return;
					ref
						.resubscribe()
						.then(() => ref.refresh())
						.catch((e) => appLogger.warn("terminal", "Grid resubscribe after reattach failed", { error: String(e) }));
				});
			});
		}
		return phase;
	}, REATTACH_PHASE_INITIAL);

	const handleBell = () => {
		const style = settingsStore.state.bellStyle;
		if (style === "none") return;
		if (style === "visual" || style === "both") {
			containerRef?.classList.add("bell-flash");
			setTimeout(() => containerRef?.classList.remove("bell-flash"), 150);
		}
		if (style === "sound" || style === "both") {
			notificationsStore.play("info").catch((err) => {
				appLogger.warn("terminal", "Bell audio playback failed", err);
			});
		}
	};

	const handleResume = () => {
		const cmd = terminalsStore.get(props.id)?.pendingResumeCommand;
		if (cmd && sessionId) {
			terminalsStore.update(props.id, { pendingResumeCommand: null });
			pty
				.sendCommand(sessionId, cmd, null)
				.catch((e) => appLogger.error("terminal", "Failed to write resume command", { error: String(e) }));
		}
	};

	/** What the tab was doing when it was saved — the intent if the agent declared one,
	 *  otherwise the last prompt. Both are restored from the session snapshot. */
	const resumeContext = () => {
		const term = terminalsStore.get(props.id);
		const intent = term?.agentIntent;
		if (intent) return `Intent: ${intent}`;
		const prompt = term?.lastPrompt;
		return prompt ? `Prompt: ${prompt}` : null;
	};

	const handleDismissResume = (e: MouseEvent) => {
		e.preventDefault();
		e.stopPropagation();
		terminalsStore.update(props.id, { pendingResumeCommand: null });
		canvasTerminalRef()?.focus();
	};

	/** True once this tab's backend session is gone for good: an agent finished
	 *  (pty-exit handler above) or a remote session closed. Deliberately keyed on
	 *  shellState, not on a null sessionId — that is also the pre-init state of a
	 *  freshly opened tab, which must not flash the notice before its PTY exists. */
	const sessionEnded = () => terminalsStore.get(props.id)?.shellState === "exited";

	const handleFileDragOver = (e: DragEvent) => {
		if (e.dataTransfer?.types?.includes("application/x-tuic-path")) {
			e.preventDefault();
			e.dataTransfer.dropEffect = "copy";
		}
	};

	const handleFileDrop = (e: DragEvent) => {
		const path = e.dataTransfer?.getData("application/x-tuic-path");
		if (!path || !sessionId) return;
		e.preventDefault();
		const quoted = `'${path.replaceAll("'", "'\\''")}' `;
		pty.write(sessionId, quoted);
		canvasTerminalRef()?.focus();
	};

	return (
		<div
			class={s.wrapper}
			data-terminal-id={props.id}
			data-focus-target="terminal"
			onDragOver={handleFileDragOver}
			onDrop={handleFileDrop}
		>
			<TerminalSearch
				visible={searchVisible()}
				focusToken={searchFocusToken()}
				canvasRef={canvasTerminalRef()}
				onClose={() => {
					closeSearchBar();
					canvasTerminalRef()?.focus();
				}}
			/>
			<ViewModeToggle terminalId={props.id} />
			<ChatViewFallbackBanner terminalId={props.id} />
			<Show
				when={
					settingsStore.state.showLastPrompt &&
					terminalsStore.get(props.id)?.agentType &&
					(terminalsStore.get(props.id)?.agentIntent ||
						terminalsStore.get(props.id)?.lastPrompt ||
						terminalsStore.get(props.id)?.ptyDescription)
				}
			>
				<LastPromptBar
					intent={() => terminalsStore.get(props.id)?.agentIntent ?? null}
					ptyDescription={() => terminalsStore.get(props.id)?.ptyDescription ?? null}
					prompt={() => terminalsStore.get(props.id)?.lastPrompt ?? null}
				/>
			</Show>
			<Show when={reconnecting()}>
				{(info) => (
					<div class={s.reconnectBanner}>
						Reconnecting ({info().attempt}/{info().max})...
					</div>
				)}
			</Show>
			<Show when={terminalsStore.get(props.id)?.pendingResumeCommand}>
				<div
					class={s.resumeBanner}
					role="button"
					tabIndex={0}
					onClick={handleResume}
					onKeyDown={onClickKeyDown(handleResume)}
				>
					<span>Agent session was active — click to resume</span>
					<Show when={resumeContext()}>
						{(ctx) => (
							<span class={s.resumeContext} title={ctx()}>
								{ctx()}
							</span>
						)}
					</Show>
					<button class={s.resumeDismiss} onClick={handleDismissResume} title="Dismiss">
						&times;
					</button>
				</div>
			</Show>
			{/* Chat mode HIDES the grid (display:none, the background-tab path) and never
			    unmounts it: a disposed CanvasTerminal under a queued frame event froze the UI. */}
			<div ref={containerRef} class={s.content} classList={{ [s.contentHidden]: chatActive() }}>
				{/* keyed: pass sessionId as a stable string value, NOT the Show's reactive
				    accessor. CanvasTerminal's onFrame (and ~35 other async IPC handlers) re-read
				    props.sessionId on every backend frame. With a non-keyed Show, props.sessionId
				    is the Show children-accessor sid(); when a PTY auto-closes, pty-exit sets
				    _currentSessionId(null) → this Show collapses → CanvasTerminal unmounts, but a
				    frame event already queued in the WebView loop fires onFrame post-dispose,
				    reading sid() on the disposed <Show> → "Stale read from <Show>" → kills the
				    SolidJS root (whole UI frozen, hover-only, backend alive). keyed makes sid a
				    plain string captured at mount, immune to stale reads. Transitions are always
				    null↔id so CanvasTerminal already remounts on session change — no behavior change. */}
				{/* fallback: an exited tab is kept on purpose (grey dot, readable name), but
				    unmounting CanvasTerminal leaves .content an empty flex column — a black
				    void indistinguishable from a broken terminal. Say what happened instead. */}
				<Show
					keyed
					when={_currentSessionId()}
					fallback={
						<Show
							when={spawnError()}
							fallback={
								<>
									<Show when={terminalsStore.get(props.id)?.suspended}>
										<div class={s.exitedNotice} data-testid="terminal-suspended-notice">
											<span class={s.exitedTitle}>{t("terminal.suspended.title", "Suspended")}</span>
											<span class={s.exitedHint}>
												{t(
													"terminal.suspended.hint",
													"The process was ended to free memory and CPU. Resume opens a new session in the same folder.",
												)}
											</span>
											<Show when={resumeContext()}>
												{(ctx) => (
													<span class={s.resumeContext} title={ctx()}>
														{ctx()}
													</span>
												)}
											</Show>
											<button class={s.resumeButton} type="button" onClick={() => void resumeTerminal(props.id)}>
												{t("terminal.suspended.resume", "Resume")}
											</button>
										</div>
									</Show>
									<Show when={sessionEnded()}>
										<div class={s.exitedNotice} data-testid="terminal-exited-notice">
											<span class={s.exitedTitle}>{t("terminal.exited.title", "Session ended")}</span>
											<span class={s.exitedHint}>
												{t(
													"terminal.exited.hint",
													"The process exited and its output was released. Close this tab to remove it.",
												)}
											</span>
										</div>
									</Show>
								</>
							}
						>
							<div class={s.exitedNotice} role="alert">
								<span class={s.exitedTitle}>{t("terminal.spawnFailed", "Terminal could not start")}</span>
								<span class={s.exitedHint}>{spawnError()}</span>
							</div>
						</Show>
					}
				>
					{(sid) => (
						<CanvasTerminal
							sessionId={sid}
							terminalId={props.id}
							onOpenFilePath={props.onOpenFilePath}
							onSearchOpen={() => openSearchBar()}
							onSearchClose={() => closeSearchBar()}
							searchVisible={searchVisible()}
							onResume={handleResume}
							onResumeDismiss={() => terminalsStore.update(props.id, { pendingResumeCommand: null })}
							hasPendingResume={!!terminalsStore.get(props.id)?.pendingResumeCommand}
							onFocus={() => props.onFocus?.(props.id)}
							onCwdChange={props.onCwdChange}
							onRef={(ref) => {
								setCanvasTerminalRef(ref);
								if (pendingCanvasFocus) {
									// A focus requested before the canvas existed is replayed here, so
									// it can land arbitrarily late — after the user opened the search
									// bar and started typing. Same guard as the visibility effect:
									// the request is dropped, not queued, since by now the user has
									// told us where the caret belongs (#ce43).
									pendingCanvasFocus = false;
									if (!focusIsInsideOwnInput(document.activeElement, props.id)) ref.focus();
								}
							}}
							onBell={handleBell}
						/>
					)}
				</Show>
			</div>
			<Show keyed when={chatActive() ? _currentSessionId() : null}>
				{(sid) => (
					<TerminalChatView
						terminalId={props.id}
						sessionId={sid}
						onSearchRef={(ref) => {
							chatSearchRef = ref;
						}}
					/>
				)}
			</Show>
			<Show when={!composeOpen() && !chatActive()}>
				<div
					class={s.composeHint}
					onClick={() => setComposeOpen(true)}
					title={`Open compose editor (${keyFor("toggle-compose-panel")})`}
				>
					<svg
						width="10"
						height="10"
						viewBox="0 0 16 16"
						fill="currentColor"
						style={{ "margin-right": "4px", opacity: 0.7 }}
					>
						<path d="M12.146.854a.5.5 0 0 1 .708 0l2.292 2.292a.5.5 0 0 1 0 .708L5.854 13.146a.5.5 0 0 1-.233.131l-3.5 1a.5.5 0 0 1-.617-.617l1-3.5a.5.5 0 0 1 .131-.233L12.146.854z" />
					</svg>
					Compose {keyFor("toggle-compose-panel")}
				</div>
			</Show>
			<Show when={composeVisible()}>
				<Suspense>
					<ComposePanel
						isOpen={composeVisible}
						initialText={pendingComposeText}
						onTextChange={setPendingComposeText}
						onSubmitted={finishCompose}
						pinned={composeDocked}
						persistent={chatActive}
						onTogglePin={() => setComposePinned(!composePinned())}
						onDismiss={() => {
							setComposePinned(false);
							setComposeOpen(false);
							canvasTerminalRef()?.focus();
						}}
						focusRequest={composeFocusRequest}
						textRequest={composeTextRequest}
						canEnqueue={() => !!terminalsStore.get(props.id)?.agentType}
						queuedCount={() => terminalsStore.get(props.id)?.queuedCommands ?? 0}
						onClearQueue={async () => {
							if (!sessionId) return;
							try {
								await pty.clearQueuedCommands(sessionId);
								terminalsStore.update(props.id, { queuedCommands: 0 });
							} catch (err) {
								appLogger.error("terminal", "ComposePanel clear queue failed", { sessionId, error: err });
							}
						}}
						onLoadQueue={async () => {
							if (!sessionId) return [];
							try {
								return await pty.listQueuedCommands(sessionId);
							} catch (err) {
								appLogger.error("terminal", "ComposePanel list queue failed", { sessionId, error: err });
								return [];
							}
						}}
						onRemoveQueued={async (commandId) => {
							if (!sessionId) return;
							try {
								await pty.removeQueuedCommand(sessionId, commandId);
								// Same reason the enqueue path trusts its own count: the badge
								// and the list must react to the click, not to the 1s poll.
								const remaining = await pty.listQueuedCommands(sessionId);
								terminalsStore.update(props.id, { queuedCommands: remaining.length });
							} catch (err) {
								appLogger.error("terminal", "ComposePanel remove queued failed", { sessionId, error: err });
							}
						}}
						onEnqueue={async (text) => {
							await submitCompose("enqueue", sessionId, async (id) => {
								const outcome = await pty.enqueueCommand(id, text);
								// Trust the call's own count instead of waiting for the next 1s
								// lifecycle poll — the badge must react to the click.
								terminalsStore.update(props.id, { queuedCommands: outcome.queued });
							});
						}}
						onClose={() => {
							if (chatActive()) terminalsStore.setViewMode(props.id, "cli");
							else if (!composePinned()) setComposeOpen(false);
							canvasTerminalRef()?.focus();
						}}
						onSend={async (text) => {
							await submitCompose("send", sessionId, (id) =>
								pty.sendCommand(id, text, terminalsStore.get(props.id)?.agentType),
							);
						}}
					/>
				</Suspense>
			</Show>
		</div>
	);
};

export default Terminal;
