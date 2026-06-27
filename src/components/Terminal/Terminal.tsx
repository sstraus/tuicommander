import { type Component, createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { detectAgentForTerminal } from "../../hooks/useAgentPolling";
import { browserCreatedSessions } from "../../hooks/useAppInit";
import { usePty } from "../../hooks/usePty";
import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { agentConfigsStore } from "../../stores/agentConfigs";
import { appLogger } from "../../stores/appLogger";
import { notificationsStore } from "../../stores/notifications";
import { paneLayoutStore } from "../../stores/paneLayout";
import { rateLimitStore } from "../../stores/ratelimit";
import { FONT_FAMILIES, settingsStore } from "../../stores/settings";
import { type AwaitingInputType, isShellState, terminalsStore } from "../../stores/terminals";
import { isTauri, subscribePty, type Unsubscribe } from "../../transport";
import { keyFor } from "../../utils/hotkey";
import { isPerfDebug } from "../../utils/perfDebug";
import { ComposePanel } from "../ComposePanel";
import { getAwaitingInputSound } from "./awaitingInputSound";
import CanvasTerminal, { type CanvasTerminalRef } from "./CanvasTerminal";
import { snapLineHeight } from "./canvasTerminalUtils";
import { getSharedMetrics } from "./glyphCache";
import { shouldApplyIntentTitle } from "./intentTitle";
import { LastPromptBar } from "./LastPromptBar";
import s from "./Terminal.module.css";
import { TerminalSearch } from "./TerminalSearch";

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
	let cleaned = title.replace(
		/^[\s*\u00B7\u2022\u2219\u22C5\u2027\u25A0-\u25FF\u2800-\u28FF\u2720-\u273F\u2580-\u259F]+/,
		"",
	);
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

/** Get initial terminal dimensions from container size + font metrics. */
function calcGridSize(container: HTMLElement): { rows: number; cols: number } {
	const fontSize = settingsStore.state.defaultFontSize;
	const fontFamily = FONT_FAMILIES[settingsStore.state.font] || FONT_FAMILIES["JetBrains Mono"];
	const fontWeight = settingsStore.state.fontWeight;
	const dpr = window.devicePixelRatio || 1;
	const m = getSharedMetrics(fontSize, fontFamily, dpr, snapLineHeight(fontSize), fontWeight);
	const cols = Math.max(2, Math.floor(container.clientWidth / m.cellWidth));
	const rows = Math.max(2, Math.floor(container.clientHeight / m.cellHeight));
	return { rows, cols };
}

export const Terminal: Component<TerminalProps> = (props) => {
	let containerRef: HTMLDivElement | undefined;
	let sessionId: string | null = null;
	const [_currentSessionId, setCurrentSessionId] = createSignal<string | null>(null);

	const [canvasTerminalRef, setCanvasTerminalRef] = createSignal<CanvasTerminalRef | undefined>();
	let pendingCanvasFocus = false;

	const [searchVisible, setSearchVisible] = createSignal(false);
	const [composeOpen, setComposeOpen] = createSignal(false);
	const [pendingComposeText, setPendingComposeText] = createSignal("");
	const [reconnecting, setReconnecting] = createSignal<{ attempt: number; max: number } | null>(null);
	let sessionInitialized = false;
	let disposed = false;
	let unsubscribePty: Unsubscribe | undefined;
	let unlistenParsed: (() => void) | undefined;
	let unlistenKitty: (() => void) | undefined;
	let unlistenOsc133: (() => void) | undefined;
	let unlistenTitle: (() => void) | undefined;
	let unlistenClipboardStore: (() => void) | undefined;

	let kittyFlags = 0;

	const RETRY_DELAYS = [5_000, 15_000, 30_000];
	let retryCount = 0;
	let retryTimer: ReturnType<typeof setTimeout> | undefined;
	let agentDetectTimer: ReturnType<typeof setTimeout> | undefined;
	let rafHandle = 0;

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

	// Original tab name before any OSC title overwrote it
	let originalName: string | null = null;

	const pty = usePty();

	/** Track PTY activity for the activity dashboard. CanvasTerminal handles
	 *  rendering and plugin dispatch; this callback only updates store metadata. */
	const handlePtyData = (_data: string) => {
		if (disposed) return;
		const now = Date.now();
		if (!lastDataAtTimestamp || now - lastDataAtTimestamp > 1000) {
			lastDataAtTimestamp = now;
			terminalsStore.touchLastDataAt(props.id, now);
		}
		if (terminalsStore.state.activeId !== props.id && !activityFlagged) {
			activityFlagged = true;
			terminalsStore.update(props.id, { activity: true });
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
					const awProg = terminalsStore.get(props.id)?.awaitingInput;
					if (awProg && awProg !== "error" && awProg !== "question") {
						terminalsStore.clearAwaitingInput(props.id);
					}
					if (parsed.state === 0) {
						terminalsStore.update(props.id, { progress: null });
					} else if (parsed.state === 1 || parsed.state === 2 || parsed.state === 3) {
						terminalsStore.update(props.id, { progress: Math.min(100, Math.max(0, parsed.value)) });
					}
					break;
				}
				case "status-line": {
					retryCount = 0;
					const awState = terminalsStore.get(props.id)?.awaitingInput;
					const clearAw = awState && awState !== "question" && awState !== "error";
					if (clearAw) {
						appLogger.debug("terminal", `clearAwaitingInput(${props.id}) was "${awState}" → null`);
					}
					terminalsStore.update(props.id, {
						currentTask: parsed.task_name,
						...(clearAw ? { awaitingInput: null, awaitingInputConfident: false } : {}),
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
					const qTerminal = terminalsStore.get(props.id);
					if (!parsed.confident && (qTerminal?.shellState === "busy" || (qTerminal?.activeSubTasks ?? 0) > 0)) {
						appLogger.debug(
							"terminal",
							`[ParsedEvent] ${props.id} question IGNORED (busy=${qTerminal?.shellState === "busy"} subTasks=${qTerminal?.activeSubTasks} low-confidence) prompt="${parsed.prompt_text}"`,
						);
						break;
					}
					terminalsStore.setAwaitingInput(props.id, "question", !!parsed.confident);
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
						agentConfigsStore.isAutoRetryEnabled(agent) &&
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
							agentConfigsStore.isAutoRetryEnabled(agent) &&
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
					terminalsStore.setAgentIntent(props.id, parsed.text);
					const term = terminalsStore.get(props.id);
					const agentType = term?.agentType;
					const perAgentEnabled = agentType ? (agentConfigsStore.getIntentTabTitle(agentType) ?? true) : true;
					if (
						shouldApplyIntentTitle({
							title: parsed.title,
							globalEnabled: settingsStore.state.intentTabTitle,
							perAgentEnabled,
							nameIsCustom: term?.nameIsCustom ?? false,
						})
					) {
						terminalsStore.update(props.id, { name: parsed.title });
					}
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
						const pendingT = terminalsStore.get(props.id);
						const initCmd = pendingT?.pendingInitCommand;
						terminalsStore.update(props.id, {
							shellState: parsed.state,
							...(initCmd ? { pendingInitCommand: null } : {}),
						});
						if (initCmd && targetSessionId) {
							pty
								.sendCommand(targetSessionId, initCmd, null)
								.catch((e) => appLogger.error("terminal", "Failed to write init command", { error: String(e) }));
						}
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

		// PTY output + exit via transport abstraction (Tauri listen or WebSocket)
		unsubscribePty = await subscribePty(
			targetSessionId,
			(data: string) => handlePtyData(data),
			() => {
				if (disposed) return;
				// Guard: terminal may have been removed from the store already
				// (e.g. pane closed). Updating a removed entry would recreate it as a ghost.
				const stillExists = terminalsStore.get(props.id);
				if (stillExists) {
					// Restore original tab name if it was overwritten by OSC title
					if (originalName && !stillExists.nameIsCustom) {
						terminalsStore.update(props.id, { name: originalName });
					}
					const hadAgent = stillExists.agentType !== null;
					terminalsStore.update(props.id, {
						// Mark the shell as exited so the tab dot turns grey/dim instead of
						// staying green ("idle"). Without this, a dead session keeps its last
						// shellState forever — a ghost tab that looks idle but has no PTY.
						shellState: "exited",
						sessionId: null,
						currentTask: null,
						agentType: null,
						agentSessionId: null,
					});
					terminalsStore.clearAwaitingInput(props.id);
					if (hadAgent) {
						pluginRegistry.notifyStateChange({
							type: "agent-stopped",
							sessionId: targetSessionId,
							terminalId: props.id,
						});
					}
				}
				sessionId = null;
				setCurrentSessionId(null);
				props.onSessionExit?.(props.id);
				if (terminalsStore.state.activeId !== props.id) {
					appLogger.info("terminal", `[Notify] ${props.id} completion — session exited (background tab)`);
					notificationsStore.playCompletion();
				}
			},
			{
				onReconnecting: (attempt, max) => setReconnecting({ attempt, max }),
				onReconnected: () => setReconnecting(null),
				// Browser mode: receive parsed events via WebSocket JSON frames
				onParsed: (frame) => {
					if (frame.type === "parsed" && frame.event) {
						handleParsedEvent(frame.event as ParsedEvent);
					}
				},
			},
		);

		// Tauri-only listeners (kitty keyboard, shell state sync)
		if (isTauri()) {
			const { listen } = await import("@tauri-apps/api/event");
			unlistenParsed = await listen<ParsedEvent>(`pty-parsed-${targetSessionId}`, (event) => {
				handleParsedEvent(event.payload);
			});
			if (disposed) {
				unlistenParsed();
				unlistenParsed = undefined;
				return;
			}

			// Listen for kitty keyboard protocol flag changes from Rust
			unlistenKitty = await listen<number>(`kitty-keyboard-${targetSessionId}`, (event) => {
				kittyFlags = event.payload;
			});
			if (disposed) {
				unlistenKitty();
				unlistenKitty = undefined;
				return;
			}

			// Listen for OSC 133 shell integration markers from Rust (native renderer)
			unlistenOsc133 = await listen<{ marker: string; line: number; exit_code: number | null }>(
				`pty-osc133-${targetSessionId}`,
				(event) => {
					if (disposed) return;
					const { marker, line, exit_code } = event.payload;
					terminalsStore.handleOsc133(props.id, marker, line, exit_code ?? undefined);
				},
			);
			if (disposed) {
				unlistenOsc133();
				unlistenOsc133 = undefined;
				return;
			}

			// Listen for OSC 0/2 title changes from Rust (native renderer)
			unlistenTitle = await listen<string>(`pty-title-${targetSessionId}`, (event) => {
				if (disposed) return;
				const title = event.payload;
				const term = terminalsStore.get(props.id);
				if (term?.nameIsCustom || (term?.agentIntent && settingsStore.state.intentTabTitle)) return;
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
			});

			// Listen for OSC 52 clipboard store from Rust (native renderer)
			unlistenClipboardStore = await listen<string>(`pty-clipboard-store-${targetSessionId}`, (event) => {
				navigator.clipboard.writeText(event.payload).catch(() => {});
			});
			if (disposed) {
				unlistenClipboardStore();
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
		sessionInitialized = true;
		if (isPerfDebug()) {
			const spawnMs = Math.round(performance.now() - mountedAt);
			appLogger.info(
				"terminal",
				`initSession(${props.id}) — existing sessionId=${sessionId ?? "null"} spawnDelay=${spawnMs}ms`,
			);
		}

		const grid = calcGridSize(containerRef);

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
					try {
						unsubscribePty?.();
					} catch {
						/* listener already gone */
					}
					unsubscribePty = undefined;
					try {
						unlistenParsed?.();
					} catch {
						/* */
					}
					unlistenParsed = undefined;
					try {
						unlistenKitty?.();
					} catch {
						/* */
					}
					unlistenKitty = undefined;
					try {
						unlistenOsc133?.();
					} catch {
						/* */
					}
					unlistenOsc133 = undefined;
					try {
						unlistenTitle?.();
					} catch {
						/* */
					}
					unlistenTitle = undefined;
					try {
						unlistenClipboardStore?.();
					} catch {
						/* */
					}
					unlistenClipboardStore = undefined;
				}
			}
			if (!reconnected) {
				appLogger.debug("terminal", `initSession(${props.id}) — creating FRESH PTY session (no prior sessionId)`);
				const termData = terminalsStore.get(props.id);
				sessionId = await pty.createSession({
					rows: grid.rows,
					cols: grid.cols,
					shell: settingsStore.state.shell ?? null,
					cwd: props.cwd || null,
					tuic_session: termData?.tuicSession ?? null,
					env: agentConfigsStore.getEnvFlags("claude"),
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
	createEffect(() => {
		if (isVisible()) {
			rafHandle = requestAnimationFrame(() => {
				rafHandle = 0;
				if (!containerRef || containerRef.offsetWidth <= 0 || containerRef.offsetHeight <= 0) {
					// Container not ready — retry on next frame
					const retry = () => {
						requestAnimationFrame(() => {
							if (containerRef && containerRef.offsetWidth > 0 && containerRef.offsetHeight > 0) {
								initSession();
							} else {
								retry();
							}
						});
					};
					retry();
					return;
				}
				initSession();

				if (terminalsStore.state.activeId === props.id) {
					canvasTerminalRef()?.focus();
				}
			});

			onCleanup(() => {
				if (rafHandle) cancelAnimationFrame(rafHandle);
			});
		}
	});

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
		const safeUnlisten = (fn: (() => void) | undefined) => {
			try {
				fn?.();
			} catch {
				/* listener already gone */
			}
		};
		safeUnlisten(unsubscribePty);
		unsubscribePty = undefined;
		safeUnlisten(unlistenParsed);
		unlistenParsed = undefined;
		safeUnlisten(unlistenKitty);
		unlistenKitty = undefined;
		safeUnlisten(unlistenOsc133);
		unlistenOsc133 = undefined;
		safeUnlisten(unlistenTitle);
		unlistenTitle = undefined;
		safeUnlisten(unlistenClipboardStore);
		unlistenClipboardStore = undefined;
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
			const ref = canvasTerminalRef();
			if (ref) ref.focus();
			else pendingCanvasFocus = true;
		},
		getSessionId: () => sessionId,
		openSearch: () => setSearchVisible(true),
		closeSearch: () => setSearchVisible(false),
		toggleCompose: () => {
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
			setComposeOpen(true);
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

	// Re-register ref and resubscribe to grid channel when this terminal becomes
	// visible again (e.g. after reattach from a floating window whose Terminal
	// overwrote the grid channel subscription and ref in the store).
	createEffect((prev: boolean) => {
		const vis = isVisible();
		if (vis && prev === false) {
			terminalsStore.update(props.id, { ref: refMethods });
			requestAnimationFrame(() => {
				requestAnimationFrame(() => {
					const ref = canvasTerminalRef();
					if (ref) {
						ref.resubscribe().then(() => ref.refresh());
					}
				});
			});
		}
		return vis;
	}, false);

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

	const handleDismissResume = (e: MouseEvent) => {
		e.preventDefault();
		e.stopPropagation();
		terminalsStore.update(props.id, { pendingResumeCommand: null });
		canvasTerminalRef()?.focus();
	};

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
		const quoted = `'${path.replace(/'/g, "'\\''")}' `;
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
				canvasRef={canvasTerminalRef()}
				onClose={() => {
					setSearchVisible(false);
					canvasTerminalRef()?.focus();
				}}
			/>
			<Show
				when={
					settingsStore.state.showLastPrompt &&
					terminalsStore.get(props.id)?.agentType &&
					terminalsStore.get(props.id)?.lastPrompt
				}
			>
				<LastPromptBar prompt={() => terminalsStore.get(props.id)?.lastPrompt ?? null} />
			</Show>
			<Show when={reconnecting()}>
				{(info) => (
					<div class={s.reconnectBanner}>
						Reconnecting ({info().attempt}/{info().max})...
					</div>
				)}
			</Show>
			<Show when={terminalsStore.get(props.id)?.pendingResumeCommand}>
				<div class={s.resumeBanner} onClick={handleResume}>
					<span>Agent session was active — click to resume</span>
					<button class={s.resumeDismiss} onClick={handleDismissResume} title="Dismiss">
						&times;
					</button>
				</div>
			</Show>
			<div ref={containerRef} class={s.content}>
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
				<Show keyed when={_currentSessionId()}>
					{(sid) => (
						<CanvasTerminal
							sessionId={sid}
							terminalId={props.id}
							onOpenFilePath={props.onOpenFilePath}
							onSearchOpen={() => setSearchVisible(true)}
							onSearchClose={() => setSearchVisible(false)}
							searchVisible={searchVisible()}
							onResume={handleResume}
							onResumeDismiss={() => terminalsStore.update(props.id, { pendingResumeCommand: null })}
							hasPendingResume={!!terminalsStore.get(props.id)?.pendingResumeCommand}
							onFocus={() => props.onFocus?.(props.id)}
							onCwdChange={props.onCwdChange}
							onRef={(ref) => {
								setCanvasTerminalRef(ref);
								if (pendingCanvasFocus) {
									pendingCanvasFocus = false;
									ref.focus();
								}
							}}
							onBell={handleBell}
						/>
					)}
				</Show>
			</div>
			<Show when={!composeOpen()}>
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
			<ComposePanel
				isOpen={composeOpen}
				initialText={pendingComposeText}
				onTextChange={setPendingComposeText}
				onClose={() => {
					setComposeOpen(false);
					canvasTerminalRef()?.focus();
				}}
				onSend={async (text) => {
					if (sessionId) {
						try {
							const term = terminalsStore.get(props.id);
							await pty.sendCommand(sessionId, text, term?.agentType);
							setPendingComposeText("");
							setComposeOpen(false);
							canvasTerminalRef()?.focus();
						} catch (err) {
							appLogger.error("terminal", "ComposePanel send failed", { sessionId, error: err });
						}
					} else {
						setPendingComposeText("");
						setComposeOpen(false);
						canvasTerminalRef()?.focus();
					}
				}}
			/>
		</div>
	);
};

export default Terminal;
