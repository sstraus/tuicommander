import { For, Show } from "solid-js";
import type { JsonValue } from "./catalogTypes";
import type { NeutralDefinition } from "./neutralClient";
import s from "./WorkflowDesigner.module.css";

function record(value: JsonValue | undefined): Record<string, JsonValue> {
	return value !== null && typeof value === "object" && !Array.isArray(value) ? value : {};
}

/** Scope topology is a view of declarations, not a second executable control graph. */
export function NeutralScopeGraph(props: {
	definition: NeutralDefinition;
	nodeId: string;
	selectedBody?: number;
	onInspectBody: (ordinal: number) => void;
}) {
	const node = () => props.definition.control.nodes[props.nodeId];
	const kind = () => node()?.type;
	const contract = () => {
		const current = node();
		if (!current || !("scope" in current)) return {};
		const table =
			current.type === "fork"
				? props.definition.forks
				: current.type === "loop"
					? props.definition.loops
					: props.definition.collections;
		return record(table[current.scope]);
	};
	const bodies = () =>
		kind() === "fork"
			? Object.entries(record(contract().branches)).map(([name, branch]) => ({
					name,
					ordinal: Number(record(branch).activity),
					required: record(branch).required !== false,
				}))
			: [
					{
						name: kind() === "loop" ? "Iteration body" : "Item body",
						ordinal: Number(contract().body),
						required: true,
					},
				];
	const width = () => Math.max(1, bodies().length) * 240 + 48;
	const bodyTitle = (ordinal: number) => props.definition.instances[ordinal]?.descriptor.name ?? "Missing activity";
	return (
		<section class={s.scopeTopology} aria-label="Scope body graph">
			<p class={s.graphHint}>
				Declared scope bodies · select an activity to configure its typed inputs and outputs. These lines describe scope
				semantics, not editable control routes.
			</p>
			<div class={s.scopeViewport}>
				<div class={s.scopeStage} style={{ width: `${width()}px` }}>
					<svg class={s.graphEdges} width={width()} height="330" aria-label="Declared scope topology">
						<For each={bodies()}>
							{(_, index) => {
								const x = () => 144 + index() * 240;
								return (
									<g>
										<path
											d={`M ${width() / 2} 42 L ${x()} 100 M ${x()} 184 L ${width() / 2} 248`}
											fill="none"
											stroke="currentColor"
											stroke-width="1.5"
										/>
										<text x={x() + 6} y="80">
											{kind() === "fork" ? "branch" : kind() === "loop" ? "carried state" : "item + index"}
										</text>
									</g>
								);
							}}
						</For>
						<Show when={kind() === "loop"}>
							<path
								d={`M ${width() / 2} 265 C ${width() - 12} 265, ${width() - 12} 25, ${width() / 2} 25`}
								fill="none"
								stroke="currentColor"
								stroke-width="1.5"
								stroke-dasharray="5 4"
							/>
							<text x="20" y="310">
								direct body outputs → next carried state · bounded feedback
							</text>
						</Show>
					</svg>
					<div class={s.scopeBoundary} style={{ left: `${width() / 2 - 110}px`, top: "8px" }}>
						{kind() === "fork"
							? "Parallel fanout"
							: kind() === "loop"
								? `While condition · max ${contract().max_iterations ?? "?"}`
								: `For each item · max ${contract().max_items ?? "?"}`}
					</div>
					<For each={bodies()}>
						{(body, index) => (
							<div class={s.graphNodeSlot} style={{ left: `${46 + index() * 240}px`, top: "100px" }}>
								<button
									type="button"
									data-control-layout="card"
									class={s.graphNode}
									aria-pressed={props.selectedBody === body.ordinal}
									disabled={!props.definition.instances[body.ordinal]}
									onClick={() => props.onInspectBody(body.ordinal)}
								>
									<strong>
										{body.name} · {bodyTitle(body.ordinal)}
									</strong>
									<small>
										activity {Number.isInteger(body.ordinal) ? body.ordinal : "missing"}
										{kind() === "fork" ? (body.required ? " · required success" : " · optional success") : ""}
									</small>
								</button>
							</div>
						)}
					</For>
					<div class={s.scopeBoundary} style={{ left: `${width() / 2 - 110}px`, top: "248px" }}>
						{kind() === "fork"
							? "All join · complete / blocked"
							: kind() === "loop"
								? "complete / exhausted / unknown / failed"
								: "Ordered result envelopes"}
					</div>
				</div>
			</div>
			<Show when={kind() === "fork" && bodies().length === 0}>
				<p class={s.graphHint}>No branches declared. Add named branches in the scope inspector.</p>
			</Show>
		</section>
	);
}
