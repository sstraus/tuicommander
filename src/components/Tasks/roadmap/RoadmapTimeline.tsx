import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, For, Show } from "solid-js";
import type { WorkItemRef } from "../../../types/work-items.gen";
import type { RoadmapSpan, RoadmapZoom, roadmapWindow } from "./calendar";
import s from "./roadmap.module.css";

export function RoadmapTimeline(props: {
	rows: RoadmapSpan[];
	window: ReturnType<typeof roadmapWindow>;
	zoom: RoadmapZoom;
	loading: boolean;
	onOpen: (reference: WorkItemRef) => void;
}) {
	let viewport: HTMLDivElement | undefined;
	const virtualizer = createVirtualizer({
		get count() {
			return props.rows.length;
		},
		getScrollElement: () => viewport ?? null,
		estimateSize: () => 56,
		overscan: 5,
		getItemKey: (index) => JSON.stringify(props.rows[index]?.item.reference),
	});
	createEffect(() => {
		props.window;
		if (viewport) viewport.scrollLeft = 0;
	});
	return (
		<div class={s.timeline} ref={viewport} role="region" aria-label="Dated tasks timeline" aria-busy={props.loading}>
			<div class={s.sheet} style={{ "--track-width": `${props.window.days * (props.zoom === "month" ? 28 : 12)}px` }}>
				<div class={s.axis} aria-hidden="true">
					<span class={s.axisTitle}>Task / local dates</span>
					<div class={s.ticks}>
						<For each={props.window.ticks}>
							{(tick, index) => (
								<span title={tick.date}>
									{props.zoom === "month" ? tick.label : index() % 7 === 0 ? `${tick.month} ${tick.label}` : ""}
								</span>
							)}
						</For>
					</div>
				</div>
				<div
					role="list"
					aria-label="Dated tasks"
					class={s.canvas}
					style={{ height: `${virtualizer.getTotalSize()}px` }}
				>
					<For each={virtualizer.getVirtualItems()}>
						{(virtualRow) => {
							const row = () => props.rows[virtualRow.index];
							return (
								<Show when={row()}>
									{(current) => (
										<div
											role="listitem"
											class={s.row}
											style={{ top: `${virtualRow.start}px`, height: `${virtualRow.size}px` }}
										>
											<button
												data-control-layout="row"
												class={s.task}
												type="button"
												onClick={() => props.onOpen(current().item.reference)}
												title={`${current().item.title}: ${current().start} – ${current().target}`}
											>
												<span class={s.identity}>
													<span class={s.title}>
														{current().item.key} · {current().item.title}
													</span>
													<span class={s.dates}>
														<time dateTime={current().start}>{current().start}</time> –{" "}
														<time dateTime={current().target}>{current().target}</time>
													</span>
												</span>
												<span class={s.track}>
													<Show
														when={!current().outside}
														fallback={<span class={s.outside}>Outside visible range</span>}
													>
														<span
															class={s.bar}
															data-tone={current().item.state.tone}
															style={{ left: `${current().left}%`, width: `${current().width}%` }}
														>
															<span>{current().item.state.name}</span>
														</span>
													</Show>
												</span>
											</button>
										</div>
									)}
								</Show>
							);
						}}
					</For>
				</div>
			</div>
		</div>
	);
}
