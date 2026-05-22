import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { activityDashboardStore } from "../../stores/activityDashboard";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { rateLimitStore } from "../../stores/ratelimit";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { projectName, terminalStatusLabel } from "../../utils/activitySnapshot";
import { getRepoColor } from "../../utils/repoColor";
import { formatRelativeTime } from "../../utils/time";
import { GlobeIcon } from "../GlobeIcon";
import { PanelWindowControls } from "../ui/PanelWindowControls";
import s from "./ActivityDashboard.module.css";

export const statusClasses = {
	rateLimited: s.statusRateLimited,
	waiting: s.statusWaiting,
	working: s.statusWorking,
	idle: s.statusIdle,
};

/** Truncate a string to a single line for display */
function truncate(text: string, maxLen = 80): string {
	const oneLine = text.replace(/\n/g, " ").trim();
	if (oneLine.length <= maxLen) return oneLine;
	return oneLine.slice(0, maxLen - 1) + "\u2026";
}

/** Speech bubble icon (last prompt) */
const PromptIcon: Component = () => (
	<svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
		<path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h11A1.5 1.5 0 0 1 15 3.5v7A1.5 1.5 0 0 1 13.5 12H9.373l-2.62 1.81A.75.75 0 0 1 5.6 13.2V12H2.5A1.5 1.5 0 0 1 1 10.5v-7Zm1.5-.5a.5.5 0 0 0-.5.5v7a.5.5 0 0 0 .5.5H6.35a.75.75 0 0 1 .75.75v.83l1.81-1.25a.75.75 0 0 1 .427-.133H13.5a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.5-.5h-11Z" />
	</svg>
);

/** Crosshair icon (agent intent) */
const IntentIcon: Component = () => (
	<svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
		<path d="M8 1a.75.75 0 0 1 .75.75v1.82a4.505 4.505 0 0 1 3.68 3.68h1.82a.75.75 0 0 1 0 1.5h-1.82a4.505 4.505 0 0 1-3.68 3.68v1.82a.75.75 0 0 1-1.5 0v-1.82a4.505 4.505 0 0 1-3.68-3.68H1.75a.75.75 0 0 1 0-1.5h1.82A4.505 4.505 0 0 1 7.25 3.57V1.75A.75.75 0 0 1 8 1ZM5.5 8a2.5 2.5 0 1 0 5 0 2.5 2.5 0 0 0-5 0Z" />
	</svg>
);

/** Gear/spinner icon (current task) */
const TaskIcon: Component = () => (
	<svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
		<path d="M7.068.727c.243-.97 1.62-.97 1.864 0l.3 1.2a.957.957 0 0 0 1.18.633l1.18-.39c.93-.31 1.753.789 1.13 1.593l-.76.98a.957.957 0 0 0 .166 1.34l1.01.76c.78.59.39 1.82-.55 1.84l-1.22.03a.957.957 0 0 0-.905.905l-.03 1.22c-.02.94-1.25 1.33-1.84.55l-.76-1.01a.957.957 0 0 0-1.34-.166l-.98.76c-.804.623-1.903-.2-1.593-1.13l.39-1.18a.957.957 0 0 0-.633-1.18l-1.2-.3c-.97-.243-.97-1.62 0-1.864l1.2-.3a.957.957 0 0 0 .633-1.18l-.39-1.18c-.31-.93.789-1.753 1.593-1.13l.98.76a.957.957 0 0 0 1.34-.166l.76-1.01ZM8 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" />
	</svg>
);

/** People icon (active sub-tasks / agents) */
const SubTaskIcon: Component = () => (
	<svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
		<path d="M2 5.5a3.5 3.5 0 1 1 5.898 2.549 5.508 5.508 0 0 1 3.034 4.084.75.75 0 1 1-1.482.235 4.001 4.001 0 0 0-7.9 0 .75.75 0 0 1-1.482-.236A5.507 5.507 0 0 1 3.102 8.05 3.493 3.493 0 0 1 2 5.5ZM11 4a.75.75 0 1 0 0 1.5 2.5 2.5 0 0 1 2.45 2.993.75.75 0 1 0 1.472.29A4.001 4.001 0 0 0 11 4Zm-5.5.5a2 2 0 1 0 0 4 2 2 0 0 0 0-4Z" />
	</svg>
);

export type TerminalRow = {
	id: string;
	name: string;
	project: string | null;
	projectColor: string | undefined;
	agent: string;
	status: { label: string; className: string };
	isWorking: boolean;
	lastDataAt: number | null;
	idleSince: number | null;
	lastPrompt: string | null;
	agentIntent: string | null;
	currentTask: string | null;
	activeSubTasks: number;
	isActive: boolean;
	isPromoted: boolean;
};

interface ActivityDashboardProps {
	onSelect?: (id: string) => void;
	onPromote?: (id: string) => void;
	/** When true, renders without overlay — used in detached panel windows. */
	embedded?: boolean;
	/** External data source. When provided, bypasses store reads. */
	terminals?: () => TerminalRow[];
}

export const ActivityDashboard: Component<ActivityDashboardProps> = (props) => {
	const [, setTick] = createSignal(0);
	const isOpen = () => activityDashboardStore.state.isOpen;

	// Tick every second to refresh relative timestamps
	createEffect(() => {
		if (!isOpen()) return;
		const interval = setInterval(() => setTick((n) => n + 1), 1000);
		onCleanup(() => clearInterval(interval));
	});

	// Keyboard navigation
	createEffect(() => {
		if (!isOpen()) return;

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Escape") {
				e.preventDefault();
				e.stopPropagation();
				activityDashboardStore.close();
			}
		};

		document.addEventListener("keydown", handleKeydown, true);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
	});

	const handleRowClick = (termId: string) => {
		if (props.onSelect) {
			props.onSelect(termId);
		} else {
			terminalsStore.setActive(termId);
			requestAnimationFrame(() => terminalsStore.get(termId)?.ref?.focus());
		}
		if (!props.embedded) activityDashboardStore.close();
	};

	/** Build a fresh row from the live store. Called at render time so every
	 *  store mutation (intent, status, last-prompt, …) flows through immediately
	 *  instead of waiting for the 10s order snapshot. */
	const buildRow = (id: string): TerminalRow | null => {
		const term = terminalsStore.get(id);
		if (!term) return null;
		const isRL = !!(term.sessionId && rateLimitStore.isRateLimited(term.sessionId));
		const status = terminalStatusLabel(term.shellState, term.awaitingInput, isRL, statusClasses);
		const repoPath = repositoriesStore.getRepoPathForTerminal(id);
		return {
			id,
			name: term.name,
			project: projectName(term.cwd),
			projectColor: repoPath ? getRepoColor(repoPath) : undefined,
			agent: term.agentType || "shell",
			status,
			isWorking: isRL || !!term.awaitingInput || terminalsStore.isBusy(id),
			lastDataAt: terminalsStore.getLastDataAt(id),
			idleSince: term.idleSince,
			lastPrompt: term.lastPrompt,
			agentIntent: term.agentIntent,
			// Claude Code spinner verbs are decorative garbage — suppress them
			currentTask: term.agentType === "claude" ? null : term.currentTask,
			activeSubTasks: term.activeSubTasks,
			isActive: terminalsStore.state.activeId === id,
			isPromoted: globalWorkspaceStore.isPromoted(id),
		};
	};

	/** Order-only snapshot: working terminals first, idle second.
	 *  Working group keeps store insertion order (stable); idle group sorts by most recent activity. */
	const liveOrder = createMemo(() => {
		const ids = terminalsStore.getAttachedIds();
		const items = ids.map((id, idx) => {
			const term = terminalsStore.get(id);
			const isRL = !!(term?.sessionId && rateLimitStore.isRateLimited(term.sessionId));
			const working = isRL || !!term?.awaitingInput || terminalsStore.isBusy(id);
			return { id, working, idx, t: term?.idleSince ?? terminalsStore.getLastDataAt(id) ?? 0 };
		});
		const workingItems = items.filter((x) => x.working).sort((a, b) => a.idx - b.idx);
		const idleItems = items.filter((x) => !x.working).sort((a, b) => b.t - a.t);
		return [...workingItems, ...idleItems].map((x) => x.id);
	});

	// Snapshot sort order every 10s so rows don't reshuffle on every mutation.
	// New items/removals trigger an immediate snapshot via count change.
	const [orderSnapshot, setOrderSnapshot] = createSignal<string[]>(liveOrder());
	createEffect(() => {
		if (!isOpen()) return;
		setOrderSnapshot(liveOrder());
		const interval = setInterval(() => setOrderSnapshot(liveOrder()), 10_000);
		onCleanup(() => clearInterval(interval));
	});

	let prevCount = liveOrder().length;
	const orderedIds = createMemo(() => {
		const current = liveOrder();
		if (current.length !== prevCount) {
			prevCount = current.length;
			setOrderSnapshot(current);
		}
		return orderSnapshot();
	});

	const storeTerminals = createMemo(() => orderedIds().map(buildRow).filter(Boolean) as TerminalRow[]);

	const terminals = () => (props.terminals ? props.terminals() : storeTerminals());

	const dashboardContent = () => (
		<>
			<div class={s.header}>
				<h3>Activity Dashboard</h3>
				<div class={s.headerActions}>
					<PanelWindowControls
						panelId="activity"
						mode={props.embedded ? "detached" : "inline"}
						onInlineClose={() => activityDashboardStore.close()}
					/>
				</div>
			</div>

			<div class={s.list}>
				<Show when={terminals().length === 0}>
					<div class={s.empty}>No active terminals</div>
				</Show>

				<For each={terminals()}>
					{(term) => (
						<div
							class={`${s.row} ${term.isActive ? s.activeRow : ""} ${!term.isWorking ? s.idleRow : ""}`}
							onClick={() => handleRowClick(term.id)}
						>
							<div class={s.rowMain}>
								<div class={s.nameCell}>
									<span class={s.termName}>{term.name}</span>
									<Show when={term.project}>
										<span class={s.project} style={term.projectColor ? { color: term.projectColor } : undefined}>
											{term.project}
										</span>
									</Show>
								</div>
								<span class={s.agent}>{term.agent}</span>
								<span class={`${s.status} ${term.status.className}`}>{term.status.label}</span>
								<span class={s.lastActivity}>{term.isWorking ? "" : formatRelativeTime(term.idleSince)}</span>
								<button
									class={`${s.promoteBtn} ${term.isPromoted ? s.promoted : ""}`}
									title={term.isPromoted ? "Remove from Global Workspace" : "Promote to Global Workspace"}
									onClick={(e) => {
										e.stopPropagation();
										if (props.onPromote) {
											props.onPromote(term.id);
										} else {
											globalWorkspaceStore.togglePromote(term.id);
										}
									}}
								>
									<GlobeIcon />
								</button>
							</div>
							<Show when={term.currentTask}>
								{(task) => (
									<div class={s.subRow} title={task()}>
										<TaskIcon />
										<span class={s.subText}>{truncate(task())}</span>
									</div>
								)}
							</Show>
							<Show when={term.activeSubTasks > 0}>
								<div class={s.subRow} title={`${term.activeSubTasks} sub-tasks running`}>
									<SubTaskIcon />
									<span class={s.subText}>{term.activeSubTasks} sub-tasks running</span>
								</div>
							</Show>
							<Show when={term.agentIntent} keyed>
								{(intent) => (
									<div class={s.subRow} title={intent}>
										<IntentIcon />
										<span class={s.subText}>{truncate(intent)}</span>
									</div>
								)}
							</Show>
							{(() => {
								const prompt = term.lastPrompt;
								if (!prompt || term.agentIntent) return null;
								return (
									<div class={s.subRow} title={prompt}>
										<PromptIcon />
										<span class={s.subText}>{truncate(prompt)}</span>
									</div>
								);
							})()}
						</div>
					)}
				</For>
			</div>

			<div class={s.footer}>
				<span>{terminals().length} terminal(s)</span>
				<Show when={!props.embedded}>
					<span style={{ "margin-left": "auto" }}>Click to switch • Esc to close</span>
				</Show>
			</div>
		</>
	);

	if (props.embedded) {
		return (
			<div class={s.dashboard} style={{ "max-height": "100vh", height: "100vh" }}>
				{dashboardContent()}
			</div>
		);
	}

	return (
		<Show when={isOpen()}>
			<div class={s.overlay} onClick={() => activityDashboardStore.close()}>
				<div class={s.dashboard} onClick={(e) => e.stopPropagation()}>
					{dashboardContent()}
				</div>
			</div>
		</Show>
	);
};
