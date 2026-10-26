import { createMemo, createSignal, For, Match, Show, Switch } from "solid-js";
import { workItemProviderRegistry } from "../../plugins/workItemProviderRegistry";
import { setupForProvider } from "../../plugins/workItemSetup";
import { repositoriesStore } from "../../stores/repositories";
import { uiStore } from "../../stores/ui";
import type { WorkItemRef } from "../../types/work-items.gen";
import { Button } from "../ui/Button";
import { SegmentedControl } from "../ui/SegmentedControl";
import { Select } from "../ui/Select";
import { TasksCreate } from "./create/TasksCreate";
import { TasksDetail } from "./detail/TasksDetail";
import { TasksList } from "./list/TasksList";
import { TasksRoadmap } from "./roadmap/TasksRoadmap";
import { TasksSetup } from "./setup/TasksSetup";
import { TasksBoardView } from "./TasksBoardView";
import s from "./TasksShell.module.css";

type View = "list" | "board" | "roadmap";
const VIEWS: readonly { id: View; label: string }[] = [
	{ id: "list", label: "List" },
	{ id: "board", label: "Board" },
	{ id: "roadmap", label: "Roadmap" },
];
const sourceKey = (providerId: string, accountId: string) => JSON.stringify([providerId, accountId]);

/** Core Tasks tab: mounts the host-owned work-item views over the registered providers. */
export function TasksShell(props: { visible?: () => boolean }) {
	const [localSource, setLocalSource] = createSignal("");
	const [localView, setLocalView] = createSignal<View>("list");
	const source = () =>
		!repoPath() ? localSource() : (uiStore.state.tasksViewPreferences[JSON.stringify([repoPath()])]?.source ?? "");
	const viewKey = () => JSON.stringify([repoPath(), provider()?.providerId, provider()?.accountId]);
	const view = () => (!repoPath() ? localView() : (uiStore.state.tasksViewPreferences[viewKey()]?.view ?? "list"));
	const setView = (next: View) =>
		repoPath() ? uiStore.setTaskViewPreference(viewKey(), { view: next }) : setLocalView(next);
	const [creating, setCreating] = createSignal(false);
	const [configuring, setConfiguring] = createSignal(false);
	const [reference, setReference] = createSignal<WorkItemRef | null>(null);
	const repoPath = () => repositoriesStore.state.activeRepoPath ?? undefined;
	const providers = () => workItemProviderRegistry.list();
	const provider = createMemo(
		() => providers().find((entry) => sourceKey(entry.providerId, entry.accountId) === source()) ?? providers()[0],
	);
	// A roadmap needs a start or target date; a provider without a date field has nothing to lay out.
	const views = (hasDates: boolean) => (hasDates ? VIEWS : VIEWS.filter((entry) => entry.id !== "roadmap"));
	function chooseSource(key: string) {
		if (repoPath()) uiStore.setTaskViewPreference(JSON.stringify([repoPath()]), { source: key });
		else setLocalSource(key);
		setReference(null);
		setCreating(false);
		setConfiguring(false);
	}
	function open(next: WorkItemRef) {
		setCreating(false);
		setConfiguring(false);
		setReference(next);
	}
	return (
		<div class={s.shell} aria-label="Tasks shell">
			<Show
				when={provider()}
				fallback={
					<div class={s.fallback}>
						<p class={s.empty}>No work-item provider is set up. Install and configure one to show tasks.</p>
						<TasksSetup />
					</div>
				}
			>
				{(current) => {
					const hasDates = () => current().fields.some((field) => field.kind === "date");
					const activeView = () => (view() === "roadmap" && !hasDates() ? "list" : view());
					const setup = () => setupForProvider(current().providerId);
					return (
						<>
							<header class={s.header}>
								<Show when={providers().length > 1}>
									<label>
										Source
										<Select
											aria-label="Tasks source"
											value={sourceKey(current().providerId, current().accountId)}
											onChange={(event) => chooseSource(event.currentTarget.value)}
										>
											<For each={providers()}>
												{(entry) => <option value={sourceKey(entry.providerId, entry.accountId)}>{entry.name}</option>}
											</For>
										</Select>
									</label>
								</Show>
								<SegmentedControl
									options={views(hasDates())}
									value={creating() || configuring() ? undefined : activeView()}
									aria-label="Tasks view"
									onChange={(id) => {
										setCreating(false);
										setConfiguring(false);
										setReference(null);
										setView(id);
									}}
								/>
								<Button
									type="button"
									class={s.create}
									onClick={() => {
										setCreating(true);
										setConfiguring(false);
										setReference(null);
									}}
								>
									New task
								</Button>
								<Show when={setup()}>
									<Button
										type="button"
										aria-pressed={configuring()}
										onClick={() => {
											setConfiguring(!configuring());
											setCreating(false);
											setReference(null);
										}}
									>
										Provider settings
									</Button>
								</Show>
							</header>
							<div class={s.body} data-detail={reference() ? "open" : "closed"}>
								<div class={s.main}>
									<Switch>
										<Match when={configuring()}>
											<TasksSetup pluginId={setup()?.pluginId} />
										</Match>
										<Match when={creating()}>
											<TasksCreate
												provider={current()}
												onCreated={(detail) => open(detail.item.reference)}
												onCancel={() => setCreating(false)}
											/>
										</Match>
										<Match when={activeView() === "list"}>
											<TasksList
												providers={[current()]}
												repoPath={repoPath()}
												onOpen={open}
												selected={reference()}
												visible={props.visible?.()}
											/>
										</Match>
										<Match when={activeView() === "board"}>
											<TasksBoardView
												provider={current()}
												onOpen={open}
												visible={props.visible?.()}
												repoPath={repoPath()}
											/>
										</Match>
										<Match when={activeView() === "roadmap"}>
											<TasksRoadmap provider={current()} onOpen={open} visible={props.visible?.()} />
										</Match>
									</Switch>
								</div>
								<Show when={reference()}>
									<aside class={s.side}>
										<Button type="button" class={s.back} onClick={() => setReference(null)}>
											Back to list
										</Button>
										<TasksDetail
											provider={current()}
											reference={reference()}
											visible={props.visible?.()}
											repoPath={repoPath()}
											onClose={() => setReference(null)}
										/>
									</aside>
								</Show>
							</div>
						</>
					);
				}}
			</Show>
		</div>
	);
}
