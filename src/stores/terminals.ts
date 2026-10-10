import { batch, untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import type { AgentType } from "../agents";
import { rpc } from "../transport";
import { getRepoConnection, setSessionConnectionLookup } from "../transportRuntime";
import type { TerminalMatch } from "../types";
import { isPerfDebug } from "../utils/perfDebug";
import { appLogger } from "./appLogger";
import { settingsStore } from "./settings";
import { activatePaneExclusively, registerPaneDeactivator, tabOrderingStore } from "./tabManager";

/** Type of input being awaited */
export type AwaitingInputType = "question" | "error" | null;

/** A completed or in-progress command block detected via OSC 133 shell integration. */
export interface CommandBlock {
	/** Prompt start marker line (OSC 133;A) */
	promptLine: number;
	/** Command text start marker line (OSC 133;B) — set when user finishes typing */
	commandLine: number | null;
	/** Execution start marker line (OSC 133;C) — set when command is submitted */
	executionLine: number | null;
	/** Command end marker line (OSC 133;D) — set when command exits */
	endLine: number | null;
	/** Exit code from OSC 133;D;exitcode */
	exitCode: number | null;
	/** Timestamp when the block started (prompt appeared) */
	startedAt: number;
	/** Timestamp when the command finished */
	endedAt: number | null;
}

/** Shell activity state: null=never had output, busy=producing output, idle=waiting for input, exited=process terminated */
export type ShellState = "busy" | "idle" | "exited" | null;
/** Authoritative task lifecycle from the backend; distinct from PTY activity. */
export type TerminalViewMode = "cli" | "chat";

export type AgentLifecycleState = "starting" | "working" | "awaiting_input" | "idle" | "completed" | null;

const VALID_SHELL_STATES = new Set<string>(["busy", "idle", "exited"]);

/** Type guard for ShellState values received from backend */
export function isShellState(value: unknown): value is ShellState {
	return value === null || (typeof value === "string" && VALID_SHELL_STATES.has(value));
}

/** Terminal pane data (without DOM references for serialization) */
export interface TerminalData {
	id: string;
	sessionId: string | null;
	fontSize: number;
	name: string;
	nameIsCustom: boolean; // When true, OSC/status-line title changes are ignored
	/** The name came from an explicit spawn name. OSC 0/2 titles leave it alone;
	 *  an `intent:` title and a user rename may still replace it. */
	nameFromSpawn: boolean;
	cwd: string | null;
	/**
	 * The registered repo that owns this terminal, resolved from `cwd`.
	 *
	 * The sidebar renders a terminal by its membership in
	 * `repos[X].workspaces[Y].terminals[]`, which made that array the ONLY record of
	 * a terminal's repo — so a wrong placement was unrecoverable and a dangling id
	 * could never be reconciled. This field is the record; the arrays are a display
	 * index derived from it.
	 *
	 * `null` means no registered repo owns the cwd. Such a terminal is parked in
	 * the active repo so it stays visible, and `reconcileTerminalOwnership` moves it
	 * home as soon as a repo claims the path.
	 */
	repoPath: string | null;
	/** Backend-authored worktree placement, independent of the shell cwd. */
	placementPath?: string | null;
	awaitingInput: AwaitingInputType;
	awaitingInputConfident: boolean; // High-confidence detection — don't clear on idle→busy
	activity: boolean;
	unseen: boolean; // Terminal completed work while user wasn't viewing it
	progress: number | null; // OSC 9;4 progress (0-100), null when inactive
	shellState: ShellState;
	/** Monotonic local event revision; prevents a stale session snapshot from overwriting PTY state. */
	shellStateRevision: number;
	agentState: AgentLifecycleState;
	backgroundWork: boolean;
	queuedCommands: number; // Compose-panel commands waiting for the agent's next idle window
	completionNotified: boolean; // Current busy cycle already produced its completion notification
	agentType: AgentType | null; // Detected foreground agent process (e.g. "claude")
	// Command used to launch the agent, for accurate resume. Set from the run config
	// at launch (e.g. "c"), then upgraded by discovery to what the process really runs
	// (e.g. "CLAUDE_CONFIG_DIR=/Users/me/.claude-private claude --dangerously-skip-permissions"),
	// because a shell alias hides both the binary and the env that locates the session.
	agentLaunchCommand: string | null;
	pendingResumeCommand: string | null; // Set at restore time, consumed on first shell idle
	pendingInitCommand: string | null; // Setup/run script to auto-execute on first shell idle
	usageLimit: { percentage: number; limitType: string } | null; // Claude Code usage limit
	lastDataAt: number | null; // Timestamp of last PTY output
	lastActivityAt: number | null; // Backend timestamp of last semantic session activity
	idleSince: number | null; // Timestamp when shellState transitioned to idle
	lastPrompt: string | null; // Last relevant user prompt (>= 10 words), set by Rust
	ptyDescription: string | null; // Orchestrator-supplied description of assigned PTY work
	agentIntent: string | null; // LLM-declared intent via intent: token
	currentTask: string | null; // Current agent task from status-line parsing (e.g. "Reading files")
	activeSubTasks: number; // Count of running sub-agents/background tasks from ›› status line
	isRemote: boolean; // Created via HTTP/MCP (not locally by the UI)
	agentSessionId: string | null; // Discovered agent session ID for exact resume (claude, gemini, codex, grok)
	tuicSession: string | null; // Stable tab UUID — injected as TUIC_SESSION env var, persists across restarts
	suggestedActions: string[] | null; // Follow-up suggestions from suggest: token
	suggestDismissed: boolean; // true after user dismissed/selected/typed — resets on shell-state:idle
	historyBase: number; // Evicted rows in the latest grid frame; stored markers use all-time rows.
	commandBlocks: CommandBlock[]; // Completed command blocks from OSC 133
	activeBlock: CommandBlock | null; // Current in-progress block (A received, D not yet)
	lastCommandExecAt: number | null; // Timestamp of last OSC 133 "C" (real command execution); monotonic, never evicted — used to gate completion against prompt-redraw/wake false-busy
	foldedBlocks: Set<number>; // promptLine values of folded blocks
	userPromptLines: number[]; // Absolute lines where the user submitted a prompt (UserInput.line), for the green scrollbar marker
	answersOnly: boolean; // Answers-only view: output of the last turn is collapsed except 💬 answer lines. Never automatic.
	/** Which view of an agent terminal is shown. "chat" hides the grid, it never unmounts it. Never persisted. */
	viewMode: TerminalViewMode;
	alias: string | null; // Human-friendly alias from Rust (e.g. "tc-1")
	standby: boolean; // Session is SIGSTOP'd (auto-standby)
	/** PTY and agent were ended on purpose (Suspend); the tab is kept and resumes like a restored one. */
	suspended: boolean;
	parentSession: string | null; // Session or TUIC_SESSION of the agent that spawned this one (sub-agent PTY)
}

/** Fields auto-populated with defaults when creating a terminal — callers only provide the remaining fields. */
type TerminalCreateData = Omit<
	TerminalData,
	| "id"
	| "activity"
	| "unseen"
	| "progress"
	| "shellState"
	| "shellStateRevision"
	| "agentState"
	| "backgroundWork"
	| "queuedCommands"
	| "completionNotified"
	| "nameIsCustom"
	| "nameFromSpawn"
	| "agentType"
	| "agentLaunchCommand"
	| "pendingResumeCommand"
	| "pendingInitCommand"
	| "usageLimit"
	| "lastDataAt"
	| "lastActivityAt"
	| "idleSince"
	| "lastPrompt"
	| "ptyDescription"
	| "agentIntent"
	| "currentTask"
	| "activeSubTasks"
	| "isRemote"
	| "agentSessionId"
	| "tuicSession"
	| "suggestedActions"
	| "suggestDismissed"
	| "awaitingInputConfident"
	| "historyBase"
	| "commandBlocks"
	| "activeBlock"
	| "lastCommandExecAt"
	| "foldedBlocks"
	| "userPromptLines"
	| "answersOnly"
	| "viewMode"
	| "alias"
	| "standby"
	| "suspended"
	| "repoPath"
	| "parentSession"
> & {
	repoPath?: string | null;
	parentSession?: string | null;
	tuicSession?: string | null;
	alias?: string | null;
	isRemote?: boolean;
	nameIsCustom?: boolean;
	nameFromSpawn?: boolean;
	agentType?: AgentType | null;
	agentSessionId?: string | null;
	agentLaunchCommand?: string | null;
	ptyDescription?: string | null;
};

/** Terminal component ref interface */
export interface TerminalRef {
	fit: () => void;
	write: (data: string) => void;
	writeln: (data: string) => void;
	input: (data: string) => void;
	clear: () => void;
	refresh: () => void;
	focus: () => void;
	getSessionId: () => string | null;
	openSearch: () => void;
	closeSearch: () => void;
	toggleCompose: () => void;
	openComposeWithText: (text: string) => void;
	/** Search the terminal buffer for a query string (case-insensitive) */
	searchBuffer: (query: string) => TerminalMatch[] | Promise<TerminalMatch[]>;
	/** Scroll to an absolute buffer line index (centered in viewport) */
	scrollToLine: (lineIndex: number) => void;
	getSelection: () => string;
	scrollToTop: () => void;
	scrollToBottom: () => void;
	scrollPages: (pages: number) => void;
	/** Read buffer lines between two absolute line indices (exclusive end) */
	getBufferLines: (startLine: number, endLine: number) => string[] | Promise<string[]>;
	/** Paste text into the terminal, applying bracketed paste wrapping based on terminal state */
	paste: (text: string) => void;
}

/** Combined terminal state */
export interface TerminalState extends TerminalData {
	ref?: TerminalRef;
}

/** Terminals store state */
interface TerminalsStoreState {
	terminals: Record<string, TerminalState>;
	activeId: string | null;
	/** Last non-null activeId — survives tab switches so non-terminal UI can find the right terminal. */
	lastActiveId: string | null;
	/**
	 * Distinct activated terminals, most recent first (max HISTORY_LIMIT). The single source
	 * for "return to last terminal" and history back/forward.
	 */
	history: string[];
	/** Position of the terminal currently shown within `history` (0 = most recent; >0 after stepping back). */
	historyCursor: number;
	counter: number;
	/** Tabs currently detached to floating windows: tabId → window label */
	detachedWindows: Record<string, string>;
	/** Debounced busy state per terminal — stays true for BUSY_HOLD_MS after idle */
	debouncedBusy: Record<string, boolean>;
}

/** How many recently activated terminals the history keeps. */
const HISTORY_LIMIT = 10;

/** Debounce hold time: how long isBusy() stays true after shellState goes idle */
const BUSY_HOLD_MS = 2000;

/** Create the terminals store */
function createTerminalsStore() {
	const [state, setState] = createStore<TerminalsStoreState>({
		terminals: {},
		activeId: null,
		lastActiveId: null,
		history: [],
		historyCursor: 0,
		counter: 0,
		detachedWindows: {},
		debouncedBusy: {},
	});

	// Reverse map: session_id → terminal_id for O(1) hot-path lookups.
	// Plain JS (not in SolidJS store) — maintained in sync with sessionId field.
	const sessionToTerminal = new Map<string, string>();

	// Session ids whose `term-alias-assigned` event arrived before the session
	// was bound to a terminal (add/register/setSessionId establish the bind).
	// The backend assigns the alias the instant the PTY spawns, while the
	// frontend still awaits session creation before binding it — so the event
	// can beat the bind. Retained here and applied the instant the bind exists.
	const pendingAliases = new Map<string, string>();

	/** Apply a retained alias for `sessionId` to `termId`, if one arrived early. */
	function consumePendingAlias(termId: string, sessionId: string): void {
		const alias = pendingAliases.get(sessionId);
		if (alias === undefined) return;
		pendingAliases.delete(sessionId);
		setState("terminals", termId, "alias", alias);
	}

	// Debounced busy tracking: timers + timestamps are plain JS, the boolean state
	// lives in the SolidJS store (state.debouncedBusy) for reactivity.
	const busySinceMap = new Map<string, number>();
	const busyDurationMap = new Map<string, number>();
	const cooldownTimers = new Map<string, ReturnType<typeof setTimeout>>();
	const busyToIdleCallbacks: Array<(id: string, durationMs: number) => void> = [];
	const idleToBusyCallbacks: Array<(id: string) => void> = [];
	const onRemoveCallbacks: Array<(id: string) => void> = [];
	const shellExitCallbacks: Array<(id: string) => void> = [];
	// Tracks which terminals have completed their initial shell startup (reached idle at least once).
	// Used to distinguish "busy from .zshrc startup" from "busy from a user-launched process".
	const reachedIdleSet = new Set<string>();

	// OSC133 block-completed batching: during burst replay (e.g. session restore),
	// hundreds of D markers arrive in a single event loop tick. Buffer completed
	// blocks and flush once per animation frame to avoid N separate setState calls.
	const _osc133Pending = new Map<string, CommandBlock[]>();
	const _osc133FlushTimers = new Map<string, number>();
	const MAX_BLOCKS = 500;

	function _scheduleOsc133Flush(id: string): void {
		if (_osc133FlushTimers.has(id)) return;
		_osc133FlushTimers.set(
			id,
			requestAnimationFrame(() => {
				_osc133FlushTimers.delete(id);
				const pending = _osc133Pending.get(id);
				if (!pending?.length) return;
				_osc133Pending.delete(id);
				// Defence-in-depth: never write to a removed terminal — setState on a
				// missing key would resurrect it as a ghost entry.
				if (!(id in state.terminals)) return;
				batch(() => {
					setState("terminals", id, "commandBlocks", (prev) => {
						const next = [...prev, ...pending];
						if (next.length <= MAX_BLOCKS) return next;
						const evicted = next.slice(0, next.length - MAX_BLOCKS);
						const evictedLines = new Set(evicted.map((b) => b.promptLine));
						setState("terminals", id, "foldedBlocks", (folds) => {
							const cleaned = new Set(folds);
							for (const line of evictedLines) cleaned.delete(line);
							return cleaned;
						});
						return next.slice(-MAX_BLOCKS);
					});
				});
				appLogger.debug(
					"terminal",
					`[OSC133] ${id} flushed ${pending.length} blocks, total=${state.terminals[id]?.commandBlocks.length ?? 0}`,
				);
			}) as unknown as number,
		);
	}

	/** Guard: check terminal exists before mutating. SolidJS setState creates keys
	 *  implicitly — calling setState("terminals", id, ...) on a removed terminal
	 *  resurrects it as a ghost entry with partial data. */
	function has(id: string): boolean {
		return id in state.terminals;
	}

	/** Handle shellState transition for debounced busy tracking */
	function handleShellStateChange(id: string, prev: ShellState, next: ShellState): void {
		if (next === "busy" && prev !== "busy") {
			// Entering busy: clear any cooldown, mark busy, record start time.
			// If a cooldown was active, this is a continuation of the same busy period
			// (e.g. shell prompt redraw after agent exit) — keep the original start time.
			const existingCooldown = cooldownTimers.get(id);
			const hadCooldown = existingCooldown != null;
			if (hadCooldown) {
				clearTimeout(existingCooldown);
				cooldownTimers.delete(id);
				appLogger.debug("terminal", `[ShellDebounce] ${id} cooldown cancelled (re-entered busy)`);
			}
			setState("debouncedBusy", id, true);
			setState("terminals", id, "idleSince", null);
			if (!hadCooldown) {
				busySinceMap.set(id, Date.now());
				for (const cb of idleToBusyCallbacks) cb(id);
			}
			busyDurationMap.delete(id);
			// Error state is NOT cleared here: API errors are persistent and should only
			// be cleared by explicit agent activity (status-line, user-input) or process exit.
			// Low-confidence question state IS cleared: idle→busy means the agent resumed
			// work, so any pending (heuristic) question notification is stale.
			// CONFIDENT questions are preserved (awaitingInputConfident): a confident prompt
			// like an Ink "Enter to select" menu stays awaiting even while the TUI repaints
			// (cursor blink, animation, scrollbar) — those repaints oscillate idle→busy and
			// would otherwise wrongly clear a genuine interactive prompt. Confident questions
			// instead clear on real user-input (Terminal.tsx) or process exit (below) —
			// the same policy the backend applies in state.rs (`question_confident` is
			// sticky across busy status-line ticks, cleared by the user-input arm).
			if (prev === "idle" && state.terminals[id]?.awaitingInput && !state.terminals[id]?.awaitingInputConfident) {
				setState("terminals", id, "awaitingInput", null);
				setState("terminals", id, "awaitingInputConfident", false);
			}
		} else if (next === "idle" && prev !== "busy") {
			// Direct null→idle (e.g. Rust sync on tab switch) — mark startup complete
			reachedIdleSet.add(id);
			if (!state.terminals[id]?.idleSince) setState("terminals", id, "idleSince", Date.now());
		} else if (next === null && state.terminals[id]?.awaitingInput) {
			// Process exit (shellState reset to null) — clear any stuck error/question
			// badge so the next session doesn't inherit stale state from the last child.
			setState("terminals", id, "awaitingInput", null);
			setState("terminals", id, "awaitingInputConfident", false);
		} else if (next !== "busy" && prev === "busy") {
			// First idle marks shell startup as complete
			if (next === "idle") reachedIdleSet.add(id);
			setState("terminals", id, "idleSince", Date.now());
			// Leaving busy: freeze duration, start cooldown
			const since = busySinceMap.get(id);
			const duration = since != null ? Date.now() - since : 0;
			busyDurationMap.set(id, duration);
			if (isPerfDebug()) {
				appLogger.debug("terminal", `[ShellDebounce] ${id} busy→idle cooldown=${BUSY_HOLD_MS}ms dur=${duration}ms`);
			}

			const timer = setTimeout(() => {
				cooldownTimers.delete(id);
				setState("debouncedBusy", id, false);
				appLogger.debug("terminal", `[ShellDebounce] ${id} cooldown expired`);
				for (const cb of busyToIdleCallbacks) cb(id, duration);
			}, BUSY_HOLD_MS);
			cooldownTimers.set(id, timer);
		}
	}

	// Non-reactive map for lastDataAt timestamps — avoids triggering the reactive
	// graph on every PTY output (was 1 store write per second per active terminal).
	// ActivityDashboard reads via getLastDataAt(); flushLastDataAt() syncs to store.
	const lastDataAtMap = new Map<string, number>();
	let lastDataAtFlushTimer: ReturnType<typeof setInterval> | null = null;

	function startLastDataAtFlush(): void {
		if (lastDataAtFlushTimer) return;
		lastDataAtFlushTimer = setInterval(() => {
			// Stop the interval once no terminal is pushing lastDataAt updates.
			// A fresh push via setLastDataAt() restarts it. Prevents a 5s timer from
			// ticking forever after the user closes every terminal.
			if (lastDataAtMap.size === 0) {
				if (lastDataAtFlushTimer) {
					clearInterval(lastDataAtFlushTimer);
					lastDataAtFlushTimer = null;
				}
				return;
			}
			batch(() => {
				for (const [id, ts] of lastDataAtMap) {
					if (state.terminals[id]) setState("terminals", id, "lastDataAt", ts);
				}
			});
		}, 5000);
	}

	/** Clean up debounced busy state for a removed terminal */
	function cleanupBusyState(id: string): void {
		const timer = cooldownTimers.get(id);
		if (timer != null) clearTimeout(timer);
		cooldownTimers.delete(id);
		reachedIdleSet.delete(id);
		setState(
			produce((s) => {
				delete s.debouncedBusy[id];
			}),
		);
		busySinceMap.delete(id);
		busyDurationMap.delete(id);
	}

	const actions = {
		/** Add a new terminal */
		add(data: TerminalCreateData): string {
			const id = `term-${state.counter + 1}`;
			setState("counter", (c) => c + 1);
			setState("terminals", id, {
				id,
				activity: false,
				unseen: false,
				progress: null,
				shellState: null,
				shellStateRevision: 0,
				agentState: null,
				backgroundWork: false,
				queuedCommands: 0,
				completionNotified: false,
				nameIsCustom: false,
				nameFromSpawn: false,
				agentType: null,
				agentLaunchCommand: null,
				pendingResumeCommand: null,
				pendingInitCommand: null,
				usageLimit: null,
				lastDataAt: null,
				lastActivityAt: null,
				idleSince: null,
				lastPrompt: null,
				ptyDescription: null,
				agentIntent: null,
				currentTask: null,
				activeSubTasks: 0,
				isRemote: false,
				agentSessionId: null,
				tuicSession: null,
				suggestedActions: null,
				suggestDismissed: false,
				awaitingInputConfident: false,
				historyBase: 0,
				commandBlocks: [],
				activeBlock: null,
				lastCommandExecAt: null,
				foldedBlocks: new Set<number>(),
				userPromptLines: [],
				answersOnly: false,
				viewMode: "cli",
				alias: null,
				standby: false,
				suspended: false,
				repoPath: null,
				parentSession: null,
				...data,
			});
			if (data.sessionId) {
				sessionToTerminal.set(data.sessionId, id);
				consumePendingAlias(id, data.sessionId);
			}
			tabOrderingStore.insert(id);
			return id;
		},

		/** Register a terminal with a specific ID (used by floating windows to reconnect to existing PTY sessions) */
		register(id: string, data: TerminalCreateData): void {
			setState("terminals", id, {
				id,
				activity: false,
				unseen: false,
				progress: null,
				shellState: null,
				shellStateRevision: 0,
				agentState: null,
				backgroundWork: false,
				queuedCommands: 0,
				completionNotified: false,
				nameIsCustom: false,
				nameFromSpawn: false,
				agentType: null,
				agentLaunchCommand: null,
				pendingResumeCommand: null,
				pendingInitCommand: null,
				usageLimit: null,
				lastDataAt: null,
				lastActivityAt: null,
				idleSince: null,
				lastPrompt: null,
				ptyDescription: null,
				agentIntent: null,
				currentTask: null,
				activeSubTasks: 0,
				isRemote: false,
				agentSessionId: null,
				tuicSession: null,
				suggestedActions: null,
				suggestDismissed: false,
				awaitingInputConfident: false,
				historyBase: 0,
				commandBlocks: [],
				activeBlock: null,
				lastCommandExecAt: null,
				foldedBlocks: new Set<number>(),
				userPromptLines: [],
				answersOnly: false,
				viewMode: "cli",
				alias: null,
				standby: false,
				suspended: false,
				repoPath: null,
				parentSession: null,
				...data,
			});
			if (data.sessionId) {
				sessionToTerminal.set(data.sessionId, id);
				consumePendingAlias(id, data.sessionId);
			}
		},

		/** Remove a terminal. Sets activeId to null when removing the active terminal —
		 *  the caller is responsible for selecting a same-branch replacement beforehand. */
		remove(id: string): void {
			appLogger.info("terminal", `TermStore.remove(${id})`, {
				remaining: Object.keys(state.terminals).filter((k) => k !== id),
			});
			const sessionId = state.terminals[id]?.sessionId;
			if (sessionId) sessionToTerminal.delete(sessionId);
			tabOrderingStore.remove(id);
			cleanupBusyState(id);
			lastDataAtMap.delete(id);
			// Cancel any pending OSC 133 flush so its rAF callback can't fire after
			// removal and resurrect a ghost terminal entry via setState("terminals", id, ...).
			const osc133Raf = _osc133FlushTimers.get(id);
			if (osc133Raf != null) cancelAnimationFrame(osc133Raf);
			_osc133FlushTimers.delete(id);
			_osc133Pending.delete(id);
			for (const cb of onRemoveCallbacks) cb(id);
			setState(
				produce((s) => {
					delete s.terminals[id];
					delete s.detachedWindows[id];
					if (s.activeId === id) {
						s.activeId = null;
					}
					if (s.lastActiveId === id) {
						s.lastActiveId = null;
					}
					const at = s.history.indexOf(id);
					if (at !== -1) {
						s.history.splice(at, 1);
						if (at < s.historyCursor) s.historyCursor--;
						s.historyCursor = Math.max(0, Math.min(s.historyCursor, s.history.length - 1));
					}
				}),
			);
		},

		/** Set the active terminal (clears unread activity indicator, preserves shell state) */
		setActive(id: string | null): void {
			if (id) {
				if (!state.terminals[id]) {
					appLogger.warn("terminal", `setActive(${id}) — terminal not in store, ignoring`);
					return;
				}
			}
			const prevId = state.activeId;
			batch(() => {
				if (id) {
					// The terminal pane is now the only one showing — see
					// activatePaneExclusively. Without this, opening a terminal on top of
					// an active editor/diff/markdown tab left both panes rendered.
					activatePaneExclusively("terminals");
					setState("terminals", id, "activity", false);
					setState("terminals", id, "unseen", false);
					setState("lastActiveId", id);
					// Walking the history lands on the entry under the cursor: keep order.
					// Any other activation drops the forward part and moves to the front.
					if (state.history[state.historyCursor] !== id) {
						setState(
							"history",
							[id, ...state.history.slice(state.historyCursor).filter((h) => h !== id)].slice(0, HISTORY_LIMIT),
						);
						setState("historyCursor", 0);
					}
				}
				setState("activeId", id);
			});
			if (prevId && prevId !== id) {
				rpc("set_session_visible", { sessionId: prevId, visible: false }).catch(() => {});
			}
			if (id) {
				rpc("set_session_visible", { sessionId: id, visible: true }).catch(() => {});
			}
		},

		/** Update terminal data */
		update(id: string, data: Partial<TerminalState>): void {
			if (!state.terminals[id]) {
				appLogger.warn("terminal", `update(${id}) — terminal not in store, ignoring`);
				return;
			}
			batch(() => {
				if ("shellState" in data) {
					const prev = state.terminals[id]?.shellState ?? null;
					const next = data.shellState ?? null;
					if (prev !== next) handleShellStateChange(id, prev, next);
					setState("terminals", id, "shellStateRevision", (revision) => revision + 1);
				}
				// Agent exited back to a plain shell (agentType set→null). Any pending
				// question/error was the agent's — a shell can't be "awaiting input" on
				// its behalf, and nothing else clears it: Ctrl+C emits no UserInput, the
				// PTY shell never "exits" (so the shellState→null clear never fires), and
				// confident questions deliberately survive idle→busy. A stale badge would
				// otherwise stick forever and hijack repo-switch routing (useGitOperations
				// prefers a tab with awaitingInput). Parallels the process-exit clear above.
				if ("agentType" in data) {
					const prevAgent = state.terminals[id]?.agentType ?? null;
					const nextAgent = data.agentType ?? null;
					if (prevAgent !== null && nextAgent === null && state.terminals[id]?.awaitingInput) {
						setState("terminals", id, "awaitingInput", null);
						setState("terminals", id, "awaitingInputConfident", false);
					}
				}
				// Keep sessionToTerminal reverse map in sync: callers that pass sessionId
				// via update() would otherwise desync the map and break plugin filtering
				// (pluginMatchesSession → getAgentTypeForSession → null → plugin starved).
				if ("sessionId" in data) {
					const prev = state.terminals[id]?.sessionId;
					if (prev) sessionToTerminal.delete(prev);
					const next = data.sessionId ?? null;
					if (next) sessionToTerminal.set(next, id);
					if (!next) {
						// Lifecycle is scoped to a live backend session. Clearing it here
						// prevents an exited agent tab from retaining a stale Working badge
						// after it can no longer appear in the next session-list snapshot.
						setState("terminals", id, "agentState", null);
						setState("terminals", id, "backgroundWork", false);
						// The backend drops the queue with the session; a lingering count
						// would offer to clear commands that no longer exist.
						setState("terminals", id, "queuedCommands", 0);
					}
				}
				setState("terminals", id, data);
			});
			// Sync display name and its origin so reconnect can distinguish a
			// user-protected rename from a transient OSC/intent title.
			if ("name" in data) {
				const sessionId = state.terminals[id]?.sessionId;
				if (sessionId) {
					rpc("set_session_name", {
						sessionId,
						name: data.name ?? null,
						isCustom: state.terminals[id]?.nameIsCustom ?? false,
					}).catch(() => {});
				}
			}
		},

		/** Update session ID */
		setSessionId(id: string, sessionId: string | null): void {
			if (!has(id)) return;
			const prev = state.terminals[id]?.sessionId;
			if (prev) sessionToTerminal.delete(prev);
			if (sessionId) sessionToTerminal.set(sessionId, id);
			batch(() => {
				setState("terminals", id, "sessionId", sessionId);
				if (sessionId) consumePendingAlias(id, sessionId);
			});
		},

		/** Apply a backend-assigned alias for a PTY session. The race-safe entry
		 *  point for `term-alias-assigned`: if the session isn't bound to a
		 *  terminal yet (setSessionId/add/register haven't run), the alias is
		 *  retained and applied the instant the bind is made, instead of being
		 *  silently dropped. Callers should use this instead of resolving the
		 *  terminal id themselves via getTerminalForSession. */
		applyAlias(sessionId: string, alias: string): void {
			const termId = sessionToTerminal.get(sessionId);
			if (termId && has(termId)) {
				setState("terminals", termId, "alias", alias);
				return;
			}
			pendingAliases.set(sessionId, alias);
		},

		/** Drop an alias retained for a session that closed before any tab bound it. */
		forgetPendingAlias(sessionId: string): void {
			pendingAliases.delete(sessionId);
		},

		/** Record the repo that owns this terminal (null = no registered repo does).
		 *  Set from the resolver at assignment time; never from the focused repo. */
		setRepoPath(id: string, repoPath: string | null): void {
			if (!has(id)) return;
			setState("terminals", id, "repoPath", repoPath);
		},

		/** Update last relevant user prompt */
		setLastPrompt(id: string, prompt: string | null): void {
			if (!has(id)) return;
			setState("terminals", id, "lastPrompt", prompt);
		},

		/** Update the orchestrator-supplied PTY task description. */
		setPtyDescription(id: string, description: string | null): void {
			if (!has(id)) return;
			setState("terminals", id, "ptyDescription", description);
		},

		/** Set suggested follow-up actions (timer-free — overlay handles visibility timeout) */
		setSuggestedActions(id: string, items: string[]): void {
			if (!has(id)) return;
			setState("terminals", id, "suggestedActions", items);
		},

		/** Dismiss suggested actions for a specific terminal */
		dismissSuggestedActions(id: string): void {
			if (!has(id)) return;
			setState("terminals", id, "suggestedActions", null);
			setState("terminals", id, "suggestDismissed", true);
		},

		/** OSC 133: Handle shell integration marker.
		 *  A=prompt start, B=command start, C=pre-execution, D=command finished.
		 *  `line` is the eviction-stable all-time row (total_scrolled + cursorY) when the marker was processed. */
		handleOsc133(id: string, type: string, line: number, exitCode?: number): void {
			const term = state.terminals[id];
			if (!term) return;
			const now = Date.now();

			switch (type) {
				case "A": {
					// Prompt start — begin a new block. If there's already an active block
					// without a D marker (e.g. Ctrl+C), finalize it first.
					if (term.activeBlock) {
						const completed: CommandBlock = { ...term.activeBlock, endedAt: now };
						_osc133Pending.get(id)?.push(completed) ?? _osc133Pending.set(id, [completed]);
						_scheduleOsc133Flush(id);
					}
					setState("terminals", id, "activeBlock", {
						promptLine: line,
						commandLine: null,
						executionLine: null,
						endLine: null,
						exitCode: null,
						startedAt: now,
						endedAt: null,
					});
					break;
				}
				case "B": {
					if (term.activeBlock) {
						setState("terminals", id, "activeBlock", { ...term.activeBlock, commandLine: line });
					}
					break;
				}
				case "C": {
					if (term.activeBlock) {
						setState("terminals", id, "activeBlock", { ...term.activeBlock, executionLine: line });
					}
					// Record that a real command executed (not a bare prompt redraw).
					// Monotonic and never evicted, so onBusyToIdle can tell genuine work
					// apart from sleep/wake / prompt-redraw false-busy.
					setState("terminals", id, "lastCommandExecAt", now);
					break;
				}
				case "D": {
					if (term.activeBlock) {
						const completed: CommandBlock = {
							...term.activeBlock,
							endLine: line,
							exitCode: exitCode ?? null,
							endedAt: now,
						};
						_osc133Pending.get(id)?.push(completed) ?? _osc133Pending.set(id, [completed]);
						_scheduleOsc133Flush(id);
						setState("terminals", id, "activeBlock", null);
					}
					break;
				}
			}
		},

		/** Record an absolute line where the user submitted a prompt (UserInput.line
		 *  from the OSC 7770 state=prompt hook). Drives the green scrollbar marker.
		 *  Ignores negative lines (keystroke-reconstructed UserInput has no grid row)
		 *  and dedups consecutive repeats (a prompt redraw can re-fire busy on the same
		 *  row). Capped at MAX_BLOCKS, mirroring commandBlocks eviction. */
		addUserPromptLine(id: string, line: number): void {
			if (!has(id) || line < 0) return;
			setState("terminals", id, "userPromptLines", (prev) => {
				if (prev.at(-1)! === line) return prev;
				const next = [...prev, line];
				return next.length > MAX_BLOCKS ? next.slice(-MAX_BLOCKS) : next;
			});
		},

		/** Fold or unfold one command block.
		 *
		 *  The `blockFoldingEnabled` gate lives here rather than in the callers
		 *  because there are two of them — CanvasTerminal's own Cmd+Shift+.
		 *  branch and `toggleNearestCommandBlock` behind the `block-fold-toggle`
		 *  action (shortcut and command palette) — and only the first used to
		 *  check it, so the Settings toggle appeared to do nothing. One check at
		 *  the point both paths converge on cannot be forgotten by a third.
		 *
		 *  Blocks folded before the setting was turned off stay folded: this
		 *  disables the control, and silently expanding history on a settings
		 *  change is the worse surprise. */
		toggleBlockFold(id: string, promptLine: number): void {
			if (!settingsStore.state.blockFoldingEnabled) return;
			const term = state.terminals[id];
			if (!term) return;
			const next = new Set(term.foldedBlocks);
			if (next.has(promptLine)) {
				next.delete(promptLine);
			} else {
				next.add(promptLine);
			}
			setState("terminals", id, "foldedBlocks", next);
		},

		/** Flip the answers-only view of one terminal (button / `answers-only` action). */
		toggleAnswersOnly(id: string | null = state.activeId): void {
			const term = id ? state.terminals[id] : undefined;
			if (!id || !term) return;
			setState("terminals", id, "answersOnly", !term.answersOnly);
		},

		/** Switch one terminal between its grid ("cli") and the transcript view ("chat"). */
		setViewMode(id: string, mode: TerminalViewMode): void {
			if (!has(id)) return;
			setState("terminals", id, "viewMode", mode);
		},

		/** Update agent-declared intent (via intent: token) */
		setAgentIntent(id: string, intent: string | null): void {
			if (!has(id)) return;
			setState("terminals", id, "agentIntent", intent);
		},

		/** Update font size (zoom) */
		setFontSize(id: string, fontSize: number): void {
			if (!has(id)) return;
			setState("terminals", id, "fontSize", fontSize);
		},

		/** Set terminal awaiting input state.
		 *
		 *  A confident question is sticky: it is NOT expired by a timer. An Ink
		 *  dialog repaints (spinner, cursor) while it waits for an answer, so
		 *  "the terminal is still producing output" does not mean "the prompt was
		 *  answered". The answer itself is the signal — `user-input` (Terminal.tsx),
		 *  which also covers channel/MCP writes because they reach the PTY through
		 *  the same path — plus process exit and explicit clearAwaitingInput. */
		setAwaitingInput(id: string, type: AwaitingInputType, confident = false): void {
			if (!has(id)) return;
			batch(() => {
				setState("terminals", id, "awaitingInput", type);
				setState("terminals", id, "awaitingInputConfident", confident);
			});
		},

		/** Clear terminal awaiting input state */
		clearAwaitingInput(id: string): void {
			if (!has(id)) return;
			batch(() => {
				setState("terminals", id, "awaitingInput", null);
				setState("terminals", id, "awaitingInputConfident", false);
			});
		},

		/** Check if any terminal is awaiting input */
		hasAwaitingInput(): boolean {
			return Object.values(state.terminals).some((t) => t.awaitingInput !== null);
		},

		/** Get all terminals awaiting input */
		getAwaitingInputIds(): string[] {
			return Object.entries(state.terminals)
				.filter(([_, t]) => t.awaitingInput !== null)
				.map(([id]) => id);
		},

		/** Get terminal by ID */
		get(id: string): TerminalState | undefined {
			return state.terminals[id];
		},

		/** Get the terminal ID for a PTY session, or null if not found */
		getTerminalForSession(sessionId: string): string | null {
			return sessionToTerminal.get(sessionId) ?? null;
		},

		/** Tag for a sub-agent PTY: the spawning agent's tab name, "external agent" when that
		 *  parent has no tab here (external caller, closed tab), null when the
		 *  terminal was not spawned by an agent. Read live, so a parent rename follows. */
		getSubAgentTag(id: string): string | null {
			const parent = state.terminals[id]?.parentSession;
			if (!parent) return null;
			const parentTerm = Object.values(state.terminals).find((t) => t.sessionId === parent || t.tuicSession === parent);
			return parentTerm?.name ?? "external agent";
		},

		/** Get the agentType for a PTY session, or null if not found */
		getAgentTypeForSession(sessionId: string): string | null {
			const termId = sessionToTerminal.get(sessionId);
			if (!termId) return null;
			return state.terminals[termId]?.agentType ?? null;
		},

		/** Get active terminal */
		getActive(): TerminalState | undefined {
			return state.activeId ? state.terminals[state.activeId] : undefined;
		},

		/** The most recently active terminal other than the current one ("return to last terminal"). */
		getPreviousActiveId(): string | null {
			return state.history.find((id) => id !== state.activeId) ?? null;
		},

		/** Terminal one step back / forward in the history, or null at the ends. */
		getHistoryTarget(direction: "back" | "forward"): string | null {
			return state.history[state.historyCursor + (direction === "back" ? 1 : -1)] ?? null;
		},

		/**
		 * Move the history cursor one step and return the terminal to show, or null at the end.
		 * The caller then activates it; setActive recognises it as the cursor entry and keeps order.
		 */
		stepHistory(direction: "back" | "forward"): string | null {
			const step = direction === "back" ? 1 : -1;
			const id = state.history[state.historyCursor + step] ?? null;
			if (id) setState("historyCursor", state.historyCursor + step);
			return id;
		},

		/** Find the best terminal with an active PTY session: active > lastActive > any. */
		findTerminalWithSession(): { sessionId: string; agentType: string | null } | null {
			for (const id of [state.activeId, state.lastActiveId]) {
				if (!id) continue;
				const t = state.terminals[id];
				if (t?.sessionId) return { sessionId: t.sessionId, agentType: t.agentType ?? null };
			}
			for (const id of Object.keys(state.terminals)) {
				const t = state.terminals[id];
				if (t?.sessionId) return { sessionId: t.sessionId, agentType: t.agentType ?? null };
			}
			return null;
		},

		/** Get all terminal IDs */
		getIds(): string[] {
			return Object.keys(state.terminals);
		},

		/** The terminal tab driving a backend PTY session, or undefined once it
		 *  is closed. Backend events carry session ids, tabs are keyed by their
		 *  own id, so every "the backend means this tab" lookup goes through here. */
		findBySessionId(sessionId: string): string | undefined {
			return Object.keys(state.terminals).find((id) => state.terminals[id]?.sessionId === sessionId);
		},

		/** Get terminal count */
		getCount(): number {
			return Object.keys(state.terminals).length;
		},

		/** Default name for the next terminal tab */
		nextDefaultName(): string {
			return `Terminal ${Object.keys(state.terminals).length + 1}`;
		},

		/** Debounced busy: true while shellState is "busy" and for 2s after it transitions to idle */
		isBusy(id: string): boolean {
			return state.debouncedBusy[id] ?? false;
		},

		/** True if the shell has completed its initial startup (reached idle at least once).
		 *  Used to distinguish "busy from .zshrc startup" from "busy from a user-launched process". */
		hasReachedIdle(id: string): boolean {
			return reachedIdleSet.has(id);
		},

		/** True if any terminal has a debounced busy state */
		isAnyBusy(): boolean {
			return Object.values(state.debouncedBusy).some(Boolean);
		},

		/** Duration in ms of the current (or last) busy cycle. 0 if never busy. */
		getBusyDuration(id: string): number {
			// If currently busy (no frozen duration yet), compute live
			const since = busySinceMap.get(id);
			const frozen = busyDurationMap.get(id);
			if (frozen != null) return frozen;
			if (since != null) return Date.now() - since;
			return 0;
		},

		/** Register a callback fired when a terminal transitions from debounced-busy to idle.
		 *  Callback receives (terminalId, busyDurationMs). */
		onBusyToIdle(callback: (id: string, durationMs: number) => void): () => void {
			busyToIdleCallbacks.push(callback);
			return () => {
				const idx = busyToIdleCallbacks.indexOf(callback);
				if (idx >= 0) busyToIdleCallbacks.splice(idx, 1);
			};
		},

		/** Register a callback fired when a terminal transitions from idle to busy (debounced).
		 *  Only fires on genuine new busy cycles — not cooldown re-entries. */
		onIdleToBusy(callback: (id: string) => void): () => void {
			idleToBusyCallbacks.push(callback);
			return () => {
				const idx = idleToBusyCallbacks.indexOf(callback);
				if (idx >= 0) idleToBusyCallbacks.splice(idx, 1);
			};
		},

		/** Register a callback fired when a terminal is removed.
		 *  Used by globalWorkspaceStore to auto-unpromote without direct coupling. */
		onRemove(callback: (id: string) => void): () => void {
			onRemoveCallbacks.push(callback);
			return () => {
				const idx = onRemoveCallbacks.indexOf(callback);
				if (idx >= 0) onRemoveCallbacks.splice(idx, 1);
			};
		},

		/** Register a callback fired when a plain interactive shell (no agent) exits.
		 *  App wires this to close the tab — a dead shell shouldn't linger as a
		 *  grey "exited" dot. Agent sessions are NOT routed here; they keep the tab. */
		onShellExit(callback: (id: string) => void): () => void {
			shellExitCallbacks.push(callback);
			return () => {
				const idx = shellExitCallbacks.indexOf(callback);
				if (idx >= 0) shellExitCallbacks.splice(idx, 1);
			};
		},

		/** Fire the shell-exit callbacks for a terminal whose plain shell ended.
		 *  Called by Terminal.tsx's pty-exit handler (the single owner of local
		 *  session-exit handling). */
		notifyShellExit(id: string): void {
			for (const cb of shellExitCallbacks) cb(id);
		},

		/** Mark a tab as detached to a floating window */
		detach(tabId: string, windowLabel: string): void {
			setState("detachedWindows", tabId, windowLabel);
		},

		/** Un-mark a tab as detached (called when floating window closes) */
		reattach(tabId: string): void {
			setState(
				produce((s) => {
					delete s.detachedWindows[tabId];
				}),
			);
		},

		/** Check if a tab is currently detached */
		isDetached(tabId: string): boolean {
			return state.detachedWindows[tabId] !== undefined;
		},

		/** Get all non-detached terminal IDs */
		getAttachedIds(): string[] {
			return Object.keys(state.terminals).filter((id) => state.detachedWindows[id] === undefined);
		},

		/** Record last PTY output timestamp without triggering reactive graph */
		touchLastDataAt(id: string, ts: number): void {
			lastDataAtMap.set(id, ts);
			startLastDataAtFlush();
		},

		/** Read last PTY output timestamp (non-reactive, for ActivityDashboard) */
		getLastDataAt(id: string): number | null {
			return lastDataAtMap.get(id) ?? state.terminals[id]?.lastDataAt ?? null;
		},

		/** Revision of the last shell-state event for snapshot freshness checks. */
		getShellStateRevision(id: string): number | null {
			return state.terminals[id]?.shellStateRevision ?? null;
		},

		/** Flush all pending lastDataAt values to the reactive store */
		flushLastDataAt(): void {
			if (lastDataAtMap.size === 0) return;
			batch(() => {
				for (const [id, ts] of lastDataAtMap) {
					if (state.terminals[id]) setState("terminals", id, "lastDataAt", ts);
				}
			});
		},
	};

	return {
		state,
		...actions,
		_testCancelPendingTimers(): void {
			for (const timer of cooldownTimers.values()) clearTimeout(timer);
			cooldownTimers.clear();
			if (lastDataAtFlushTimer) {
				clearInterval(lastDataAtFlushTimer);
				lastDataAtFlushTimer = null;
			}
		},
	};
}

export const terminalsStore = createTerminalsStore();

// Terminals own a pane but don't go through createTabManager, so register the
// deactivator by hand — otherwise opening a diff/markdown/editor tab would leave
// the terminal pane active underneath it. Uses setActive(null), not a raw state
// write, so the visibility RPC for the terminal we're leaving still fires.
registerPaneDeactivator("terminals", () => terminalsStore.setActive(null));

// A session belongs to the backend that spawned it, which is the one owning its
// repo. `repoPath` is assigned by ownership reconciliation and can still be null
// right after a tab opens, so `cwd` — what the tab was opened in — answers until
// then. Without this, a write or resize on a remote session would land locally.
// `untrack` for the same reason as the repo lookup: this runs inside whichever
// effect made the call, and reading the terminal would otherwise subscribe that
// effect to it.
setSessionConnectionLookup((sessionId) =>
	untrack(() => {
		const terminalId = terminalsStore.getTerminalForSession(sessionId);
		const terminal = terminalId ? terminalsStore.get(terminalId) : undefined;
		return getRepoConnection(terminal?.repoPath ?? terminal?.cwd ?? null);
	}),
);
