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
import { Switch } from "../ui/Switch";
import { DiscussionAsk } from "./DiscussionAsk";
import { DiscussionThread } from "./DiscussionThread";
import s from "./ProgressDialog.module.css";
import { ProgressFlow } from "./ProgressFlow";
import { ProgressRecent } from "./ProgressRecent";
import { ProgressReply } from "./ProgressReply";

const KIND_ICON: Record<ProgressKind, string> = {
	// A check, a warning triangle and a small arrow. Monochrome paths, never emoji.
	done: "M20 6 9 17l-5-5",
	blocked: "M12 9v4m0 4h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z",
	intent: "M5 12h14m-6-6 6 6-6 6",
	// A fork into two branches, and a speech bubble.
	delegated: "M6 3v6a6 6 0 0 0 6 6h6m-4-4 4 4-4 4M6 21V9",
	message: "M4 5h16v11H8l-4 4z",
	// A question mark in a circle, and a reply arrow.
	question: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zm-2.9-13a3 3 0 0 1 5.8 1c0 2-3 3-3 3m.1 4h.01",
	answer: "M9 17 4 12l5-5m-5 5h12a4 4 0 0 1 4 4v2",
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
	const [filtersOpen, setFiltersOpen] = createSignal(false);
	const [detailEntry, setDetailEntry] = createSignal<number | null>(null);
	const [expandedEntries, setExpandedEntries] = createSignal<number[]>([]);
	const [openThread, setOpenThread] = createSignal<number | null>(null);
	/** Re-read an open thread whenever the journal list moves (an answer landed). */
	const listRevision = () => `${entries().length}:${entries()[0]?.id ?? 0}`;
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
	/** The cross-project list, a third view beside List and Flow. */
	const [allProjects, setAllProjects] = createSignal(false);
	const isFlow = () => !allProjects() && progressStore.view() === "flow";
	const viewValue = () => (allProjects() ? "all" : isFlow() ? "flow" : "list");
	const chooseView = (next: "list" | "flow" | "all") => {
		setAllProjects(next === "all");
		if (next !== "all") progressStore.setView(next);
	};
	const viewOptions = [
		{ id: "list", label: "List" },
		{ id: "flow", label: "Flow" },
		{ id: "all", label: "All projects" },
	] as const;
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
			<Show
				when={props.embedded}
				fallback={
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
							options={viewOptions}
							value={viewValue()}
							onChange={chooseView}
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
							<label class={s.filter} title="Intents, messages and delegations">
								<input
									type="checkbox"
									checked={progressStore.showActivity()}
									onChange={(e) => progressStore.setShowActivity(e.currentTarget.checked)}
								/>
								Activity
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
				}
			>
				<div class={s.mobileHeader}>
					<h4>Progress</h4>
					<div class={s.mobileControls}>
						<Show when={showProjectSelector()} fallback={<span class={s.mobileProject}>{displayName()}</span>}>
							<Select
								aria-label="Progress project"
								value={project() ?? ""}
								onChange={(event) => progressStore.open(event.currentTarget.value, null)}
							>
								<For each={props.projects}>
									{(path) => (
										<option value={path}>
											{repositoriesStore.get(path)?.displayName ?? path.split(/[\\/]/).pop() ?? path}
										</option>
									)}
								</For>
							</Select>
						</Show>
						<Button
							aria-expanded={filtersOpen()}
							aria-controls="mobile-progress-filters"
							onClick={() => setFiltersOpen(!filtersOpen())}
						>
							Filters
						</Button>
					</div>
					<div class={s.mobileViewRow}>
						<SegmentedControl
							aria-label="Progress view"
							options={viewOptions}
							value={viewValue()}
							onChange={chooseView}
						/>
						<span class={s.scope}>
							{progressStore.selectedPtyId() ? terminalName(progressStore.selectedPtyId()!) : "All agents"}
						</span>
					</div>
					<Show when={filtersOpen()}>
						<div class={s.mobileFilters} id="mobile-progress-filters">
							<Select
								aria-label="Progress terminal"
								value={progressStore.selectedPtyId() ?? ""}
								onChange={(event) => progressStore.selectPty(event.currentTarget.value || null)}
							>
								<option value="">All agents</option>
								<For each={ptyOptions()}>{(terminal) => <option value={terminal.id}>{terminal.label}</option>}</For>
							</Select>
							<Show when={!isFlow()}>
								<label>
									<Switch
										checked={progressStore.blockedOnly()}
										onChange={(event) => progressStore.setBlockedOnly(event.currentTarget.checked)}
									/>
									Blocked only
								</label>
								<label>
									<Switch
										checked={progressStore.showActivity()}
										onChange={(event) => progressStore.setShowActivity(event.currentTarget.checked)}
									/>
									Activity
								</label>
							</Show>
						</div>
					</Show>
				</div>
			</Show>

			<Show when={progressStore.popupsMuted()}>
				<div class={s.popupStatus} role="status">
					<span>Progress popups muted for this session. Blocked reports still appear.</span>
					<Button onClick={() => progressStore.unmutePopups()}>Unmute</Button>
				</div>
			</Show>

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

			<Show when={allProjects()}>
				<ProgressRecent
					revision={progressStore.liveRevision()}
					onOpen={() => {
						setAllProjects(false);
						setOpenThread(null);
					}}
				/>
			</Show>

			<Show when={!allProjects() && !isFlow() && progressStore.selectedPtyId()}>
				{(ptyId) => <DiscussionAsk sessionId={ptyId()} />}
			</Show>

			<div class={s.list} hidden={isFlow() || allProjects()}>
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
									<Show when={!props.embedded}>
										<KindIcon kind={entry.type} />
									</Show>
									<div class={s.body}>
										<Show when={props.embedded}>
											<div class={s.eventHeader}>
												<KindIcon kind={entry.type} />
												<span>
													{
														{
															done: "Done",
															blocked: "Blocked",
															intent: "Started",
															message: "Message",
															delegated: "Delegated",
															question: "Question",
															answer: "Answer",
														}[entry.type]
													}
												</span>
												<time>{formatRelativeTime(entry.createdAtMs, { showDateFallback: true })}</time>
											</div>
										</Show>
										<ProgressReply entry={entry}>
											{/* An open thread shows the question and the answer in full:
											    the row text would repeat them. An answer is a preview. */}
											<Show when={openThread() !== entry.id}>
												<p
													class={s.text}
													classList={{
														[s.textCollapsed]:
															(entry.type === "answer" || (!!props.embedded && isLong(entry.text))) &&
															!expandedEntries().includes(entry.id),
													}}
												>
													{entry.text}
												</p>
											</Show>
										</ProgressReply>
										<Show when={openThread() === entry.id}>
											<DiscussionThread entryId={entry.id} revision={listRevision()} />
										</Show>
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
										<Show
											when={props.embedded}
											fallback={
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
													<Show when={entry.type === "question" || entry.type === "answer"}>
														<Button
															type="button"
															variant="ghost"
															class={s.more}
															aria-expanded={openThread() === entry.id}
															onClick={() => setOpenThread(openThread() === entry.id ? null : entry.id)}
														>
															{openThread() === entry.id ? "Hide thread" : "Thread"}
														</Button>
													</Show>
												</div>
											}
										>
											<div class={s.mobileMeta}>
												<span>{entry.agentName || (entry.ptyId ? terminalName(entry.ptyId) : "Terminal unknown")}</span>
												<Button
													variant="ghost"
													shape="icon"
													aria-label="Entry actions"
													aria-expanded={detailEntry() === entry.id}
													aria-controls={`progress-detail-${entry.id}`}
													onClick={() => setDetailEntry(detailEntry() === entry.id ? null : entry.id)}
												>
													⋯
												</Button>
											</div>
											<Show when={detailEntry() === entry.id}>
												<div class={s.entryDetail} id={`progress-detail-${entry.id}`}>
													<Show when={entry.step}>
														<span>{entry.step}</span>
													</Show>
													<span>{entry.ptyId ? terminalName(entry.ptyId) : "Terminal unknown"}</span>
													<Show when={entry.targetPtyId || entry.targetName}>
														<span>To: {entry.targetName || terminalName(entry.targetPtyId!)}</span>
													</Show>
													<Show when={entry.type === "question" || entry.type === "answer"}>
														<Button onClick={() => setOpenThread(openThread() === entry.id ? null : entry.id)}>
															{openThread() === entry.id ? "Hide thread" : "Thread"}
														</Button>
													</Show>
													<Button
														variant="danger"
														onClick={() => {
															const path = project();
															if (path) void progressStore.deleteEntries(path, [entry.id]);
														}}
													>
														Delete entry
													</Button>
												</div>
											</Show>
										</Show>
									</div>
									<Show when={!props.embedded}>
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
									</Show>
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
