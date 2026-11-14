import { For, Show } from "solid-js";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import type { ActivityDescriptor, ActivityInstance } from "./catalogTypes";
import { SchemaFields } from "./SchemaFields";

/** No operation-name branches: custom scripts use the same descriptor form. */
export function CatalogInspector(props: {
	descriptor: ActivityDescriptor;
	instance: ActivityInstance;
	label: string;
	onLabel: (label: string) => void;
	onChange: (instance: ActivityInstance) => void;
}) {
	return (
		<section aria-label="Catalog activity inspector">
			<h4>{props.descriptor.id.name}</h4>
			<p>{props.descriptor.purpose}</p>
			<p>
				{props.descriptor.id.namespace} · version {props.descriptor.id.version}
			</p>
			<label>
				Label
				<Input
					aria-label="Activity label"
					value={props.label}
					onInput={(event) => props.onLabel(event.currentTarget.value)}
				/>
			</label>
			<details>
				<summary>Schema and handler contract</summary>
				<pre>{JSON.stringify(props.descriptor, null, 2)}</pre>
			</details>
			<h5>Configuration</h5>
			<SchemaFields
				schema={props.descriptor.config}
				values={props.instance.config}
				onChange={(config) => props.onChange({ ...props.instance, config })}
			/>
			<label>
				Execution policy
				<Select
					aria-label="Execution policy"
					value={props.instance.execution_policy}
					onChange={(event) => props.onChange({ ...props.instance, execution_policy: event.currentTarget.value })}
				>
					<option value="">Choose policy</option>
					<For each={props.descriptor.execution_policies}>{(policy) => <option value={policy}>{policy}</option>}</For>
				</Select>
			</label>
			<h5>Required capabilities</h5>
			<p>{props.descriptor.required_capabilities.join(", ") || "None"}</p>
			<h5>Named outcomes</h5>
			<p>{props.descriptor.outcomes.join(", ")}</p>
			<Show when={props.descriptor.subject}>
				{(subject) => (
					<details>
						<summary>Operation subject constraints</summary>
						<pre>{JSON.stringify(subject(), null, 2)}</pre>
					</details>
				)}
			</Show>
			<For
				each={[
					{ title: "Inputs", schema: () => props.descriptor.inputs },
					{ title: "Outputs", schema: () => props.descriptor.outputs },
				]}
			>
				{(port) => (
					<details open>
						<summary>{port.title}</summary>
						<dl>
							<For each={Object.entries(port.schema().fields)}>
								{([name, field]) => (
									<>
										<dt>{name}</dt>
										<dd>
											{field.value_type} · {field.required ? "required" : "optional"}
										</dd>
									</>
								)}
							</For>
						</dl>
					</details>
				)}
			</For>
		</section>
	);
}
