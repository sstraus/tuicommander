import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { rateLimitStore } from "../stores/ratelimit";
import { terminalsStore } from "../stores/terminals";

export type EffectiveActivityState =
	| "rate_limited"
	| "error"
	| "awaiting_input"
	| "working"
	| "completed"
	| "idle"
	| "unknown";

/** Resolve the dashboard state from task lifecycle and PTY activity.
 *
 * Task lifecycle is authoritative when it says a turn is active or complete:
 * an input-ready PTY may legitimately be idle while an agent-owned descendant
 * keeps working. Shell activity remains the fallback when lifecycle is idle or
 * unavailable. */
export function effectiveActivityState(
	shellState: string | null,
	awaitingInput: string | null,
	isRateLimited: boolean,
	agentState: string | null,
	backgroundWork: boolean,
): EffectiveActivityState {
	if (isRateLimited) return "rate_limited";
	if (awaitingInput === "error") return "error";
	if (awaitingInput || agentState === "awaiting_input") return "awaiting_input";
	// A ready composer is available work from the dashboard's point of view.
	// Codex may intentionally leave a long-lived background terminal (for
	// example a dev server) running after the turn has completed; that process
	// remains tracked by the backend but must not keep this row latched Working.
	if (shellState === "idle" && backgroundWork) return "idle";
	if (agentState === "starting" || agentState === "working" || backgroundWork) return "working";
	if (agentState === "completed") return "completed";
	if (shellState === "busy") return "working";
	if (agentState === "idle") return "idle";
	if (shellState === "idle") return "idle";
	return "unknown";
}

/** True only for states that should receive working row styling and ordering. */
export function isActivityWorking(state: EffectiveActivityState): boolean {
	return state === "rate_limited" || state === "awaiting_input" || state === "working";
}

/** Every label `terminalStatusLabel` can produce, lowercased. */
const STATUS_LABELS = new Set(["rate limited", "error", "waiting for input", "working", "completed", "idle"]);

/**
 * The task worth showing next to a status badge — `null` when there is none.
 *
 * A `current_task` is whatever the agent painted on its status line. When that
 * is a bare spinner verb it repeats the badge sitting right next to it: a row
 * reading "Working / Working" costs a line and says nothing. Claude's verbs are
 * dropped wholesale because it cycles decorative ones ("Undulating",
 * "Crunching") that restate the badge without matching its wording.
 *
 * Matching is exact against the labels this module itself produces, so a real
 * task ("Waiting for background terminal") is never mistaken for a verb.
 */
export function displayTask(
	currentTask: string | null | undefined,
	agentType: string | null | undefined,
): string | null {
	if (!currentTask) return null;
	if (agentType === "claude") return null;
	return STATUS_LABELS.has(currentTask.trim().toLowerCase()) ? null : currentTask;
}

/** Extract project name (last path segment) from cwd */
export function projectName(cwd: string | null): string | null {
	if (!cwd) return null;
	const segments = cwd.replace(/\/+$/, "").split("/");
	return segments.at(-1)! || null;
}

/** Derive display status from terminal state fields.
 *  `classNames` maps status keys to CSS module class names.
 *  `awaitingInput` is "question" | "error" | null — an API error must NOT be
 *  collapsed into "Waiting for input": it gets its own badge. */
export function terminalStatusLabel(
	shellState: string | null,
	awaitingInput: string | null,
	isRateLimited: boolean,
	classNames: { rateLimited: string; error: string; waiting: string; working: string; idle: string },
	agentState: string | null = null,
	backgroundWork = false,
): { label: string; className: string } {
	const state = effectiveActivityState(shellState, awaitingInput, isRateLimited, agentState, backgroundWork);
	if (state === "rate_limited") return { label: "Rate limited", className: classNames.rateLimited };
	if (state === "error") return { label: "Error", className: classNames.error };
	if (state === "awaiting_input") return { label: "Waiting for input", className: classNames.waiting };
	if (state === "working") return { label: "Working", className: classNames.working };
	if (state === "completed") return { label: "Completed", className: classNames.idle };
	if (state === "idle") return { label: "Idle", className: classNames.idle };
	return { label: "—", className: classNames.idle };
}

/** Reconcile a persistent display spine with the current terminal set, then return
 *  ids partitioned working-first / idle-second, each group in spine (first-seen) order.
 *
 *  Mutates `spine` in place: drops removed terminals (preserving order), appends
 *  newly-seen ones at the end. A terminal's rendered position is a pure function of
 *  (working, spine-index) — so a row moves ONLY when it crosses the working/idle
 *  boundary (a real state change), never on a recency/timestamp tick. This is what
 *  keeps the dashboard from reshuffling avanti-e-indietro on every poll. */
export function reconcileActivityOrder(spine: string[], ids: string[], isWorking: (id: string) => boolean): string[] {
	const present = new Set(ids);
	for (let i = spine.length - 1; i >= 0; i--) {
		if (!present.has(spine[i])) spine.splice(i, 1);
	}
	for (const id of ids) if (!spine.includes(id)) spine.push(id);
	const working: string[] = [];
	const idle: string[] = [];
	for (const id of spine) (isWorking(id) ? working : idle).push(id);
	return [...working, ...idle];
}

export interface ActivityTerminalRow {
	id: string;
	name: string;
	shellState: string | null;
	awaitingInput: string | null;
	sessionId: string | null;
	agentType: string | null;
	agentIntent: string | null;
	currentTask: string | null;
	lastPrompt: string | null;
	activeSubTasks: number;
	cwd: string | null;
	lastDataAt: number | null;
	idleSince: number | null;
	isActive: boolean;
	isRateLimited: boolean;
	agentState: string | null;
	backgroundWork: boolean;
	isBusy: boolean; // Debounced busy (2s hold) — calmer than raw shellState for badge/ordering
	isPromoted: boolean;
	subAgentTag: string | null;
}

export interface ActivitySnapshot {
	terminals: ActivityTerminalRow[];
}

// Persistent display spine for the detached/serialized snapshot. Module-level so
// it survives across the 1s serialize ticks — keeps rows from reshuffling while
// their working/idle state is unchanged. The inline overlay keeps its own spine.
const snapshotSpine: string[] = [];

export function buildActivitySnapshot(): ActivitySnapshot {
	const ids = terminalsStore.getAttachedIds();
	const rowById = new Map<string, ActivityTerminalRow>();
	for (const id of ids) {
		const t = terminalsStore.get(id);
		const isRateLimited = !!(t?.sessionId && rateLimitStore.isRateLimited(t.sessionId));
		rowById.set(id, {
			id,
			name: t?.name ?? "",
			shellState: t?.shellState ?? null,
			awaitingInput: t?.awaitingInput ?? null,
			sessionId: t?.sessionId ?? null,
			agentType: t?.agentType ?? null,
			agentIntent: t?.agentIntent ?? null,
			currentTask: displayTask(t?.currentTask, t?.agentType),
			lastPrompt: t?.lastPrompt ?? null,
			activeSubTasks: t?.activeSubTasks ?? 0,
			cwd: t?.cwd ?? null,
			lastDataAt: terminalsStore.getLastDataAt(id),
			idleSince: t?.idleSince ?? null,
			isActive: terminalsStore.state.activeId === id,
			isRateLimited,
			agentState: t?.agentState ?? null,
			backgroundWork: t?.backgroundWork ?? false,
			isBusy: terminalsStore.isBusy(id),
			isPromoted: globalWorkspaceStore.isPromoted(id),
			subAgentTag: terminalsStore.getSubAgentTag(id),
		});
	}
	const isWorking = (id: string): boolean => {
		const r = rowById.get(id);
		return (
			!!r &&
			isActivityWorking(
				effectiveActivityState(r.shellState, r.awaitingInput, r.isRateLimited, r.agentState, r.backgroundWork),
			)
		);
	};
	const order = reconcileActivityOrder(snapshotSpine, ids, isWorking);
	return { terminals: order.map((id) => rowById.get(id)).filter((r): r is ActivityTerminalRow => !!r) };
}
