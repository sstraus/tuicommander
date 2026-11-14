import { createEffect, For } from "solid-js";
import { Select } from "../ui/Select";
import type { ObjectSchema } from "./catalogTypes";

/** Field selection only. Compatibility and requiredness are authoritative in Rust. */
export function TriggerMappings(props: {
	title: string;
	source: ObjectSchema;
	target: ObjectSchema;
	mapping: Record<string, string>;
	onChange: (mapping: Record<string, string>) => void;
}) {
	return (
		<fieldset>
			<legend>{props.title}</legend>
			<For each={Object.keys(props.target.fields)}>
				{(name) => {
					let select: HTMLSelectElement | undefined;
					// Reconcile the native value after dynamic options have mounted.
					createEffect(() => {
						void props.source.fields;
						const value = props.mapping[name] ?? "";
						if (select) select.value = value;
					});
					return (
						<label>
							{name} · {props.target.fields[name].value_type}
							{props.target.fields[name].required ? " (required)" : ""}
							<Select
								ref={select}
								aria-label={`${props.title}: ${name}`}
								onChange={(event) => {
									const { [name]: _removed, ...rest } = props.mapping;
									props.onChange(event.currentTarget.value ? { ...rest, [name]: event.currentTarget.value } : rest);
								}}
							>
								<option value="">Unmapped / default</option>
								<For each={Object.keys(props.source.fields)}>
									{(source) => (
										<option value={source}>
											{source} · {props.source.fields[source].value_type}
										</option>
									)}
								</For>
							</Select>
						</label>
					);
				}}
			</For>
		</fieldset>
	);
}
