import { createEffect, createMemo, createSignal, For, Show } from "solid-js";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import { tasksNotifications } from "../../../services/tasksNotifications";
import { appLogger } from "../../../stores/appLogger";
import type { WorkItemDetail, WorkItemRef } from "../../../types/work-items.gen";
import { writeClipboard } from "../../../utils/clipboard";
import { handleOpenUrl } from "../../../utils/openUrl";
import { Button } from "../../ui/Button";
import { Textarea } from "../../ui/Textarea";
import { START_WORK_NEEDS_REPO, suggestedBranch, useStartWork } from "../startWork";
import { TaskComment } from "./TaskComment";
import { TaskField } from "./TaskField";
import { TaskMarkdown } from "./TaskMarkdown";
import styles from "./TasksDetail.module.css";
import { commentKey, useTasksDetail } from "./useTasksDetail";

export interface TasksDetailProps {
	provider: WorkItemProvider;
	reference: WorkItemRef | null;
	visible?: boolean;
	/** Repository that Start work creates its worktree in. */
	repoPath?: string;
	onClose?: () => void;
	onItemChange?: (detail: WorkItemDetail) => void;
}

/** Up to this many fields stay open; a wider schema folds so Description and Comments come first. */
const INLINE_FIELD_LIMIT = 8;

/** Core-owned detail surface. Providers supply data and declared actions, never markup. */
export function TasksDetail(props: TasksDetailProps) {
	const model = useTasksDetail(props);
	const start = useStartWork();
	const [draft, setDraft] = createSignal("");
	createEffect(() => {
		model.identity();
		props.provider;
		setDraft("");
		start.reset();
	});
	const busy = () => Boolean(model.pending()) || model.paging() || start.pending();
	const commentEntries = createMemo(
		() => new Map(model.detail()?.comments.entries.map((entry) => [commentKey(entry), entry])),
	);
	const transitions = () => model.detail()?.actions.filter((action) => action.kind === "transition") ?? [];
	const startActions = () => model.detail()?.actions.filter((action) => action.kind === "start_work") ?? [];
	const commentAction = () => model.detail()?.actions.find((action) => action.kind === "comment");
	const editAction = () => model.detail()?.actions.find((action) => action.kind === "edit_field");
	function runStart() {
		const value = model.detail();
		if (value && props.repoPath) void start.run(props.provider, value, props.repoPath);
	}
	async function submitComment(event: SubmitEvent) {
		event.preventDefault();
		const body = draft();
		if (!body.trim() || !commentAction()?.enabled || busy()) return;
		if (await model.act({ kind: "comment", value: body }, "Adding comment…")) setDraft("");
	}
	return (
		<section class={styles.detail} aria-label="Task detail" hidden={props.visible === false || !props.reference}>
			<Show when={model.detail()} fallback={<p>{model.loading() ? "Loading task…" : "No task selected"}</p>}>
				{(detail) => (
					<>
						<header class={styles.header}>
							<div class={styles.identity}>
								<span class={styles.key}>{detail().item.key}</span>
								<span>{detail().item.itemType}</span>
							</div>
							<div class={styles.toolbar}>
								<Button
									type="button"
									aria-pressed={tasksNotifications.followed(detail().item.reference)}
									onClick={() => {
										tasksNotifications.setFollowed(
											detail().item.reference,
											!tasksNotifications.followed(detail().item.reference),
										);
										void tasksNotifications.observe(detail(), true);
									}}
								>
									{tasksNotifications.followed(detail().item.reference) ? "Unfollow task" : "Follow task"}
								</Button>
								<Button type="button" onClick={() => handleOpenUrl(detail().item.url)}>
									Open in provider
								</Button>
								<Button
									type="button"
									onClick={() => {
										void model.refresh();
									}}
									disabled={model.loading() || busy()}
								>
									Refresh
								</Button>
								<Show when={props.onClose}>
									<Button type="button" onClick={props.onClose}>
										Close detail
									</Button>
								</Show>
							</div>
							<h2>{detail().item.title}</h2>
							<Show when={detail().item.assignees.length || detail().item.labels.length}>
								<div class={styles.metadata}>
									<For each={detail().item.assignees}>{(user) => <span>{user.name}</span>}</For>
									<For each={detail().item.labels}>{(label) => <span class={styles.label}>{label.name}</span>}</For>
								</div>
							</Show>
							<div class={styles.statusRow}>
								<span
									class={styles.state}
									data-tone={detail().item.state.tone}
									role="status"
									aria-live="polite"
									aria-busy={model.pendingKind() === "transition"}
								>
									{model.pendingKind() === "transition" ? `${model.pending()}…` : detail().item.state.name}
								</span>
								<span class={styles.muted}>Updated {new Date(detail().item.updatedAt).toLocaleString()}</span>
							</div>
							<div class={styles.actions}>
								<For each={transitions()}>
									{(action) => (
										<Button
											type="button"
											disabled={busy() || !action.enabled}
											title={action.disabledReason ?? undefined}
											onClick={async () => {
												await model.act({ kind: "transition", value: action.id }, action.name);
											}}
										>
											{action.name}
										</Button>
									)}
								</For>
								<For each={startActions()}>
									{(action) => (
										<Button
											type="button"
											disabled={busy() || !action.enabled || !props.repoPath}
											title={action.disabledReason ?? (props.repoPath ? undefined : START_WORK_NEEDS_REPO)}
											onClick={runStart}
										>
											{start.pending() ? "Starting…" : action.name}
										</Button>
									)}
								</For>
								<Button
									type="button"
									onClick={() => {
										writeClipboard(suggestedBranch(detail().item)).catch((cause) =>
											appLogger.error("app", "Failed to copy branch name", cause),
										);
									}}
								>
									Copy branch name
								</Button>
							</div>
							<Show when={start.startedBranch()}>
								{(branch) => (
									<p class={styles.muted} role="status">
										Started work on {branch()}
									</p>
								)}
							</Show>
							<Show when={start.failure()}>
								{(message) => (
									<p class={styles.error} role="alert">
										{message()}
										<Button type="button" disabled={busy()} onClick={runStart}>
											Retry start work
										</Button>
									</p>
								)}
							</Show>
						</header>
						<div class={styles.content}>
							<section class={styles.description} aria-label="Description">
								<h3>Description</h3>
								<TaskMarkdown content={detail().body} />
							</section>
							<details class={styles.fieldGroup} open={detail().fields.length <= INLINE_FIELD_LIMIT}>
								<summary>Fields ({detail().fields.length})</summary>
								<section class={styles.fields} aria-label="Task fields">
									<For each={detail().fields.map((field) => field.id)}>
										{(id) => {
											const schema = () => detail().fields.find((field) => field.id === id);
											return (
												<Show when={schema()}>
													{(field) => (
														<TaskField
															schema={field()}
															value={detail().item.fields[id] ?? null}
															disabled={busy()}
															canEdit={editAction()?.enabled === true}
															onSave={(value) =>
																model.act(
																	{ kind: "edit_field", value: { fieldId: id, value } },
																	`Saving ${field().name}…`,
																)
															}
														/>
													)}
												</Show>
											);
										}}
									</For>
								</section>
							</details>
							<section aria-label="Comments">
								<h3>Comments</h3>
								<Show when={commentEntries().size > 0} fallback={<p class={styles.muted}>No comments yet</p>}>
									<For each={[...commentEntries().keys()]}>
										{(id) => <Show when={commentEntries().get(id)}>{(entry) => <TaskComment entry={entry()} />}</Show>}
									</For>
								</Show>
								<Show when={detail().comments.nextCursor}>
									<Button
										type="button"
										disabled={busy() || model.loading()}
										onClick={() => {
											void model.loadMore();
										}}
									>
										{model.paging() ? "Loading comments…" : "Load more comments"}
									</Button>
								</Show>
								<Show when={commentAction()}>
									{(action) => (
										<form class={styles.composer} onSubmit={submitComment}>
											<label>
												New comment
												<Textarea
													aria-label="New comment"
													placeholder="Write a comment in Markdown"
													value={draft()}
													disabled={busy() || !action().enabled}
													onInput={(event) => setDraft(event.currentTarget.value)}
												/>
											</label>
											<Button
												variant="primary"
												type="submit"
												title={action().disabledReason ?? undefined}
												disabled={busy() || !action().enabled || !draft().trim()}
											>
												Add comment
											</Button>
										</form>
									)}
								</Show>
							</section>
						</div>
					</>
				)}
			</Show>
			<Show when={model.error()}>
				<p class={styles.error} role="alert">
					{model.error()}
					<Button
						type="button"
						disabled={busy()}
						onClick={() => {
							void model.refresh();
						}}
					>
						Retry
					</Button>
				</p>
			</Show>
		</section>
	);
}
