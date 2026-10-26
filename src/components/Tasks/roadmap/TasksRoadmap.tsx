import { createEffect, createMemo, createSignal, onCleanup, Show, untrack } from "solid-js";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import { createTasksQuery } from "../../../stores/tasksQuery";
import type { WorkItemQuery, WorkItemRef } from "../../../types/work-items.gen";
import { Button } from "../../ui/Button";
import { Select } from "../../ui/Select";
import { TaskRows } from "../list/TaskRows";
import listStyles from "../list/tasksList.module.css";
import { projectRoadmap, type RoadmapDateFields, type RoadmapZoom, roadmapWindow } from "./calendar";
import { RoadmapTimeline } from "./RoadmapTimeline";
import s from "./roadmap.module.css";

export interface TasksRoadmapProps extends RoadmapDateFields {
	provider?: WorkItemProvider;
	query?: WorkItemQuery;
	onOpen: (reference: WorkItemRef) => void;
	visible?: boolean;
	initialDate?: Date;
}
const allItems: WorkItemQuery = {
	filters: [],
	savedFilterId: null,
	search: "",
	sort: null,
	groupBy: null,
	cursor: null,
	limit: 100,
};

/** Host-owned roadmap. Providers supply typed dates; this component only lays out their pages. */
export function TasksRoadmap(props: TasksRoadmapProps) {
	// Roadmap consumes provider pages; complete-scope projection belongs to list controls.
	const model = createTasksQuery({ local: false });
	const [zoom, setZoom] = createSignal<RoadmapZoom>("month");
	const [anchor, setAnchor] = createSignal(props.initialDate ?? new Date());
	const window = createMemo(() => roadmapWindow(anchor(), zoom()));
	const projection = createMemo(() =>
		projectRoadmap(model.state().items, props.provider?.fields ?? [], props, window()),
	);
	createEffect(() => {
		const provider = props.provider;
		const query = props.query ?? allItems;
		if (props.visible === false) return;
		untrack(() => {
			if (provider) void model.select(provider, { ...query, groupBy: null, cursor: null });
			else model.clear();
		});
	});
	onCleanup(() => model.dispose());
	function move(direction: number) {
		setAnchor(
			new Date(window().start.getFullYear(), window().start.getMonth() + direction * (zoom() === "month" ? 1 : 3), 1),
		);
	}
	return (
		<section class={s.roadmap} aria-label="Tasks roadmap">
			<div class={s.toolbar}>
				<div class={s.navigation}>
					<Button type="button" aria-label="Previous period" onClick={() => move(-1)}>
						<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" aria-hidden="true">
							<path d="M10 2 4 8l6 6 1.4-1.4L6.8 8l4.6-4.6Z" />
						</svg>
					</Button>
					<h2>{window().label}</h2>
					<Button type="button" aria-label="Next period" onClick={() => move(1)}>
						<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" aria-hidden="true">
							<path d="m6 2 6 6-6 6-1.4-1.4L9.2 8 4.6 3.4Z" />
						</svg>
					</Button>
				</div>
				<Button type="button" onClick={() => setAnchor(new Date())}>
					Today
				</Button>
				<label>
					Zoom
					<Select
						aria-label="Roadmap zoom"
						value={zoom()}
						onChange={(event) => setZoom(event.currentTarget.value as RoadmapZoom)}
					>
						<option value="month">Month</option>
						<option value="quarter">Quarter</option>
					</Select>
				</label>
				<Button
					type="button"
					disabled={!props.provider || model.state().loading || props.visible === false}
					onClick={() => void model.refresh()}
				>
					Refresh
				</Button>
			</div>
			<div class={s.status} role="status" aria-live="polite">
				<span>
					{model.state().loading
						? "Loading tasks…"
						: `${model.state().items.length} loaded · ${projection().dated.length} dated · ${projection().undated.length} without usable dates`}
				</span>
				<Show when={model.state().stale}>
					<span class={s.warning}>Stale results</span>
				</Show>
				<Show when={model.state().error}>
					<span class={s.error}>{model.state().error}</span>
				</Show>
			</div>
			<Show when={props.provider} fallback={<p class={s.empty}>Install a work-item provider to show the roadmap.</p>}>
				<div class={s.content}>
					<div class={s.dated}>
						<Show when={projection().dated.length > 0} fallback={<p class={s.empty}>No dated tasks loaded.</p>}>
							<RoadmapTimeline
								rows={projection().dated}
								window={window()}
								zoom={zoom()}
								loading={model.state().loading}
								onOpen={props.onOpen}
							/>
						</Show>
					</div>
					<aside class={`${s.undated} ${listStyles.list}`} aria-label="Tasks without usable dates">
						<h3>Without usable dates ({projection().undated.length})</h3>
						<TaskRows
							items={projection().undated}
							groups={[]}
							loading={model.state().loading}
							resetKey={props.provider}
							onOpen={props.onOpen}
						/>
					</aside>
				</div>
			</Show>
			<Show when={model.state().nextCursor}>
				<Button
					class={s.more}
					type="button"
					disabled={model.state().loading || props.visible === false}
					onClick={() => void model.loadMore()}
				>
					Load more
				</Button>
			</Show>
		</section>
	);
}
