import { For } from "solid-js";
import { BindingSourceEditor } from "./BindingSourceEditor";
import type { JsonValue, ObjectSchema, ValueType } from "./catalogTypes";
import s from "./WorkflowDesigner.module.css";

/** Mirrors workflows/data_binding.rs; absent binding differs from explicit null. */
export type DataSource =
	| { source: "workflow_input"; field: string }
	| { source: "activity_output"; activity: number; field: string }
	| { source: "scope_output"; scope: string; field: string }
	| { source: "constant"; value: JsonValue }
	/** A JSON pointer inside one input/output field, with its declared type. */
	| { source: "projection"; from: DataSource; pointer: string; value_type: ValueType }
	/** A typed object built from named member sources. */
	| { source: "object"; fields: Record<string, DataSource> };
export type InputBindings = Record<string, DataSource>;

export function MappingFields(props: {
	schema: ObjectSchema;
	bindings: InputBindings;
	onChange: (bindings: InputBindings) => void;
	/** Defaults to activity input mappings; workflow returns pass "Return mappings". */
	title?: string;
}) {
	const title = () => props.title ?? "Input mappings";
	return (
		<section aria-label={`Typed ${title().toLowerCase()}`} class={s.mappings}>
			<h5>{title()}</h5>
			<p>Types and required fields are checked by the backend before publication.</p>
			{/* Key rows by field name: entry tuples are new on every definition edit and would remount the row. */}
			<For each={Object.keys(props.schema.fields)}>
				{(name) => (
					<fieldset>
						<legend>
							{name} · {props.schema.fields[name]?.value_type}
						</legend>
						<BindingSourceEditor
							name={name}
							schema={props.schema.fields[name]}
							source={props.bindings[name]}
							onChange={(source) => {
								const { [name]: _removed, ...rest } = props.bindings;
								props.onChange(source ? { ...rest, [name]: source } : rest);
							}}
						/>
					</fieldset>
				)}
			</For>
		</section>
	);
}
