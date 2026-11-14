import { createSignal, For, Show } from "solid-js";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Textarea } from "../ui/Textarea";
import type { JsonValue, ObjectSchema, ValueType } from "./catalogTypes";
import { SchemaFields } from "./SchemaFields";

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
}) {
	const update = (name: string, binding?: DataSource) => {
		const { [name]: _removed, ...rest } = props.bindings;
		props.onChange(binding ? { ...rest, [name]: binding } : rest);
	};
	return (
		<section aria-label="Typed input mappings">
			<h5>Input mappings</h5>
			<p>Types and required inputs are checked by the backend before publication.</p>
			<For each={Object.entries(props.schema.fields)}>
				{([name, schema]) => {
					const binding = () => props.bindings[name];
					return (
						<fieldset>
							<legend>
								{name} · {schema.value_type}
							</legend>
							<Select
								aria-label={`${name} source`}
								value={binding()?.source ?? ""}
								onChange={(event) => {
									switch (event.currentTarget.value) {
										case "workflow_input":
											update(name, { source: "workflow_input", field: "" });
											break;
										case "activity_output":
											update(name, { source: "activity_output", activity: 0, field: "" });
											break;
										case "scope_output":
											update(name, { source: "scope_output", scope: "", field: "" });
											break;
										case "constant":
											update(name, { source: "constant", value: null });
											break;
										case "projection":
											update(name, {
												source: "projection",
												from: { source: "activity_output", activity: 0, field: "" },
												pointer: "/",
												value_type: schema.value_type,
											});
											break;
										case "object":
											update(name, { source: "object", fields: {} });
											break;
										default:
											update(name);
									}
								}}
							>
								<option value="">Unbound / default</option>
								<option value="workflow_input">Workflow input</option>
								<option value="activity_output">Activity output</option>
								<option value="scope_output">Scope output</option>
								<option value="constant">Constant</option>
								<option value="projection">Nested value (JSON pointer)</option>
								<option value="object">Object from sources</option>
							</Select>
							<Show when={binding()}>
								{(source) => (
									<>
										<Show when={"field" in source()}>
											<label>
												Source field
												<Input
													aria-label={`${name} source field`}
													value={"field" in source() ? (source() as { field: string }).field : ""}
													onInput={(event) => {
														const current = binding();
														if (current && "field" in current)
															update(name, { ...current, field: event.currentTarget.value });
													}}
												/>
											</label>
										</Show>
										<Show when={source().source === "activity_output"}>
											<label>
												Activity ordinal
												<Input
													aria-label={`${name} activity ordinal`}
													type="number"
													min="0"
													value={"activity" in source() ? (source() as { activity: number }).activity : 0}
													onInput={(event) => {
														const current = binding();
														if (current?.source === "activity_output")
															update(name, { ...current, activity: Number(event.currentTarget.value) });
													}}
												/>
											</label>
										</Show>
										<Show when={source().source === "scope_output"}>
											<label>
												Scope
												<Input
													aria-label={`${name} scope`}
													value={"scope" in source() ? (source() as { scope: string }).scope : ""}
													onInput={(event) => {
														const current = binding();
														if (current?.source === "scope_output")
															update(name, { ...current, scope: event.currentTarget.value });
													}}
												/>
											</label>
										</Show>
										<Show when={source().source === "projection" || source().source === "object"}>
											<CompositeSource name={name} source={source()} onChange={(next) => update(name, next)} />
										</Show>
										<Show when={source().source === "constant"}>
											<SchemaFields
												schema={{ fields: { [name]: schema } }}
												values={{ [name]: "value" in source() ? (source() as { value: JsonValue }).value : null }}
												onChange={(values) => {
													if (Object.hasOwn(values, name)) update(name, { source: "constant", value: values[name] });
													else update(name);
												}}
											/>
										</Show>
									</>
								)}
							</Show>
						</fieldset>
					);
				}}
			</For>
		</section>
	);
}

/** Composite sources are edited as JSON; the backend validates the shape and types. */
function CompositeSource(props: { name: string; source: DataSource; onChange: (source: DataSource) => void }) {
	const [error, setError] = createSignal("");
	return (
		<>
			<Textarea
				aria-label={`${props.name} composite source`}
				value={JSON.stringify(props.source, null, 2)}
				onChange={(event) => {
					try {
						const next: DataSource = JSON.parse(event.currentTarget.value);
						if (next?.source !== props.source.source) throw new Error("source kind changed");
						props.onChange(next);
						setError("");
					} catch {
						setError(`${props.name}: enter valid JSON for a ${props.source.source} source`);
					}
				}}
			/>
			<Show when={error()}>
				<span role="alert">{error()}</span>
			</Show>
		</>
	);
}
