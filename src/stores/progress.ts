import { batch, createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import { showNativeNotice } from "../services/nativeNotifications";
import { repositoriesStore } from "./repositories";
import { terminalsStore } from "./terminals";
import { toastsStore } from "./toasts";

export type ProgressKind = "done" | "blocked" | "intent" | "delegated" | "message";

export interface ProgressEntry {
	id: number;
	project: string;
	ptyId?: string;
	createdAtMs: number;
	type: ProgressKind;
	text: string;
	step?: string;
	agentName?: string;
	/** The terminal a `delegated` or `message` entry points at. */
	targetPtyId?: string;
	targetName?: string;
}

export type FlowState = "busy" | "idle" | "awaiting" | "closed" | "running" | "done";

export interface FlowParticipant {
	id: string;
	kind: "terminal" | "subagent";
	title: string;
	agentType?: string;
	state: FlowState;
	parent?: string;
	intent?: string;
	toolCalls: number;
	ptyId: string;
	agentId?: string;
}

export interface FlowDetailRef {
	ptyId: string;
	agentId: string;
	part: "prompt" | "report";
}

export type FlowEventKind =
	| "intent"
	| "done"
	| "blocked"
	| "delegated"
	| "message"
	| "subagent_spawn"
	| "subagent_return";

export interface FlowEvent {
	/** Stable across refreshes; the view keys expanded rows by it, never by position. */
	id: string;
	kind: FlowEventKind;
	from: string;
	to?: string;
	summary: string;
	/** The whole journal text, when the summary is shorter. */
	text?: string;
	/** Where the whole text of a subagent arrow is fetched from. */
	detail?: FlowDetailRef;
	step?: string;
	atMs: number;
}

export interface ProgressFlow {
	project: string;
	participants: FlowParticipant[];
	events: FlowEvent[];
	truncated: boolean;
}

export type ProgressView = "list" | "flow";

export interface ProjectFlowState {
	data?: ProgressFlow;
	loading: boolean;
	error: string | null;
}

export interface ProgressList {
	project: string;
	entries: ProgressEntry[];
	total: number;
	nextCursor: number | null;
	ptyIds: string[];
	lastViewedMs?: number;
}

export interface ProgressRecordedPayload {
	repo_path: string;
	payload: { entry: ProgressEntry };
}

export interface ProjectProgressState {
	entries: ProgressEntry[];
	ptyIds: string[];
	/// The divider position, frozen when the dialog opened. The stored value
	/// moves on close; redrawing the line under the reader's cursor while they
	/// are still reading is the one thing it must not do.
	dividerMs?: number;
	loading: boolean;
	error: string | null;
}

const [dialogVisible, setDialogVisible] = createSignal(false);
const [requestedProject, setRequestedProject] = createSignal<string | null>(null);
const [selectedPtyId, setSelectedPtyId] = createSignal<string | null>(null);
const [blockedOnly, setBlockedOnly] = createSignal(false);
const [view, setView] = createSignal<ProgressView>("list");

function messageOf(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

export function createProgressStore() {
	const [state, setState] = createStore<{
		projects: Record<string, ProjectProgressState>;
		flows: Record<string, ProjectFlowState>;
	}>({ projects: {}, flows: {} });
	// Entries that arrived while the dialog was closed. The bell shows this
	// count. It is deliberately NOT a query across every registered repository:
	// that fan-out is what made the old panel fire 78 requests on open, and it
	// answered with one red block per repository that no longer existed.
	const [arrivedSinceOpen, setArrivedSinceOpen] = createSignal(0);
	const visitedScopes = new Set<string | null>();
	// The rich sidebar's own copy of each project's flow. Kept apart from
	// `flows` because the dialog scopes that one to a terminal and guards it
	// against the dialog's selection; the sidebar always wants the whole project.
	const [sidebarFlows, setSidebarFlows] = createStore<Record<string, ProgressFlow>>({});
	const sidebarFlowFetchedAt = new Map<string, number>();
	const SIDEBAR_FLOW_MIN_GAP_MS = 5000;
	// An ask inside the gap is not dropped: one trailing refresh runs when the gap ends.
	const sidebarFlowTrailing = new Map<string, ReturnType<typeof setTimeout>>();
	// Responses are applied only if no reset and no newer request happened since they were sent.
	let sidebarFlowEpoch = 0;
	const sidebarFlowSeq = new Map<string, number>();
	const sidebarFlowApplied = new Map<string, number>();
	// Rows showing the flow of a project; the trailing refresh is cancelled when the last one leaves.
	const sidebarFlowHolds = new Map<string, number>();

	function ensure(project: string): void {
		if (state.projects[project]) return;
		setState("projects", project, { entries: [], ptyIds: [], loading: false, error: null });
	}

	async function refreshProject(project: string, freezeDivider = true): Promise<void> {
		ensure(project);
		const ptyId = selectedPtyId();
		const blocked = blockedOnly();
		setState("projects", project, { loading: true, error: null });
		try {
			const input = { blockedOnly: blocked, ...(ptyId ? { ptyId } : {}), limit: 100 };
			let page = await invoke<ProgressList>("progress_list", {
				project,
				input,
			});
			const first = page;
			const entries = [...page.entries];
			while (page.nextCursor != null) {
				if (requestedProject() !== project || selectedPtyId() !== ptyId || blockedOnly() !== blocked) return;
				page = await invoke<ProgressList>("progress_list", { project, input: { ...input, cursor: page.nextCursor } });
				entries.push(...page.entries);
			}
			if (requestedProject() !== project || selectedPtyId() !== ptyId || blockedOnly() !== blocked) return;
			const frozen = freezeDivider ? (state.projects[project]?.dividerMs ?? first.lastViewedMs) : first.lastViewedMs;
			setState("projects", project, { loading: false, error: null, dividerMs: frozen });
			setState("projects", project, "entries", reconcile(entries));
			setState("projects", project, "ptyIds", reconcile(first.ptyIds ?? []));
		} catch (error) {
			if (requestedProject() !== project || selectedPtyId() !== ptyId || blockedOnly() !== blocked) return;
			setState("projects", project, { loading: false, error: messageOf(error) });
		}
	}

	/// The Flow view's read. The backend builds the whole sequence — columns,
	/// order, arrows, redaction — so this only stores what came back.
	async function refreshFlow(project: string): Promise<void> {
		const ptyId = selectedPtyId();
		setState("flows", project, (current) => ({ ...(current ?? {}), loading: true, error: null }));
		try {
			const flow = await invoke<ProgressFlow>("progress_flow", {
				project,
				input: ptyId ? { ptyId } : {},
			});
			if (requestedProject() !== project || selectedPtyId() !== ptyId) return;
			setState("flows", project, { data: flow, loading: false, error: null });
		} catch (error) {
			if (requestedProject() !== project || selectedPtyId() !== ptyId) return;
			setState("flows", project, { loading: false, error: messageOf(error) });
		}
	}

	/// Reads the existing `progress_flow` command for the rich sidebar's subagent
	/// lines. Rows of one repo ask together, so asks closer than the gap collapse
	/// into one trailing refresh.
	async function refreshSidebarFlow(project: string): Promise<void> {
		const elapsed = Date.now() - (sidebarFlowFetchedAt.get(project) ?? 0);
		// A last read stamped in the future means the clock went back: read now. Waiting
		// out the skew could exceed setTimeout's 2^31-1 ms limit, which fires after 1 ms
		// and re-arms the trailing timer in a busy loop.
		const wait = elapsed < 0 ? 0 : SIDEBAR_FLOW_MIN_GAP_MS - elapsed;
		if (wait > 0) {
			if (!sidebarFlowTrailing.has(project)) {
				sidebarFlowTrailing.set(
					project,
					setTimeout(() => {
						sidebarFlowTrailing.delete(project);
						void refreshSidebarFlow(project);
					}, wait),
				);
			}
			return;
		}
		sidebarFlowFetchedAt.set(project, Date.now());
		const epoch = sidebarFlowEpoch;
		const seq = (sidebarFlowSeq.get(project) ?? 0) + 1;
		sidebarFlowSeq.set(project, seq);
		try {
			const flow = await invoke<ProgressFlow>("progress_flow", { project, input: {} });
			// Drop only a response older than one already applied: a newer read that failed
			// must not discard an older valid one.
			if (epoch !== sidebarFlowEpoch || seq < (sidebarFlowApplied.get(project) ?? 0)) return;
			sidebarFlowApplied.set(project, seq);
			setSidebarFlows(project, reconcile(flow));
		} catch {
			// Keep the last flow: a failed read must not blank the subagent lines.
		}
	}

	/// Registers a row that shows `project`'s flow. The returned release cancels the
	/// pending trailing refresh once no row is left.
	function holdSidebarFlow(project: string): () => void {
		sidebarFlowHolds.set(project, (sidebarFlowHolds.get(project) ?? 0) + 1);
		let released = false;
		return () => {
			if (released) return;
			released = true;
			const left = (sidebarFlowHolds.get(project) ?? 1) - 1;
			if (left > 0) {
				sidebarFlowHolds.set(project, left);
				return;
			}
			sidebarFlowHolds.delete(project);
			clearTimeout(sidebarFlowTrailing.get(project));
			sidebarFlowTrailing.delete(project);
		};
	}

	/// Refresh whichever view is showing. The list stays the source of the
	/// divider and of deletion, so it is read in both.
	function refreshVisible(project: string, freezeDivider = true): void {
		void refreshProject(project, freezeDivider);
		if (view() === "flow") void refreshFlow(project);
	}

	function open(project: string | null = null, ptyId?: string | null): void {
		const target = project ?? repositoriesStore.state.activeRepoPath ?? null;
		const activeId = terminalsStore.state.activeId;
		const active = activeId ? terminalsStore.state.terminals[activeId] : undefined;
		const selected = ptyId === undefined ? (active?.repoPath === target ? active.sessionId : null) : ptyId;
		visitedScopes.clear();
		visitedScopes.add(selected ?? null);
		setRequestedProject(target);
		setSelectedPtyId(selected ?? null);
		setArrivedSinceOpen(0);
		setDialogVisible(true);
		if (target) {
			// A project opened for the first time in this session has no frozen
			// divider yet, so the stored one is adopted here and held until close.
			setState("projects", target, (current) => ({
				...(current ?? { entries: [], ptyIds: [], loading: false, error: null }),
			}));
			setState("projects", target, "entries", []);
			refreshVisible(target, false);
		}
	}

	function selectPty(ptyId: string | null): void {
		visitedScopes.add(ptyId);
		setSelectedPtyId(ptyId);
		const project = requestedProject();
		if (project) {
			setState("projects", project, "entries", []);
			setState("projects", project, "dividerMs", undefined);
			refreshVisible(project, false);
		}
	}

	async function close(): Promise<void> {
		const project = requestedProject();
		setDialogVisible(false);
		if (!project) return;
		let failure: unknown;
		for (const ptyId of visitedScopes) {
			try {
				await invoke("progress_mark_viewed", { project, ...(ptyId ? { ptyId } : {}) });
			} catch (error) {
				failure ??= error;
			}
		}
		setState("projects", project, "dividerMs", undefined);
		if (failure !== undefined) {
			// Failing to move the divider costs the reader a stale line, never an
			// entry. It is not worth a toast.
			setState("projects", project, "error", messageOf(failure));
		}
	}

	async function deleteEntries(project: string, ids: number[]): Promise<boolean> {
		ensure(project);
		setState("projects", project, "error", null);
		try {
			await invoke("progress_delete", { project, input: { ids } });
			await refreshProject(project);
			return true;
		} catch (error) {
			setState("projects", project, "error", messageOf(error));
			return false;
		}
	}

	/// A scoped Flow also draws the terminal's parent and children, so an
	/// entry from or to any of its columns is on screen.
	function drawnInFlow(project: string, entry: ProgressEntry): boolean {
		if (view() !== "flow") return false;
		const participants = state.flows[project]?.data?.participants ?? [];
		return participants.some((p) => p.id === entry.ptyId || p.id === entry.targetPtyId);
	}

	function presentLive(payload: ProgressRecordedPayload): void {
		const entry = payload.payload.entry;
		const project = payload.repo_path;
		const showing = dialogVisible() && requestedProject() === project;
		let onScreen = true;
		if (showing && (selectedPtyId() === null || selectedPtyId() === entry.ptyId)) {
			refreshVisible(project);
		} else if (showing && drawnInFlow(project, entry)) {
			void refreshFlow(project);
		} else {
			onScreen = false;
		}
		// An `intent:` is what the agent set out to do, and a hand-off is one
		// agent talking to another — neither is an outcome. They belong in the
		// journal and not in the user's face: no toast, and no bell count.
		if (entry.type !== "done" && entry.type !== "blocked") return;
		if (!onScreen) setArrivedSinceOpen((count) => count + 1);
		const projectName =
			repositoriesStore.get(payload.repo_path)?.displayName ??
			payload.repo_path.split(/[\\/]/).pop() ??
			payload.repo_path;
		void showNativeNotice({
			title: `${projectName} · Progress ${entry.type}`,
			body: entry.text,
			key: `progress:${payload.repo_path}:${entry.type}:${entry.text}`,
			target: { kind: "progress", project: payload.repo_path, ptyId: entry.ptyId ?? null },
		});
		toastsStore.add(
			// The repo badge already names the project (it is passed below), so a
			// title of "<project> · <step>" printed it twice and spent half the
			// title's width doing it. The project name is the title only when
			// there is no step to put there.
			entry.step ?? projectName,
			entry.text,
			entry.type === "blocked" ? "warn" : "info",
			// Silent by default: a blocked entry is not automatically a request
			// for the user's attention.
			false,
			{ label: "Open Progress", onClick: () => open(payload.repo_path, entry.ptyId ?? null) },
			undefined,
			payload.repo_path,
			entry.ptyId,
			false,
		);
	}

	return {
		state,
		dialogVisible,
		requestedProject,
		selectedPtyId,
		selectPty,
		blockedOnly,
		setBlockedOnly: (value: boolean) => {
			setBlockedOnly(value);
			const project = requestedProject();
			if (project) void refreshProject(project);
		},
		view,
		setView: (next: ProgressView) => {
			setView(next);
			const project = requestedProject();
			if (next === "flow" && project) void refreshFlow(project);
		},
		refreshFlow,
		sidebarFlow: (project: string): ProgressFlow | undefined => sidebarFlows[project],
		refreshSidebarFlow,
		holdSidebarFlow,
		fetchFlowDetail: (detail: FlowDetailRef) =>
			invoke<{ text: string }>("progress_flow_detail", { input: detail }).then((result) => result.text),
		open,
		close,
		toggle: () => (dialogVisible() ? void close() : open()),
		refreshProject,
		deleteEntries,
		presentLive,
		/** Acknowledge the bell count without changing journal entries or view dividers. */
		markRead: () => setArrivedSinceOpen(0),
		get unreadCount() {
			return arrivedSinceOpen();
		},
		resetForTests() {
			batch(() => {
				setState("projects", reconcile({}));
				setState("flows", reconcile({}));
				setSidebarFlows(reconcile({}));
				sidebarFlowFetchedAt.clear();
				sidebarFlowEpoch++;
				sidebarFlowSeq.clear();
				sidebarFlowApplied.clear();
				sidebarFlowHolds.clear();
				for (const timer of sidebarFlowTrailing.values()) clearTimeout(timer);
				sidebarFlowTrailing.clear();
				setView("list");
				setDialogVisible(false);
				setRequestedProject(null);
				setSelectedPtyId(null);
				setBlockedOnly(false);
				setArrivedSinceOpen(0);
				visitedScopes.clear();
			});
		},
	};
}

export const progressStore = createProgressStore();
