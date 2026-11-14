import { For, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import type { ActivityDescriptor } from "./catalogTypes";
import type { ControlNode, NeutralDefinition } from "./neutralClient";
import s from "./WorkflowDesigner.module.css";

export function NeutralGraphEditor(props: {
	definition: NeutralDefinition;
	activities: ActivityDescriptor[];
	selected: string;
	labels: Record<string, string>;
	onSelect: (id: string) => void;
	onChange: (definition: NeutralDefinition) => void;
}) {
	const replaceNode = (id: string, node: ControlNode) =>
		props.onChange({
			...props.definition,
			control: { ...props.definition.control, nodes: { ...props.definition.control.nodes, [id]: node } },
		});
	const connect = (id: string, port: string, target: string) => {
		const { [port]: _removed, ...remaining } = props.definition.control.edges[id] ?? {};
		props.onChange({
			...props.definition,
			control: {
				...props.definition.control,
				edges: { ...props.definition.control.edges, [id]: target ? { ...remaining, [port]: target } : remaining },
			},
		});
	};
	function add(node: ControlNode) {
		const id = crypto.randomUUID();
		replaceNode(id, node);
		props.onSelect(id);
	}
	// Ports for neutral controls are part of ControlGraph's published contract;
	// activity ports always come from the pinned catalog descriptor.
	const ports = (node: ControlNode): string[] => {
		switch (node.type) {
			case "start":
			case "wait":
				return ["next"];
			case "end":
				return [];
			case "condition":
				return ["true", "false", "unknown"];
			case "collection":
			case "fork":
				return ["complete", "blocked"];
			case "loop":
				return ["complete", "exhausted", "unknown", "failed"];
			case "activity": {
				const instance = props.definition.instances[node.activity];
				return (
					props.activities.find(
						(entry) =>
							entry.id.namespace === instance?.descriptor.namespace &&
							entry.id.name === instance?.descriptor.name &&
							entry.id.version === instance?.descriptor.version,
					)?.outcomes ?? []
				);
			}
		}
	};
	return (
		<section class={s.canvas} aria-label="Neutral control graph">
			<h4>Control graph</h4>
			<div class={s.actions}>
				<Button onClick={() => add({ type: "wait", duration_ms: 1000 })}>Add wait</Button>
				<Button onClick={() => add({ type: "condition", condition: "" })}>Add condition</Button>
				<Button onClick={() => add({ type: "fork", scope: "" })}>Add fork</Button>
				<Button onClick={() => add({ type: "collection", scope: "" })}>Add collection</Button>
				<Button onClick={() => add({ type: "loop", scope: "", condition: "" })}>Add loop</Button>
				<Button onClick={() => add({ type: "end", outcome: "completed" })}>Add end</Button>
			</div>
			<For each={Object.entries(props.definition.control.nodes)}>
				{([id, node]) => (
					<section>
						<button
							type="button"
							data-control-layout="row"
							class={s.draft}
							aria-current={props.selected === id ? "true" : undefined}
							onClick={() => props.onSelect(id)}
						>
							<strong>{props.labels[id] || id}</strong>
							<small>
								{node.type}
								{node.type === "activity" ? ` · activity ${node.activity}` : ""}
							</small>
						</button>
						<Show when={props.selected === id}>
							<Show when={node.type === "wait"}>
								<label>
									Wait milliseconds
									<Input
										aria-label="Wait milliseconds"
										type="number"
										min="1"
										value={node.type === "wait" ? node.duration_ms : 1}
										onInput={(event) =>
											replaceNode(id, { type: "wait", duration_ms: Number(event.currentTarget.value) })
										}
									/>
								</label>
							</Show>
							<Show when={"condition" in node}>
								<label>
									Condition key
									<Input
										aria-label="Condition key"
										value={"condition" in node ? node.condition : ""}
										onInput={(event) => {
											if ("condition" in node) replaceNode(id, { ...node, condition: event.currentTarget.value });
										}}
									/>
								</label>
							</Show>
							<Show when={"scope" in node}>
								<label>
									Scope key
									<Input
										aria-label="Scope key"
										value={"scope" in node ? node.scope : ""}
										onInput={(event) => {
											if ("scope" in node) replaceNode(id, { ...node, scope: event.currentTarget.value });
										}}
									/>
								</label>
							</Show>
							<Show when={node.type === "end"}>
								<label>
									End outcome
									<Input
										aria-label="End outcome"
										value={node.type === "end" ? node.outcome : ""}
										onInput={(event) => replaceNode(id, { type: "end", outcome: event.currentTarget.value })}
									/>
								</label>
							</Show>
							<For each={ports(node)}>
								{(port) => (
									<label>
										{port}
										<Select
											aria-label={`${id} ${port} target`}
											value={props.definition.control.edges[id]?.[port] ?? ""}
											onChange={(event) => connect(id, port, event.currentTarget.value)}
										>
											<option value="">Unconnected</option>
											<For each={Object.keys(props.definition.control.nodes)}>
												{(target) => <option value={target}>{target}</option>}
											</For>
										</Select>
									</label>
								)}
							</For>
						</Show>
					</section>
				)}
			</For>
		</section>
	);
}
