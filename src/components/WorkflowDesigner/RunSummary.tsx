import { For, Show } from "solid-js";
import type { JsonValue } from "./catalogTypes";
import type { NeutralDefinition } from "./neutralClient";
import { nodeTitle } from "./neutralGraph";
import type { Inspection } from "./neutralInspectionTypes";
import s from "./WorkflowDesigner.module.css";

const compact = (value: JsonValue | undefined) => (value === undefined ? "—" : JSON.stringify(value));
const record = (value: JsonValue | undefined): Record<string, JsonValue> =>
	value && typeof value === "object" && !Array.isArray(value) ? value : {};

/** A readable view of a retained run; the exact snapshot stays one disclosure away. */
export function RunSummary(props: {
	inspection: Inspection;
	definition: NeutralDefinition;
	labels: Record<string, string>;
}) {
	const snapshot = () => props.inspection.snapshot;
	const status = () => record(snapshot().status);
	const statusDetail = () => {
		const { type: _type, ...rest } = status();
		return Object.keys(rest).length ? compact(rest) : "";
	};
	const node = () => {
		const id = snapshot().node;
		if (typeof id !== "string") return compact(id);
		return props.definition.control.nodes[id] ? nodeTitle(id, props.definition, props.labels) : id;
	};
	const outputs = () => record(record(snapshot().outputs).values);
	/** Effects arrive keyed by digest; count them by their observed outcome. */
	const effects = () => {
		const raw = snapshot().effects;
		const list = Array.isArray(raw) ? raw : Object.values(record(raw));
		const counts = new Map<string, number>();
		for (const effect of list) {
			const outcome = record(record(record(effect).observation).outcome).type;
			const key = typeof outcome === "string" ? outcome : "pending";
			counts.set(key, (counts.get(key) ?? 0) + 1);
		}
		return list.length
			? `${list.length} recorded · ${[...counts].map(([key, n]) => `${n} ${key}`).join(", ")}`
			: "none";
	};
	return (
		<dl class={s.runSummary} aria-label="Run summary">
			<dt>Status</dt>
			<dd>
				{props.inspection.progress?.waiting_on_approval ? "Waiting for approval" : String(status().type ?? "unknown")}
				<Show when={statusDetail()}> · {statusDetail()}</Show>
			</dd>
			<dt>Current node</dt>
			<dd>{node()}</dd>
			<For each={Object.entries(outputs())}>
				{([name, value]) => (
					<>
						<dt>Return {name}</dt>
						<dd>{compact(value)}</dd>
					</>
				)}
			</For>
			<dt>Effects</dt>
			<dd>{effects()}</dd>
			<dt>Events</dt>
			<dd>{props.inspection.events.length} on this page</dd>
		</dl>
	);
}
