import { createEffect, createMemo, createSignal, createUniqueId, For, on, onCleanup, Show } from "solid-js";
import { Button } from "../ui/Button";
import type { ActivityDescriptor } from "./catalogTypes";
import { NeutralScopeGraph } from "./NeutralScopeGraph";
import type { NeutralDefinition } from "./neutralClient";
import { connectRoute, GRAPH_NODE_HEIGHT, GRAPH_NODE_WIDTH, graphLayout, nodePorts, nodeTitle } from "./neutralGraph";
import s from "./WorkflowDesigner.module.css";

export function NeutralGraphEditor(props: {
	documentKey?: number;
	definition: NeutralDefinition;
	activities: ActivityDescriptor[];
	selected: string;
	labels: Record<string, string>;
	selectedBody?: number;
	onInspectBody?: (ordinal: number) => void;
	onSelect: (id: string) => void;
	onChange: (definition: NeutralDefinition) => void;
}) {
	const layout = createMemo(() => graphLayout(props.definition));
	const [openScope, setOpenScope] = createSignal(false);
	const scopeSelected = () =>
		["fork", "collection", "loop"].includes(props.definition.control.nodes[props.selected]?.type);
	createEffect(
		on(
			() => [props.selected, props.documentKey],
			() => setOpenScope(false),
		),
	);
	const [zoom, setZoom] = createSignal<number>();
	const [size, setSize] = createSignal({ width: 800, height: 600 });
	const [connecting, setConnecting] = createSignal<{ id: string; port: string }>();
	const scale = () =>
		zoom() ?? Math.min(1, Math.max(0.15, Math.min(size().width / layout().width, size().height / layout().height)));
	const marker = `workflow-arrow-${createUniqueId()}`;
	let viewport: HTMLDivElement | undefined;
	// The canvas element is recreated when a scope view replaces it, so every new element is observed.
	const observer =
		typeof ResizeObserver === "undefined"
			? undefined
			: new ResizeObserver(([entry]) => setSize({ width: entry.contentRect.width, height: entry.contentRect.height }));
	onCleanup(() => observer?.disconnect());
	createEffect(
		on(
			() => props.documentKey,
			() => {
				setConnecting(undefined);
				setZoom(undefined);
				viewport?.scrollTo?.(0, 0);
			},
		),
	);
	createEffect(() => {
		const pending = connecting();
		if (
			pending &&
			(!props.definition.control.nodes[pending.id] ||
				!nodePorts(props.definition.control.nodes[pending.id], props.definition, props.activities).includes(
					pending.port,
				))
		)
			setConnecting(undefined);
	});
	const title = (id: string) => nodeTitle(id, props.definition, props.labels);
	const edges = () =>
		Object.entries(props.definition.control.edges).flatMap(([from, routes]) =>
			Object.entries(routes).map(([port, to]) => ({ from, port, to })),
		);
	const broken = () => edges().filter((edge) => !layout().positions[edge.from] || !layout().positions[edge.to]);
	function select(id: string) {
		const pending = connecting();
		if (pending) {
			props.onChange(connectRoute(props.definition, pending.id, pending.port, id));
			setConnecting(undefined);
		}
		props.onSelect(id);
	}
	return (
		<section class={s.graphWorkspace} aria-label="Neutral control graph">
			<div class={s.graphToolbar}>
				<strong>Control graph</strong>
				<span class={s.muted}>
					{Object.keys(props.definition.control.nodes).length} nodes · {edges().length} routes
				</span>
				<div class={s.actions}>
					<Button aria-label="Zoom out" onClick={() => setZoom(Math.max(0.15, scale() - 0.15))}>
						−
					</Button>
					<span class={s.muted}>{Math.round(scale() * 100)}%</span>
					<Button aria-label="Zoom in" onClick={() => setZoom(Math.min(1.5, scale() + 0.15))}>
						+
					</Button>
					<Button
						onClick={() => {
							setZoom(undefined);
							viewport?.scrollTo?.(0, 0);
						}}
					>
						Fit graph
					</Button>
				</div>
			</div>
			<nav class={s.graphPorts} aria-label="Workflow graph location">
				<Button disabled={!openScope()} onClick={() => setOpenScope(false)}>
					Workflow graph
				</Button>
				<Show when={scopeSelected() && props.onInspectBody}>
					<Button
						aria-pressed={openScope()}
						onClick={() => {
							setConnecting(undefined);
							setOpenScope(true);
						}}
					>
						Open scope · {title(props.selected)}
					</Button>
				</Show>
			</nav>
			<Show
				when={!openScope()}
				fallback={
					<NeutralScopeGraph
						definition={props.definition}
						nodeId={props.selected}
						selectedBody={props.selectedBody}
						onInspectBody={(ordinal) => props.onInspectBody?.(ordinal)}
					/>
				}
			>
				<div class={s.graphPorts} aria-label="Selected node outgoing routes">
					<For
						each={
							props.definition.control.nodes[props.selected]
								? nodePorts(props.definition.control.nodes[props.selected], props.definition, props.activities)
								: []
						}
					>
						{(port) => (
							<Button
								size="compact"
								aria-label={`Connect ${title(props.selected)} ${port}`}
								aria-pressed={connecting()?.id === props.selected && connecting()?.port === port}
								onClick={() => setConnecting({ id: props.selected, port })}
							>
								{port}
							</Button>
						)}
					</For>
				</div>
				<Show
					when={connecting()}
					fallback={
						<p class={s.graphHint}>Select a node to configure it. Choose an outgoing route, then its destination.</p>
					}
				>
					{(pending) => (
						<div class={s.graphHint} role="status">
							Connect {title(pending().id)} · {pending().port}: choose a destination node.{" "}
							<Button onClick={() => setConnecting(undefined)}>Cancel connection</Button>
						</div>
					)}
				</Show>
				<Show when={broken().length}>
					<p class={s.error}>
						Some routes reference missing nodes. Use validation and the route inspector to repair them.
					</p>
				</Show>
				<div
					class={s.graphViewport}
					ref={(element) => {
						viewport = element;
						observer?.observe(element);
					}}
					tabIndex={0}
					aria-label="Scrollable workflow canvas"
					onKeyDown={(event) => {
						if (event.key === "Escape") setConnecting(undefined);
					}}
				>
					<div style={{ width: `${layout().width * scale()}px`, height: `${layout().height * scale()}px` }}>
						<div
							class={s.graphStage}
							style={{ width: `${layout().width}px`, height: `${layout().height}px`, transform: `scale(${scale()})` }}
						>
							<svg
								class={s.graphEdges}
								width={layout().width}
								height={layout().height}
								aria-label="Named control routes"
							>
								<defs>
									<marker
										id={marker}
										viewBox="0 0 10 10"
										refX="9"
										refY="5"
										markerWidth="7"
										markerHeight="7"
										orient="auto-start-reverse"
									>
										<path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
									</marker>
								</defs>
								<For each={edges()}>
									{(edge) => {
										const a = () => layout().positions[edge.from];
										const b = () => layout().positions[edge.to];
										const siblings = () => Object.keys(props.definition.control.edges[edge.from] ?? {});
										const x = () =>
											a().x + (GRAPH_NODE_WIDTH * (siblings().indexOf(edge.port) + 1)) / (siblings().length + 1);
										const y = () => a().y + GRAPH_NODE_HEIGHT;
										const tx = () => b().x + GRAPH_NODE_WIDTH / 2;
										return (
											<Show when={a() && b()}>
												<g>
													<title>
														{title(edge.from)} · {edge.port} → {title(edge.to)}
													</title>
													<path
														d={`M ${x()} ${y()} C ${x()} ${y() + 48}, ${tx()} ${b().y - 48}, ${tx()} ${b().y}`}
														fill="none"
														stroke="currentColor"
														stroke-width="1.5"
														marker-end={`url(#${marker})`}
													/>
													<text x={x() + 6} y={y() + 24}>
														{edge.port}
													</text>
												</g>
											</Show>
										);
									}}
								</For>
							</svg>
							<For each={Object.entries(props.definition.control.nodes)}>
								{([id, node]) => (
									<div
										class={s.graphNodeSlot}
										style={{ left: `${layout().positions[id].x}px`, top: `${layout().positions[id].y}px` }}
									>
										<button
											type="button"
											data-control-layout="card"
											class={s.graphNode}
											aria-pressed={props.selected === id}
											onClick={() => select(id)}
										>
											<strong>{title(id)}</strong>
											<small>
												{node.type}
												{node.type === "activity" ? ` · activity ${node.activity}` : ""}
											</small>
										</button>
									</div>
								)}
							</For>
						</div>
					</div>
				</div>
			</Show>
		</section>
	);
}
