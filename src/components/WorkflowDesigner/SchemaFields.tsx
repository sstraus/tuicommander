import { createSignal, For, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Textarea } from "../ui/Textarea";
import type { FieldSchema, JsonValue, ObjectSchema } from "./catalogTypes";

/** Edits literals only. Backend resolves defaults and validates the declared type. */
export function SchemaFields(props: {
	schema: ObjectSchema;
	values: Record<string, JsonValue>;
	onChange: (values: Record<string, JsonValue>) => void;
}) {
	const set = (name: string, value: JsonValue) => props.onChange({ ...props.values, [name]: value });
	const unset = (name: string) => {
		const { [name]: _removed, ...rest } = props.values;
		props.onChange(rest);
	};
	return (
		<For each={Object.entries(props.schema.fields)}>
			{([name, schema]) => {
				const value = () => (Object.hasOwn(props.values, name) ? props.values[name] : schema.default);
				return (
					<div>
						<label>
							{name} · {schema.value_type}
							{schema.required ? " · required" : " · optional"}
							<Show
								when={schema.value_type === "string"}
								fallback={
									<Show
										when={schema.value_type === "boolean"}
										fallback={
											<JsonField name={name} schema={schema} value={value()} onChange={(next) => set(name, next)} />
										}
									>
										<Select
											aria-label={name}
											value={value() === null ? "" : String(value())}
											onChange={(event) => {
												if (event.currentTarget.value === "") unset(name);
												else set(name, event.currentTarget.value === "true");
											}}
										>
											<option value="">Unset</option>
											<option value="true">True</option>
											<option value="false">False</option>
										</Select>
									</Show>
								}
							>
								<Input
									aria-label={name}
									value={typeof value() === "string" ? String(value()) : ""}
									onInput={(event) => set(name, event.currentTarget.value)}
								/>
							</Show>
						</label>
						<Button type="button" onClick={() => unset(name)} disabled={!Object.hasOwn(props.values, name)}>
							Use default / unset {name}
						</Button>
					</div>
				);
			}}
		</For>
	);
}

function JsonField(props: {
	name: string;
	schema: FieldSchema;
	value: JsonValue;
	onChange: (value: JsonValue) => void;
}) {
	const [error, setError] = createSignal("");
	return (
		<>
			<Textarea
				aria-label={props.name}
				value={JSON.stringify(props.value, null, 2)}
				onChange={(event) => {
					try {
						const value: JsonValue = JSON.parse(event.currentTarget.value);
						props.onChange(value);
						setError("");
					} catch {
						setError(`${props.name}: enter valid JSON for ${props.schema.value_type}`);
					}
				}}
			/>
			<Show when={error()}>
				<span role="alert">{error()}</span>
			</Show>
		</>
	);
}
