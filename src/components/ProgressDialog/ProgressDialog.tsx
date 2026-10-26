import { type Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import {
	type ProgressFlow as FlowData,
	type ProgressEntry,
	type ProgressKind,
	progressStore,
} from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { formatRelativeTime } from "../../utils/time";
import d from "../shared/dialog.module.css";
import { Button } from "../ui/Button";
import { SegmentedControl } from "../ui/SegmentedControl";
import { Select } from "../ui/Select";
import s from "./ProgressDialog.module.css";
import { ProgressFlow } from "./ProgressFlow";

const KIND_ICON: Record<ProgressKind, string> = {
	// A check, a warning triangle and a small arrow. Monochrome paths, never emoji.
	done: "M20 6 9 17l-5-5",
	blocked: "M12 9v4m0 4h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z",
	intent: "M5 12h14m-6-6 6 6-6 6",
	// A fork into two branches, and a speech bubble.
	delegated: "M6 3v6a6 6 0 0 0 6 6h6m-4-4 4 4-4 4M6 21V9",
	message: "M4 5h16v11H8l-4 4z",
};

const KindIcon: Component<{ kind: ProgressKind }> = (props) => (
	<svg
		class={s.icon}
		width="14"
		height="14"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="2"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
	>
		<path d={KIND_ICON[props.kind]} />
	</svg>
);

export interface ProgressDialogProps {
	/**
	 * Fill the parent instead of floating over it. The mobile shell gives
	 * Progress a whole bottom tab, which is already the modal surface a dialog
	 * would create — so it drops the overlay and keeps the list.
	 */
	embedded?: boolean;
	/** Populated journal projects supplied by the mobile backend. */
	projects?: string[];
	projectsError?: string;
}

/**
 * The Progress journal for one project: one newest-first list, a line marking
 * where the last visit ended, and on-demand older pages.
 */
export const ProgressDialog: Component<ProgressDialogProps> = (props) => {
	if (!props.embedded) registerModal(() => void progressStore.close());
	createEffect(() => {
		if (!props.embedded || !props.projects?.length) return;
		if (!props.projects.includes(progressStore.requestedProject() ?? "")) progressStore.open(props.projects[0], null);
	});

	const project = () => progressStore.requestedProject();
	const projectState = () => {
		const path = project();
		return path ? progressStore.state.projects[path] : undefined;
	};
	const entries = createMemo<ProgressEntry[]>(() => projectState()?.entries ?? []);
	const [expandedEntries, setExpandedEntries] = createSignal<number[]>([]);
	createEffect(() => {
		project();
		setExpandedEntries([]);
	});
	const isLong = (text: string) => text.length > 160 || text.split("\n").length > 4;
	function toggleExpanded(id: number) {
		setExpandedEntries((current) =>
			current.includes(id) ? current.filter((entryId) => entryId !== id) : [...current, id],
		);
	}
	const terminalNames = createMemo(
		() =>
			new Map(
				Object.values(terminalsStore.state.terminals)
					.filter((terminal) => terminal.sessionId)
					.map((terminal) => [terminal.sessionId as string, terminal.name]),
			),
	);
	const terminalName = (ptyId: string) => terminalNames().get(ptyId) || `Terminal ${ptyId.slice(0, 8)}`;
	const ptyOptions = createMemo(() => {
		const path = project();
		const ids = new Set(projectState()?.ptyIds ?? []);
		for (const terminal of Object.values(terminalsStore.state.terminals)) {
			if (terminal.repoPath === path && terminal.sessionId) ids.add(terminal.sessionId);
		}
		const selected = progressStore.selectedPtyId();
		if (selected) ids.add(selected);
		return Array.from(ids).map((id) => ({ id, label: terminalName(id) }));
	});

	const displayName = () => {
		const path = project();
		if (!path) return "";
		return repositoriesStore.get(path)?.displayName ?? path.split(/[\\/]/).pop() ?? path;
	};

	/// The index the divider is drawn above: the first entry older than the
	/// frozen last-visit timestamp. `-1` means every entry is new, `entries
	/// .length` means none is, and either way the divider is simply not drawn.
	const dividerIndex = createMemo(() => {
		const since = projectState()?.dividerMs;
		if (since === undefined) return -1;
		const index = entries().findIndex((entry) => entry.createdAtMs <= since);
		return index <= 0 ? -1 : index;
	});

	const showProjectSelector = createMemo(() => !!props.embedded && (props.projects?.length ?? 0) > 0);
	const isFlow = () => progressStore.view() === "flow";
	const flowState = () => {
		const path = project();
		return path ? progressStore.state.flows?.[path] : undefined;
	};

	const body = (
		<div
			class={`${s.dialog}${isFlow() ? ` ${s.wide}` : ""}${props.embedded ? ` ${s.embedded}` : ""}`}
			role={props.embedded ? "region" : "dialog"}
			aria-label="Progress"
			onClick={(e) => e.stopPropagation()}
		>
			<div class={s.header}>
				<div class={s.title}>
					<h4>Progress</h4>
					<Show when={!showProjectSelector()}>
						<span class={s.project}>{displayName()}</span>
					</Show>
				</div>
				<Show when={showProjectSelector()}>
					<Select
						class={s.terminalSelect}
						aria-label="Progress project"
						value={project() ?? ""}
						onChange={(event) => progressStore.open(event.currentTarget.value, null)}
					>
						<For each={props.projects}>
							{(path) => <option value={path}>{path.split(/[\\/]/).pop() || path}</option>}
						</For>
					</Select>
				</Show>
				<Select
					class={s.terminalSelect}
					aria-label="Progress terminal"
					onChange={(event) => progressStore.selectPty(event.currentTarget.value || null)}
				>
					<option value="" selected={!progressStore.selectedPtyId()}>
						All repo
					</option>
					<For each={ptyOptions()}>
						{(terminal) => (
							<option value={terminal.id} selected={progressStore.selectedPtyId() === terminal.id}>
								{terminal.label}
							</option>
						)}
					</For>
				</Select>
				<SegmentedControl
					class={s.viewToggle}
					aria-label="Progress view"
					options={[
						{ id: "list", label: "List" },
						{ id: "flow", label: "Flow" },
					]}
					value={isFlow() ? "flow" : "list"}
					onChange={(view) => progressStore.setView(view)}
				/>
				<Show when={!isFlow()}>
					<label class={s.filter}>
						<input
							type="checkbox"
							checked={progressStore.blockedOnly()}
							onChange={(e) => progressStore.setBlockedOnly(e.currentTarget.checked)}
						/>
						Blocked only
					</label>
				</Show>
				<Show when={!props.embedded}>
					<Button type="button" variant="ghost" aria-label="Close" onClick={() => void progressStore.close()}>
						<svg
							width="16"
							height="16"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							aria-hidden="true"
						>
							<path d="M18 6 6 18M6 6l12 12" />
						</svg>
					</Button>
				</Show>
			</div>

			<Show when={projectState()?.error}>
				{/* One line for the whole failure. The old panel printed one red block
					    per repository and filled the view with them. */}
				<p class={s.error}>{projectState()?.error}</p>
			</Show>

			<Show when={isFlow()}>
				<Show
					when={(flowState()?.data?.participants.length ?? 0) > 0}
					fallback={
						<p class={s.empty}>
							{flowState()?.error
								? flowState()?.error
								: flowState()?.loading
									? "Loading…"
									: "No hand-offs recorded yet. Spawning an agent or sending it a message draws the first arrow."}
						</p>
					}
				>
					<ProgressFlow flow={flowState()?.data as FlowData} fetchDetail={progressStore.fetchFlowDetail} />
				</Show>
			</Show>

			<div class={s.list} hidden={isFlow()}>
				<Show
					when={entries().length > 0}
					fallback={
						<p class={s.empty}>
							{/* A journal that could not be read is unavailable. Saying
							    "nothing recorded" there would report a failure as a
							    finished, empty project. */}
							{projectState()?.error
								? "This project's journal is unavailable."
								: props.embedded && props.projectsError
									? "Progress projects are unavailable."
									: props.embedded && props.projects === undefined
										? "Loading Progress projects…"
										: props.embedded && props.projects?.length === 0
											? "No project with Progress entries is available."
											: projectState()?.loading
												? "Loading…"
												: progressStore.blockedOnly()
													? "Nothing is blocked."
													: progressStore.selectedPtyId()
														? "No progress recorded for this terminal yet."
														: "No progress recorded for this project yet."}
						</p>
					}
				>
					<For each={entries()}>
						{(entry, index) => (
							<>
								<Show when={index() === dividerIndex()}>
									<div class={s.divider}>Seen before</div>
								</Show>
								<div class={s.entry} data-kind={entry.type}>
									<KindIcon kind={entry.type} />
									<div class={s.body}>
										<p
											class={s.text}
											classList={{
												[s.textCollapsed]:
													!!props.embedded && isLong(entry.text) && !expandedEntries().includes(entry.id),
											}}
										>
											{entry.text}
										</p>
										<Show when={props.embedded && isLong(entry.text)}>
											<Button
												type="button"
												variant="ghost"
												class={s.more}
												aria-expanded={expandedEntries().includes(entry.id)}
												onClick={() => toggleExpanded(entry.id)}
											>
												{expandedEntries().includes(entry.id) ? "Less" : "More"}
											</Button>
										</Show>
										<div class={s.meta}>
											<Show when={entry.type === "intent"}>
												<span class={s.kindLabel}>set out to</span>
												<span>·</span>
											</Show>
											<Show when={entry.type === "delegated" || entry.type === "message"}>
												<span class={s.kindLabel}>
													{entry.type === "delegated" ? "delegated to" : "messaged"}{" "}
													{entry.targetPtyId
														? terminalNames().get(entry.targetPtyId) ||
															entry.targetName ||
															terminalName(entry.targetPtyId)
														: entry.targetName}
												</span>
												<span>·</span>
											</Show>
											<span>{formatRelativeTime(entry.createdAtMs, { showDateFallback: true })}</span>
											<Show when={entry.step}>
												<span>· {entry.step}</span>
											</Show>
											<Show when={entry.agentName}>
												<span>· {entry.agentName}</span>
											</Show>
											<Show when={!progressStore.selectedPtyId()}>
												<span>· {entry.ptyId ? terminalName(entry.ptyId) : "Terminal unknown"}</span>
											</Show>
										</div>
									</div>
									<Button
										type="button"
										variant="ghost"
										class={s.delete}
										aria-label="Delete entry"
										onClick={() => {
											const path = project();
											if (path) void progressStore.deleteEntries(path, [entry.id]);
										}}
									>
										<svg
											width="14"
											height="14"
											viewBox="0 0 24 24"
											fill="none"
											stroke="currentColor"
											stroke-width="2"
											stroke-linecap="round"
											aria-hidden="true"
										>
											<path d="M3 6h18M8 6V4h8v2m-9 0 1 14h8l1-14" />
										</svg>
									</Button>
								</div>
							</>
						)}
					</For>
				</Show>
				<Show when={projectState()?.nextCursor != null}>
					<Button
						type="button"
						class={s.loadOlder}
						disabled={projectState()?.loading}
						onClick={() => void progressStore.loadOlder()}
					>
						{projectState()?.loading ? "Loading…" : "Load older"}
					</Button>
				</Show>
			</div>
		</div>
	);

	return (
		<Show when={!props.embedded} fallback={body}>
			{/* Clicking the backdrop dismisses; the dialog itself stops the event. */}
			<div class={d.overlay} onClick={() => void progressStore.close()}>
				{body}
			</div>
		</Show>
	);
};

export default ProgressDialog;
