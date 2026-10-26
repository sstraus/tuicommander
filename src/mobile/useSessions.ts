import { createSignal, onCleanup } from "solid-js";
import { appLogger } from "../stores/appLogger";
import type { ChatViewWake } from "../stores/chatView";
import { rpc, subscribeEvents } from "../transport";
import { isAcpConfigured } from "../utils/acpAvailability";
import { createVisibilityInterval } from "./utils/visibilityInterval";

/** Server-side accumulated state for a session (matches Rust SessionState) */
export interface SessionState {
	awaiting_input: boolean;
	question_text?: string;
	question_confident?: boolean;
	rate_limited: boolean;
	retry_after_ms?: number;
	usage_limit_pct?: number;
	shell_state?: string;
	agent_state?: string;
	abandoned_since_ms?: number;
	last_activity_ms: number;
	agent_type?: string;
	last_error?: string;
	agent_intent?: string;
	current_task?: string;
	active_sub_tasks?: number;
	last_prompt?: string;
	progress?: number;
	suggested_actions?: string[];
	slash_menu_items?: SlashMenuItem[];
	choice_prompt?: ChoicePrompt;
}

/** A single slash command menu item (matches Rust output_parser::SlashMenuItem) */
export interface SlashMenuItem {
	command: string;
	description: string;
	highlighted: boolean;
}

/** A numbered choice dialog (edit-confirm, bash-confirm, etc.).
 *  Matches Rust output_parser::ChoicePromptPayload. */
export interface ChoicePrompt {
	title: string;
	options: ChoiceOption[];
	dismiss_key?: string;
	amend_key?: string;
	/** How this agent's dialog accepts a selected option. */
	selection_mode?: "navigate-only" | "navigate-enter" | "toggle-enter";
}

/** Single option in a ChoicePrompt. Matches Rust output_parser::ChoiceOption. */
export interface ChoiceOption {
	key: string;
	label: string;
	highlighted: boolean;
	destructive: boolean;
	hint?: string;
}

/** Session info returned by GET /sessions (matches Rust SessionInfo) */
export interface SessionInfo {
	/** Live PTY child PID; omitted when unavailable or exited. */
	child_pid?: number;
	session_id: string;
	/** A completion this phone has not opened yet; derived from agent_state. */
	unseen?: boolean;
	cwd: string | null;
	worktree_path: string | null;
	worktree_branch: string | null;
	display_name?: string | null;
	/** Live agent identity bound to this PTY; a child's parent_session may name it. */
	tuic_session?: string | null;
	/** Session (or $TUIC_SESSION) of the agent that spawned this one. */
	parent_session?: string | null;
	state?: SessionState;
}

const POLL_INTERVAL_MS = 3_000;
const MAX_BACKOFF_MS = 30_000;

/** Delay before the next poll after `failures` consecutive failures: 3s doubling up to 30s. */
export function pollDelayMs(failures: number): number {
	return Math.min(POLL_INTERVAL_MS * 2 ** failures, MAX_BACKOFF_MS);
}

/** The server answered but refused this client (`HttpRpcError.status`): sign in again, retrying is pointless. */
export function isAuthError(err: unknown): boolean {
	const status = err instanceof Error ? (err as { status?: number }).status : undefined;
	return status === 401 || status === 403;
}

/**
 * Merge a freshly polled list into the previous one, reusing the previous object
 * for every session whose payload is unchanged.
 *
 * The server rebuilds each `SessionInfo` per request, so a plain `setSessions`
 * hands Solid brand-new objects 20 times a minute. `<For>` maps rows by
 * reference, so every session card — and every memo derived from the list —
 * would be torn down and rebuilt on each poll even when nothing moved.
 *
 * Returns `prev` itself when the two lists are equivalent, so an idle poll is
 * fully inert. Comparison is by serialization: the payload comes from one serde
 * struct, so key order is stable, and a handful of sessions costs microseconds
 * against the DOM work it avoids.
 */
export function reconcileSessions(prev: SessionInfo[], next: SessionInfo[]): SessionInfo[] {
	const byId = new Map(prev.map((s) => [s.session_id, s]));
	let changed = prev.length !== next.length;

	const merged = next.map((incoming, index) => {
		const existing = byId.get(incoming.session_id);
		if (existing && JSON.stringify(existing) === JSON.stringify(incoming)) {
			// Same content, but possibly a different position in the list.
			if (prev[index] !== existing) changed = true;
			return existing;
		}
		changed = true;
		return incoming;
	});

	return changed ? merged : prev;
}

/**
 * Thin hook that polls GET /sessions every 3s while the page is visible, and
 * subscribes to SSE for real-time session create/close events between polls.
 *
 * Returns reactive signals for the session list, loading state, and error.
 */
export function useSessions(onRecovered?: () => void) {
	const [aiChatAvailable, setAiChatAvailable] = createSignal(false);
	let availabilityRevision = 0;
	async function refreshAiChatAvailability() {
		const revision = ++availabilityRevision;
		try {
			const config = await rpc<{ ego_executable?: string }>("load_config");
			if (revision === availabilityRevision) setAiChatAvailable(isAcpConfigured(config.ego_executable));
		} catch (err) {
			if (revision === availabilityRevision) setAiChatAvailable(false);
			appLogger.warn("network", "Could not load AI Chat availability", err);
		}
	}
	onCleanup(() => {
		availabilityRevision++;
	});
	void refreshAiChatAvailability();

	const [sessions, setSessions] = createSignal<SessionInfo[]>([]);
	const [loading, setLoading] = createSignal(true);
	const [refreshing, setRefreshing] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const [authError, setAuthError] = createSignal(false);
	const seenCompletions = new Set<string>();
	const chatWakes = new Map<string, Set<() => void>>();
	const subscribeChatWake: ChatViewWake = (id, wake) => {
		const handlers = chatWakes.get(id) ?? new Set<() => void>();
		handlers.add(wake);
		chatWakes.set(id, handlers);
		return () => {
			handlers.delete(wake);
			if (!handlers.size) chatWakes.delete(id);
		};
	};

	let refreshToken = 0;
	let failures = 0;
	let nextAttemptAt = 0;
	let request: AbortController | undefined;
	let disposed = false;
	onCleanup(() => {
		disposed = true;
		request?.abort();
	});

	/** `force` skips the backoff wait: first load, SSE events, manual retry, back online. */
	async function fetchSessions(force = true, retryViews = false) {
		if (disposed || (!force && (request || Date.now() < nextAttemptAt))) return;
		request?.abort();
		const current = new AbortController();
		request = current;
		const timeout = setTimeout(() => current.abort(), 5_000);
		try {
			const result = await rpc<SessionInfo[]>("list_active_sessions", {}, undefined, current.signal);
			if (disposed || request !== current) return;
			const recovered = error() !== null;
			for (const session of result) {
				if (session.state?.agent_state !== "completed") seenCompletions.delete(session.session_id);
				session.unseen = session.state?.agent_state === "completed" && !seenCompletions.has(session.session_id);
			}
			setSessions((prev) => reconcileSessions(prev, result));
			setError(null);
			setAuthError(false);
			failures = 0;
			if (recovered || retryViews) {
				void refreshAiChatAvailability();
				onRecovered?.();
			}
		} catch (err) {
			if (disposed || request !== current) return;
			const msg = err instanceof Error ? err.message : String(err);
			setError(isAuthError(err) ? "Sign in again to reconnect." : "Could not connect to the server.");
			setAuthError(isAuthError(err));
			nextAttemptAt = Date.now() + pollDelayMs(failures++);
			appLogger.warn("network", `Failed to fetch sessions: ${msg}`);
		} finally {
			clearTimeout(timeout);
			if (request === current) {
				request = undefined;
				if (!disposed) setLoading(false);
			}
			// NOTE: do NOT set refreshing here — refresh() manages its own lifecycle
			// via the refreshToken guard to avoid race conditions with concurrent fetches.
		}
	}

	// Initial fetch
	fetchSessions();

	// Poll every 3s, but only while the page is visible
	createVisibilityInterval(() => void fetchSessions(false), POLL_INTERVAL_MS);

	// A returning connection should not wait out the backoff.
	const onOnline = () => void fetchSessions();
	const onVisible = () => {
		if (document.visibilityState !== "hidden") void fetchSessions();
	};
	window.addEventListener("pageshow", onVisible);
	document.addEventListener("resume", onVisible);
	onCleanup(() => {
		window.removeEventListener("pageshow", onVisible);
		document.removeEventListener("resume", onVisible);
	});
	window.addEventListener("online", onOnline);
	onCleanup(() => window.removeEventListener("online", onOnline));

	// One shared SSE subscription: create/close refresh sessions, parsed state
	// updates rows, and app settings changes refresh AI Chat availability.
	//
	// `subscribeEvents` turns these handler keys into the `?types=` query the
	// /events endpoint filters on. Without it the client is sent — and has to
	// JSON-parse — every event kind the backend publishes, the large majority of
	// which the mobile UI has no handler for.
	const unsubscribe = subscribeEvents(
		{
			"settings-changed": (payload) => {
				if ((payload as { section: string }).section === "app") void refreshAiChatAvailability();
			},
			"chat-view-changed": (payload) => {
				const { session_id } = payload as { session_id: string };
				for (const wake of chatWakes.get(session_id) ?? []) wake();
			},
			"session-created": () => void fetchSessions(),
			"session-closed": () => void fetchSessions(),
			"pty-parsed": (payload) => {
				const { session_id, parsed } = payload as {
					session_id: string;
					parsed: { type: string; state?: string };
				};
				if (parsed.type !== "shell-state" || !parsed.state) return;
				setSessions((prev) =>
					prev.map((s) =>
						s.session_id === session_id && s.state ? { ...s, state: { ...s.state, shell_state: parsed.state } } : s,
					),
				);
			},
		},
		{ onResync: () => void refreshAiChatAvailability() },
	);

	onCleanup(() => {
		unsubscribe.then((fn) => fn()).catch(() => {});
	});

	/** A working session can be opened before its completion; do not pre-mark that turn. */
	function markSeen(sessionId: string) {
		if (sessions().find((session) => session.session_id === sessionId)?.state?.agent_state !== "completed") return;
		seenCompletions.add(sessionId);
		setSessions((prev) =>
			prev.map((session) => (session.session_id === sessionId ? { ...session, unseen: false } : session)),
		);
	}

	/** Force an immediate refresh (sets refreshing=true while in-flight) */
	function refresh() {
		const token = ++refreshToken;
		setRefreshing(true);
		fetchSessions(true, true).finally(() => {
			if (refreshToken === token) setRefreshing(false);
		});
	}

	return { aiChatAvailable, sessions, loading, refreshing, error, authError, refresh, markSeen, subscribeChatWake };
}
