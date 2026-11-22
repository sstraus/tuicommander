import { For, Show } from "solid-js";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import type { ActivityDescriptor } from "./catalogTypes";
import type { ControlNode, NeutralDefinition } from "./neutralClient";
import { connectRoute, nodePorts, nodeTitle } from "./neutralGraph";

/** Configuration is deliberately outside the canvas so graph selection never changes geometry. */
export function NeutralControlInspector(props: {
	definition: NeutralDefinition;
	activities: ActivityDescriptor[];
	selected: string;
	labels: Record<string, string>;
	onChange: (definition: NeutralDefinition) => void;
	onLabel: (label: string) => void;
}) {
	const node = () => props.definition.control.nodes[props.selected];
	function replace(next: ControlNode) {
		props.onChange({
			...props.definition,
			control: { ...props.definition.control, nodes: { ...props.definition.control.nodes, [props.selected]: next } },
		});
	}
	const title = (id: string) => nodeTitle(id, props.definition, props.labels);
	return (
		<Show when={node()}>
			{(current) => (
				<section aria-label="Control node inspector">
					<h4>{title(props.selected)}</h4>
					<Show when={current().type !== "activity"}>
						<label>
							Node label
							<Input
								value={props.labels[props.selected] ?? ""}
								onInput={(event) => props.onLabel(event.currentTarget.value)}
							/>
						</label>
					</Show>
					<Show when={current().type === "wait"}>
						<label>
							Wait milliseconds
							<Input
								type="number"
								min="1"
								value={(current() as { duration_ms: number }).duration_ms}
								onInput={(event) => replace({ type: "wait", duration_ms: Number(event.currentTarget.value) })}
							/>
						</label>
					</Show>
					<Show when={"condition" in current()}>
						<label>
							Condition key
							<Input
								value={(current() as { condition: string }).condition}
								onInput={(event) => {
									const n = current();
									if ("condition" in n) replace({ ...n, condition: event.currentTarget.value });
								}}
							/>
						</label>
					</Show>
					<Show when={"scope" in current()}>
						<label>
							Scope key
							<Input
								value={(current() as { scope: string }).scope}
								onInput={(event) => {
									const n = current();
									if ("scope" in n) replace({ ...n, scope: event.currentTarget.value });
								}}
							/>
						</label>
					</Show>
					<Show when={current().type === "end"}>
						<label>
							End outcome
							<Input
								value={(current() as { outcome: string }).outcome}
								onInput={(event) => replace({ type: "end", outcome: event.currentTarget.value })}
							/>
						</label>
					</Show>
					<h5>Outgoing routes</h5>
					{/* Recreate the route rows per node: a reused Select keeps the previous node's value before its options reconcile. */}
					<Show when={props.selected} keyed>
						<For
							each={[
								...new Set([
									...nodePorts(current(), props.definition, props.activities),
									...Object.keys(props.definition.control.edges[props.selected] ?? {}),
								]),
							]}
						>
							{(port) => (
								<label>
									{port}
									<Select
										aria-label={`${title(props.selected)} ${port} target`}
										value={props.definition.control.edges[props.selected]?.[port] ?? ""}
										onChange={(event) =>
											props.onChange(connectRoute(props.definition, props.selected, port, event.currentTarget.value))
										}
									>
										<For
											each={[
												"",
												...Object.keys(props.definition.control.nodes),
												...(props.definition.control.edges[props.selected]?.[port] &&
												!props.definition.control.nodes[props.definition.control.edges[props.selected][port]]
													? [props.definition.control.edges[props.selected][port]]
													: []),
											]}
										>
											{(id, index) => (
												<option
													value={id}
													selected={id === (props.definition.control.edges[props.selected]?.[port] ?? "")}
												>
													{!id
														? "Unconnected"
														: props.definition.control.nodes[id]
															? `${title(id)} · node ${index()}`
															: "Missing destination"}
												</option>
											)}
										</For>
									</Select>
								</label>
							)}
						</For>
					</Show>
					<Show when={current().type === "end"}>
						<p>This node ends the workflow with its named outcome.</p>
					</Show>
				</section>
			)}
		</Show>
	);
}
