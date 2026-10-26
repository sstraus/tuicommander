import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createMemo, For, Show } from "solid-js";
import type { WorkItem, WorkItemGroup, WorkItemRef, WorkItemTone } from "../../../types/work-items.gen";
import { formatRelativeTime } from "../../../utils/formatRelativeTime";
import s from "./tasksList.module.css";

interface TaskRowsProps {
	items: WorkItem[];
	groups: WorkItemGroup[];
	loading: boolean;
	resetKey: unknown;
	selected?: WorkItemRef | null;
	/** Branch of the worktree linked to the task, when there is one. */
	linkedBranch?: (reference: WorkItemRef) => string | undefined;
	onOpen: (reference: WorkItemRef) => void;
}
// One shape per tone so state never relies on colour alone.
const STATE_SHAPES: Record<WorkItemTone, string> = {
	neutral: "M6 1.5a4.5 4.5 0 1 0 0 9 4.5 4.5 0 0 0 0-9zm0 1.5a3 3 0 1 1 0 6 3 3 0 0 1 0-6z",
	info: "M6 1.5a4.5 4.5 0 1 0 0 9 4.5 4.5 0 0 0 0-9zM6 3a3 3 0 0 1 0 6z",
	success: "M2 6.2l1.2-1.2 1.8 1.8 3.8-3.8L10 4.2 5 9.2z",
	warning: "M6 1.5l5 9H1z",
	danger: "M3 2l3 3 3-3 1 1-3 3 3 3-1 1-3-3-3 3-1-1 3-3-3-3z",
};
const sameRef = (a: WorkItemRef, b: WorkItemRef | null | undefined) =>
	!!b && a.providerId === b.providerId && a.accountId === b.accountId && a.id === b.id;
const assigneeLabel = (item: WorkItem) =>
	item.assignees.length
		? `${item.assignees[0]?.name}${item.assignees.length > 1 ? ` +${item.assignees.length - 1}` : ""}`
		: "Unassigned";
const age = (updatedAt: string) => formatRelativeTime(Math.max(0, Date.now() - Date.parse(updatedAt)));

type DisplayRow = { kind: "group"; group: WorkItemGroup } | { kind: "item"; item: WorkItem; groupId: string };

/** Flatten only the provider's presentation groups; never infer group membership. */
export function TaskRows(props: TaskRowsProps) {
	let scroll: HTMLDivElement | undefined;
	const rows = createMemo<DisplayRow[]>(() =>
		props.groups.length
			? props.groups.flatMap((group): DisplayRow[] => [
					{ kind: "group", group },
					...group.items.map((item): DisplayRow => ({ kind: "item", item, groupId: group.id })),
				])
			: props.items.map((item) => ({ kind: "item", item, groupId: "" })),
	);
	createEffect(() => {
		props.resetKey;
		if (scroll) scroll.scrollTop = 0;
	});
	const virtualizer = createVirtualizer({
		get count() {
			return rows().length;
		},
		getScrollElement: () => scroll ?? null,
		estimateSize: () => 48,
		overscan: 5,
		getItemKey: (index) => {
			const row = rows()[index];
			return row?.kind === "group" ? `group:${row.group.id}` : JSON.stringify([row?.groupId, row?.item.reference]);
		},
	});
	return (
		<div class={s.viewport} ref={scroll} role="list" aria-label="Task results" aria-busy={props.loading}>
			<div class={s.canvas} style={{ height: `${virtualizer.getTotalSize()}px` }}>
				<For each={virtualizer.getVirtualItems()}>
					{(row) => {
						const entry = () => rows()[row.index];
						return (
							<div class={s.row} style={{ top: `${row.start}px`, height: `${row.size}px` }}>
								<Show when={entry()} keyed>
									{(current) => {
										if (current.kind === "group") return <h3 class={s.group}>{current.group.name}</h3>;
										return (
											<div role="listitem" aria-posinset={row.index + 1} aria-setsize={rows().length}>
												<button
													data-control-layout="row"
													type="button"
													class={s.item}
													aria-current={sameRef(current.item.reference, props.selected) ? "true" : undefined}
													onClick={() => props.onOpen(current.item.reference)}
												>
													<span class={s.key}>{current.item.key}</span>
													<span class={s.title}>{current.item.title}</span>
													<Show when={props.linkedBranch?.(current.item.reference)}>
														{(branch) => (
															<svg
																class={s.linked}
																role="img"
																aria-label={`Linked to worktree ${branch()}`}
																viewBox="0 0 12 12"
																width="12"
																height="12"
																fill="currentColor"
															>
																<path d="M3.5 1a1.5 1.5 0 0 1 .5 2.915v.5c0 .3.2.585.5.585H7a2 2 0 0 1 1.5.7V5.9a1.5 1.5 0 1 1 1 0v.6A3 3 0 0 0 7.2 5.5H4.5c-.17 0-.34-.03-.5-.08v2.665a1.5 1.5 0 1 1-1 0V3.915A1.5 1.5 0 0 1 3.5 1z" />
															</svg>
														)}
													</Show>
													<span class={s.assignee}>{assigneeLabel(current.item)}</span>
													<time class={s.age} datetime={current.item.updatedAt} title={current.item.updatedAt}>
														{age(current.item.updatedAt)}
													</time>
													<span class={s.state} data-tone={current.item.state.tone}>
														<svg
															data-state-icon
															viewBox="0 0 12 12"
															width="12"
															height="12"
															fill="currentColor"
															aria-hidden="true"
														>
															<path d={STATE_SHAPES[current.item.state.tone]} />
														</svg>
														{current.item.state.name}
													</span>
												</button>
											</div>
										);
									}}
								</Show>
							</div>
						);
					}}
				</For>
			</div>
		</div>
	);
}
