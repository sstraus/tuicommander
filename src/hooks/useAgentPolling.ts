import { createEffect, createMemo, onCleanup, untrack } from "solid-js";
import { AGENT_TYPES, AGENTS, type AgentType } from "../agents";
import { invoke } from "../invoke";
import type { ChoicePrompt } from "../mobile/useSessions";
import { pluginRegistry } from "../plugins/pluginRegistry";
import { appLogger } from "../stores/appLogger";
import { type AgentLifecycleState, type ProviderRecovery, type ShellState, terminalsStore } from "../stores/terminals";
import { isTauri, rpc, subscribeEvents, type Unsubscribe } from "../transport";

/** Fallback polling interval — only catches cold starts and edge cases (ms) */
const POLL_INTERVAL_MS = 30_000;
const NATIVE_LIFECYCLE_TIMEOUT_MS = 5_000;

type BackendSessionState = {
	provider_recovery?: ProviderRecovery | null;
	last_error?: string | null;
	codex_binding?: { session_id: string } | null;
	agent_type?: string | null;
	shell_state?: string;
	agent_state?: string;
	abandoned_since_ms?: number;
	agent_intent?: string | null;
	last_prompt?: string | null;
	awaiting_input?: boolean;
	question_confident?: boolean;
	question_text?: string;
	choice_prompt?: ChoicePrompt;
	background_work?: boolean;
	queued_commands?: number;
	last_activity_ms?: number;
};

type SessionLifecycleResponse = {
	session_id: string;
	state?: BackendSessionState | null;
};

let nextLifecycleRequest = 0;
let lastAppliedLifecycleRequest = 0;
let lifecycleSyncInFlight: Promise<void> | null = null;

function toAgentLifecycleState(value: unknown): AgentLifecycleState {
	return value === "starting" ||
		value === "working" ||
		value === "awaiting_input" ||
		value === "idle" ||
		value === "completed"
		? value
		: null;
}

function toShellState(value: unknown): ShellState | undefined {
	return value === "busy" || value === "idle" ? value : undefined;
}

function listNativeSessionsWithTimeout(): Promise<SessionLifecycleResponse[]> {
	return new Promise((resolve, reject) => {
		const timeout = setTimeout(
			() => reject(new Error("Lifecycle session snapshot timed out")),
			NATIVE_LIFECYCLE_TIMEOUT_MS,
		);
		Promise.resolve(invoke<SessionLifecycleResponse[]>("list_active_sessions")).then(
			(sessions) => {
				clearTimeout(timeout);
				resolve(sessions);
			},
			(error) => {
				clearTimeout(timeout);
				reject(error);
			},
		);
	});
}

/**
 * Write one authoritative backend snapshot onto the terminal that owns the
 * session. Shared by the `session-state-changed` push and the mount-time
 * catch-up below so the two can never disagree about which fields a snapshot
 * owns — the push exists precisely to make the snapshot path rare, and a field
 * only one of them applied would then look like an intermittent bug.
 */
function applySessionState(termId: string, sessionId: string, state: BackendSessionState | null | undefined): void {
	const shellState = toShellState(state?.shell_state);
	const wasAwaiting = terminalsStore.get(termId)?.awaitingInput === "question";
	const isAwaiting = state?.awaiting_input === true;
	terminalsStore.update(termId, {
		agentState: toAgentLifecycleState(state?.agent_state),
		providerRecovery: state?.provider_recovery ?? null,
		// SessionState snapshots are complete. Rust omits Option::None fields,
		// therefore absence retracts context instead of meaning "unchanged".
		agentIntent: state?.agent_intent ?? null,
		lastPrompt: state?.last_prompt ?? null,
		// API failure is durable backend state, not a question. Idle snapshots
		// must not erase the red dot set by the parsed-event consumer.
		awaitingInput: state?.last_error ? "error" : isAwaiting ? "question" : null,
		awaitingInputConfident: state?.question_confident === true,
		questionText: isAwaiting ? (state?.question_text ?? null) : null,
		choicePrompt: isAwaiting ? (state?.choice_prompt ?? null) : null,
		backgroundWork: state?.background_work === true,
		abandonedSinceMs: state?.abandoned_since_ms ?? null,
		// Omitted by the backend when zero (serde skips it), so absence is an
		// empty queue — not "unknown".
		queuedCommands: state?.queued_commands ?? 0,
		lastActivityAt: state?.last_activity_ms ?? null,
		...(shellState !== undefined ? { shellState } : {}),
	});
	applyAgentIdentity(termId, toAgentType(state?.agent_type ?? null));
	const exactId = state?.agent_type === "codex" ? (state.codex_binding?.session_id ?? null) : null;
	terminalsStore.update(termId, {
		exactAgentSessionId: exactId,
		...(exactId ? { agentSessionId: exactId } : {}),
	});
	if (wasAwaiting !== isAwaiting) {
		pluginRegistry.dispatchStructuredEvent(
			"awaiting",
			{ awaiting: isAwaiting, confident: state?.question_confident === true },
			sessionId,
		);
	}
}

/**
 * Handle one `session-state-changed` push. The backend emits it once per real
 * state transition on both transports (Tauri window event + `/events` SSE), so
 * a visible-but-idle window costs zero IPC — this replaced a 1 Hz
 * `list_active_sessions` poll that ran for as long as any terminal existed.
 */
export function applySessionStateEvent(payload: unknown): void {
	const event = payload as SessionLifecycleResponse | null;
	const sessionId = event?.session_id;
	if (typeof sessionId !== "string") return;
	const termId = terminalsStore.getTerminalForSession(sessionId);
	if (!termId) return;
	applySessionState(termId, sessionId, event?.state);
}

/** Apply the backend's task lifecycle snapshot to its local terminal. The
 * snapshot is intentionally separate from foreground-process detection: a
 * ready composer can be shell-idle while a meaningful descendant still works. */
export function syncAgentLifecycleStates(): Promise<void> {
	if (lifecycleSyncInFlight) return lifecycleSyncInFlight;
	lifecycleSyncInFlight = syncAgentLifecycleStatesOnce().finally(() => {
		lifecycleSyncInFlight = null;
	});
	return lifecycleSyncInFlight;
}

/** Recovery must read state from after the gap: a snapshot already in flight
 * may have been captured before it, so run one fresh snapshot once it settles. */
async function resyncAgentLifecycleStates(): Promise<void> {
	await lifecycleSyncInFlight?.catch(() => {});
	await syncAgentLifecycleStates();
}

async function syncAgentLifecycleStatesOnce(): Promise<void> {
	const request = ++nextLifecycleRequest;
	const requestedSessions = new Map<
		string,
		{
			sessionId: string;
			shellStateRevision: number;
			agentIntent: string | null;
			lastPrompt: string | null;
			agentType: AgentType | null;
		}
	>();
	for (const termId of terminalsStore.getIds()) {
		const terminal = terminalsStore.get(termId);
		const sessionId = terminal?.sessionId;
		const revision = terminalsStore.getShellStateRevision(termId);
		if (sessionId && revision !== null) {
			requestedSessions.set(termId, {
				sessionId,
				shellStateRevision: revision,
				agentIntent: terminal?.agentIntent ?? null,
				lastPrompt: terminal?.lastPrompt ?? null,
				agentType: terminal?.agentType ?? null,
			});
		}
	}
	let sessions: SessionLifecycleResponse[];
	try {
		if (isTauri()) {
			sessions = await listNativeSessionsWithTimeout();
		} else {
			sessions = await rpc<SessionLifecycleResponse[]>("list_active_sessions");
		}
	} catch (err) {
		appLogger.debug("app", "[AgentLifecycle] session list failed", err);
		return;
	}
	if (!Array.isArray(sessions)) return;
	// A slow earlier poll must never overwrite a newer lifecycle conclusion.
	if (request < lastAppliedLifecycleRequest) return;
	lastAppliedLifecycleRequest = request;

	const seenSessionIds = new Set(sessions.map((session) => session.session_id));
	for (const [termId, requested] of requestedSessions) {
		const current = terminalsStore.get(termId);
		if (
			!seenSessionIds.has(requested.sessionId) &&
			current?.sessionId === requested.sessionId &&
			terminalsStore.getShellStateRevision(termId) === requested.shellStateRevision
		) {
			terminalsStore.update(termId, { shellState: "exited", sessionId: null, agentState: null, backgroundWork: false });
		}
	}
	for (const session of sessions) {
		const termId = terminalsStore.getTerminalForSession(session.session_id);
		if (!termId) continue;
		const requested = requestedSessions.get(termId);
		const snapshotIsFresh =
			requested?.sessionId === session.session_id &&
			requested.shellStateRevision === terminalsStore.getShellStateRevision(termId) &&
			requested.agentIntent === (terminalsStore.get(termId)?.agentIntent ?? null) &&
			requested.lastPrompt === (terminalsStore.get(termId)?.lastPrompt ?? null) &&
			requested.agentType === (terminalsStore.get(termId)?.agentType ?? null);
		if (!snapshotIsFresh) continue;
		applySessionState(termId, session.session_id, session.state);
	}
}

/**
 * Detection trigger source — determines whether the call can clear an existing agentType.
 * - "idle": Shell-state transitioned to idle (prompt returned). This is the ONLY source
 *   that can clear a previously detected agent, because it means the foreground process
 *   ended and the shell reclaimed the terminal.
 * - "busy": Shell-state transitioned to busy. Can only discover (set) agents, never clear.
 * - "poll": Periodic 30s fallback. Can only discover (set) agents, never clear.
 */
export type DetectionSource = "idle" | "busy" | "poll";

/** Validate a string from the backend is a known AgentType */
function toAgentType(value: string | null): AgentType | null {
	if (value === null) return null;
	return (AGENT_TYPES as readonly string[]).includes(value) ? (value as AgentType) : null;
}

/** Apply identity before plugin dispatch so both snapshot and discovery paths agree. */
function applyAgentIdentity(termId: string, agentType: AgentType | null): void {
	const current = terminalsStore.get(termId);
	if (!current) return;
	const prevAgentType = current.agentType;
	if (prevAgentType !== agentType) {
		appLogger.debug("app", `[AgentDetect] ${termId} agentType "${prevAgentType}" → "${agentType}"`);

		const sessId = current.sessionId;

		// Notify stop of previous agent BEFORE updating the store. Plugin dispatch
		// filters read the current store.agentType, so agent-stopped must fire
		// while the previous type is still current or filtered plugins miss it
		// (their internal per-session tracking then leaks across agent changes —
		// e.g. cache-keepalive kept writing to a session that switched claude→codex).
		if (prevAgentType !== null && sessId) {
			pluginRegistry.notifyStateChange({ type: "agent-stopped", sessionId: sessId, terminalId: termId });
		}

		terminalsStore.update(termId, { agentType, exactAgentSessionId: null });

		// Reset agent-specific state carried over from the previous agent.
		if (prevAgentType !== null) {
			terminalsStore.update(termId, { agentSessionId: null });
		}

		// Notify start of new agent AFTER updating the store so filtered plugins
		// for the new type see the event and receive the synthetic shell-state replay.
		if (agentType !== null && sessId) {
			pluginRegistry.notifyStateChange({ type: "agent-started", sessionId: sessId, terminalId: termId });
			// Replay current shell state to plugins filtered by agentType — they missed
			// events dispatched before detection completed (agentType was still stale).
			const freshShellState = terminalsStore.get(termId)?.shellState;
			if (freshShellState) {
				pluginRegistry.dispatchStructuredEvent("shell-state", { state: freshShellState }, sessId);
			}
		}
	}
}

/**
 * Detect the foreground agent for a single terminal and update the store.
 * Called on shell-state transitions (event-driven) and by the fallback poll.
 *
 * @param source - What triggered this detection. Only "idle" can clear an existing agent.
 *   "busy" and "poll" can only discover new agents — they never clear, because
 *   foreground-process sampling is inherently flaky during subprocess transitions.
 */
export async function detectAgentForTerminal(termId: string, source: DetectionSource = "poll"): Promise<void> {
	const current = terminalsStore.get(termId);
	if (!current) {
		return;
	}
	if (!current.sessionId) return;

	let agentType: AgentType | null;
	try {
		const result = await invoke<string | null>("get_session_foreground_process", {
			sessionId: current.sessionId,
		});
		agentType = toAgentType(result);
	} catch (err) {
		appLogger.debug("app", `[AgentDetect] ${termId} invoke failed`, err);
		return;
	}

	const prevAgentType = current.agentType;

	// Agent→null transition: only allowed from "idle" source (shell prompt returned).
	// Poll and busy sources can only discover agents, never clear them — foreground
	// process sampling is too flaky during subprocess transitions (git, node, etc.).
	// An idle transition is definitive, though: waiting for further idle events leaves
	// agentType stuck after an agent exits, because a normal shell emits just one.
	if (prevAgentType !== null && agentType === null) {
		if (source !== "idle") return; // Not a reliable clearing signal — skip
	}

	applyAgentIdentity(termId, agentType);

	// Attempt session discovery when an agent is running.
	// Agents with sessionDiscovery: always re-discover (session ID changes after /clear, /new, etc.).
	// Agents without sessionDiscovery: nothing to discover.
	if (agentType !== null) {
		const disc = AGENTS[agentType].sessionDiscovery;
		if (disc) {
			const cwd = current.cwd ?? null;

			// Collect UUIDs already claimed by other terminals (exclude self)
			const claimedIds: string[] = [];
			for (const id of terminalsStore.getIds()) {
				if (id === termId) continue;
				const sid = terminalsStore.get(id)?.agentSessionId;
				if (sid) claimedIds.push(sid);
			}

			// Read the agent's leaf PID so the backend can extract env vars
			// (CLAUDE_CONFIG_DIR, GEMINI_CLI_HOME, CODEX_HOME, HOME) directly from
			// the process's initial environment — the ground-truth source.
			let agentPid: number | null = null;
			if (current.sessionId) {
				try {
					agentPid = await invoke<number | null>("get_session_leaf_pid", {
						sessionId: current.sessionId,
					});
				} catch {
					// Process may have exited — fall through to run-config fallback
				}
			}

			try {
				const found = await invoke<{ sessionId: string; launchCommand: string | null; provider: string | null } | null>(
					"discover_agent_session",
					{
						agentType,
						cwd,
						claimedIds,
						agentPid,
						envOverrides: {},
					},
				);
				const exactId = agentType === "codex" ? terminalsStore.get(termId)?.exactAgentSessionId : null;
				const sessionId = exactId ?? found?.sessionId;
				if (sessionId && sessionId !== terminalsStore.get(termId)?.agentSessionId) {
					appLogger.debug(
						"app",
						`[AgentDetect] ${termId} discovered agentSessionId "${sessionId}" (was "${current.agentSessionId}")`,
					);
					terminalsStore.update(termId, { agentSessionId: sessionId });
				}
				// The rebuilt command is ground truth read from the live process: it names
				// the real binary and the env (CLAUDE_CONFIG_DIR) that decides which store
				// holds the session. The run config cannot — a shell alias is expanded
				// before exec, so TUIC only ever sees "c2".
				if (found?.launchCommand && found.launchCommand !== current.agentLaunchCommand) {
					terminalsStore.update(termId, { agentLaunchCommand: found.launchCommand });
				}
				const provider = found?.provider ?? null;
				if (found && provider !== (current.agentProvider ?? null)) {
					terminalsStore.update(termId, { agentProvider: provider });
				}
			} catch (err) {
				appLogger.debug("app", `[AgentDetect] ${termId} discover_agent_session failed`, err);
			}
		}
	}
}

/**
 * Fallback polling loop for agent detection.
 * Primary detection happens event-driven via shell-state transitions in Terminal.tsx.
 * This 30s fallback catches cold starts and edge cases.
 */
export function useAgentPolling(): void {
	// Tracked as a boolean, not the raw id list: the effect below only needs to
	// know "are there any terminals at all", but `terminalsStore.getIds()` is a
	// fresh array on every add/remove. Depending on it directly re-ran this
	// whole effect (tearing down and restarting both timers) on every tab
	// add/remove, which reset the 30s discovery poll's countdown on every
	// churn — during rapid tab churn it could starve indefinitely. A memo
	// only notifies downstream when the boolean actually flips.
	const hasTerminals = createMemo(() => terminalsStore.getIds().length > 0);
	// Terminal rows are created before their backend PTY has necessarily been
	// attached. Track the bindings themselves so the authoritative catch-up runs
	// when a session id arrives, without tearing down the subscription or the
	// independent 30s foreground-agent discovery timer below.
	const lifecycleBindings = createMemo(() =>
		terminalsStore
			.getIds()
			.map((id) => `${id}:${terminalsStore.get(id)?.sessionId ?? ""}`)
			.join("\0"),
	);

	createEffect(() => {
		const requestedBindings = lifecycleBindings();
		if (!requestedBindings) return;
		untrack(() => {
			void syncAgentLifecycleStates().then(() => {
				// A binding may land while an earlier catch-up is in flight. Calls are
				// coalesced, so explicitly schedule the one trailing read it needs.
				if (lifecycleBindings() !== requestedBindings) void syncAgentLifecycleStates();
			});
		});
	});

	createEffect(() => {
		if (!hasTerminals()) return;

		const pollAll = async () => {
			const currentIds = terminalsStore.getIds();
			// NOT parallelized (story 143-8bef): detectAgentForTerminal collects
			// claimedIds from the OTHER terminals' already-stored agentSessionId to
			// avoid two terminals claiming the same discovered session. That dedup
			// only holds if each terminal's claim is persisted before the next is
			// polled — Promise.allSettled would race and break it (see test
			// "passes claimed_ids from other terminals to avoid duplicate assignment").
			for (const id of currentIds) {
				await detectAgentForTerminal(id);
			}
		};

		// The only timer left in this hook, and it is NOT the lifecycle poll:
		// 30s agent *discovery* (which agent owns each terminal, and its session
		// id), which has no push to replace it. The 1 Hz `list_active_sessions`
		// lifecycle sample that used to sit beside it is gone — see the
		// subscription below (#687-be9d).
		const timer = setInterval(() => {
			pollAll().catch((err) => appLogger.debug("app", "[AgentPoll] poll failed", err));
		}, POLL_INTERVAL_MS);

		// Lifecycle arrives as a push, not a sample. The backend publishes
		// `session-state-changed` once per real transition — dual-emitted on the
		// Tauri window and on `/events` SSE, so `subscribeEvents` covers desktop
		// and browser with one handler. The 1 Hz `list_active_sessions` poll it
		// replaces ran for as long as any terminal existed, awake or idle, and
		// gating it on visibility only silenced the hidden case (#652-0114).
		//
		let unsubscribeState: Unsubscribe | null = null;
		let subscriptionDisposed = false;
		void subscribeEvents(
			{ "session-state-changed": applySessionStateEvent },
			{
				// A transition that lands while the SSE stream is down, or that the
				// backend's bounded broadcast dropped, is never redelivered — so the
				// badge would read stale until the session NEXT really transitions,
				// which for a quiet session is never. Re-running the same catch-up
				// used at mount closes the gap for both causes; it reads the truth
				// rather than replaying what was missed, so it needs no knowledge of
				// which events were lost, and running it twice is harmless.
				//
				// Desktop never gets here: `subscribeEvents` only wires this on the
				// SSE transport (#721-7dd5).
				onResync: (reason) => {
					if (subscriptionDisposed) return;
					appLogger.debug("app", `[AgentLifecycle] resync after ${reason}`);
					void resyncAgentLifecycleStates();
				},
			},
		)
			.then((unsubscribe) => {
				if (subscriptionDisposed) unsubscribe();
				else unsubscribeState = unsubscribe;
			})
			.catch((err) => appLogger.debug("app", "[AgentLifecycle] state subscription failed", err));

		onCleanup(() => {
			clearInterval(timer);
			subscriptionDisposed = true;
			unsubscribeState?.();
		});
	});
}
