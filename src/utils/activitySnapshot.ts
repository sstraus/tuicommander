import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { rateLimitStore } from "../stores/ratelimit";
import { terminalsStore } from "../stores/terminals";

/** Extract project name (last path segment) from cwd */
export function projectName(cwd: string | null): string | null {
	if (!cwd) return null;
	const segments = cwd.replace(/\/+$/, "").split("/");
	return segments[segments.length - 1] || null;
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
): { label: string; className: string } {
	if (isRateLimited) return { label: "Rate limited", className: classNames.rateLimited };
	if (awaitingInput === "error") return { label: "Error", className: classNames.error };
	if (awaitingInput) return { label: "Waiting for input", className: classNames.waiting };
	if (shellState === "busy") return { label: "Working", className: classNames.working };
	if (shellState === "idle") return { label: "Idle", className: classNames.idle };
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
	isBusy: boolean; // Debounced busy (2s hold) — calmer than raw shellState for badge/ordering
	isPromoted: boolean;
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
			currentTask: t?.agentType === "claude" ? null : (t?.currentTask ?? null),
			lastPrompt: t?.lastPrompt ?? null,
			activeSubTasks: t?.activeSubTasks ?? 0,
			cwd: t?.cwd ?? null,
			lastDataAt: terminalsStore.getLastDataAt(id),
			idleSince: t?.idleSince ?? null,
			isActive: terminalsStore.state.activeId === id,
			isRateLimited,
			isBusy: terminalsStore.isBusy(id),
			isPromoted: globalWorkspaceStore.isPromoted(id),
		});
	}
	const isWorking = (id: string): boolean => {
		const r = rowById.get(id);
		return !!r && (r.isRateLimited || !!r.awaitingInput || r.isBusy);
	};
	const order = reconcileActivityOrder(snapshotSpine, ids, isWorking);
	return { terminals: order.map((id) => rowById.get(id)).filter((r): r is ActivityTerminalRow => !!r) };
}
