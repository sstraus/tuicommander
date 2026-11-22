import { createMemo, createUniqueId, For, Show } from "solid-js";
import { GRAPH_NODE_HEIGHT, GRAPH_NODE_WIDTH, graphLayout, nodeTitle } from "./neutralGraph";
import type { RunProgress } from "./neutralInspectionTypes";
import s from "./WorkflowDesigner.module.css";

/** A pinned, read-only graph. Node states come from complete backend replay, not the event page. */
export function RunGraph(props: { progress: RunProgress }) {
	const definition = () => props.progress.publication.definition;
	const layout = createMemo(() => graphLayout(definition()));
	const marker = `run-arrow-${createUniqueId()}`;
	return (
		<section aria-label="Pinned run graph" class={s.runGraph}>
			<h4>Run graph · revision {props.progress.publication.revision}</h4>
			<For each={props.progress.resource_waits}>
				{(wait) => (
					<p role="status">
						Waiting on resource {wait.resource}, held by run {wait.owner_run}
					</p>
				)}
			</For>
			<svg viewBox={`0 0 ${layout().width} ${layout().height}`} role="img" aria-label="Live run node states">
				<defs>
					<marker id={marker} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
						<path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
					</marker>
				</defs>
				<For each={Object.entries(definition().control.edges)}>
					{([from, routes]) => (
						<For each={Object.entries(routes)}>
							{([port, to]) => {
								const a = () => layout().positions[from];
								const b = () => layout().positions[to];
								return (
									<Show when={a() && b()}>
										<g class={s.runEdge}>
											<path
												d={`M ${a().x + GRAPH_NODE_WIDTH / 2} ${a().y + GRAPH_NODE_HEIGHT} L ${b().x + GRAPH_NODE_WIDTH / 2} ${b().y}`}
												fill="none"
												stroke="currentColor"
												marker-end={`url(#${marker})`}
											/>
											<text x={a().x + GRAPH_NODE_WIDTH / 2 + 8} y={a().y + GRAPH_NODE_HEIGHT + 20}>
												{port}
											</text>
										</g>
									</Show>
								);
							}}
						</For>
					)}
				</For>
				<For each={Object.keys(definition().control.nodes)}>
					{(id) => (
						<g
							class={s.runNode}
							data-state={props.progress.node_states[id] ?? "unvisited"}
							transform={`translate(${layout().positions[id].x},${layout().positions[id].y})`}
						>
							<title>
								{nodeTitle(id, definition(), props.progress.publication.editor.labels)} ·{" "}
								{props.progress.node_states[id] ?? "unvisited"}
							</title>
							<rect width={GRAPH_NODE_WIDTH} height={GRAPH_NODE_HEIGHT} rx="4" />
							<text x="12" y="28">
								{nodeTitle(id, definition(), props.progress.publication.editor.labels)}
							</text>
							<text x="12" y="50">
								{props.progress.node_states[id] ?? "unvisited"}
							</text>
						</g>
					)}
				</For>
			</svg>
		</section>
	);
}
