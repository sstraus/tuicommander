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
 *  `classNames` maps status keys to CSS module class names. */
export function terminalStatusLabel(
	shellState: string | null,
	awaitingInput: string | null,
	isRateLimited: boolean,
	classNames: { rateLimited: string; waiting: string; working: string; idle: string },
): { label: string; className: string } {
	if (isRateLimited) return { label: "Rate limited", className: classNames.rateLimited };
	if (awaitingInput) return { label: "Waiting for input", className: classNames.waiting };
	if (shellState === "busy") return { label: "Working", className: classNames.working };
	if (shellState === "idle") return { label: "Idle", className: classNames.idle };
	return { label: "—", className: classNames.idle };
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
	isPromoted: boolean;
}

export interface ActivitySnapshot {
	terminals: ActivityTerminalRow[];
}

export function buildActivitySnapshot(): ActivitySnapshot {
	const rows = terminalsStore.getAttachedIds().map((id) => {
		const t = terminalsStore.get(id);
		return {
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
			isRateLimited: !!(t?.sessionId && rateLimitStore.isRateLimited(t.sessionId)),
			isPromoted: globalWorkspaceStore.isPromoted(id),
		};
	});
	rows.sort((a, b) => (b.lastDataAt ?? 0) - (a.lastDataAt ?? 0));
	return { terminals: rows };
}
