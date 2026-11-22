import { createSignal, For, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Textarea } from "../ui/Textarea";
import { activityOptions, activityOutputFields, useBindingChoices } from "./bindingChoices";
import { type FieldSchema, type ValueType, VALUE_TYPES as valueTypes } from "./catalogTypes";
import type { DataSource } from "./MappingFields";
import { SchemaFields } from "./SchemaFields";

const kinds = ["workflow_input", "activity_output", "scope_output", "constant", "projection", "object"] as const;
const labels = [
	"Workflow input",
	"Activity output",
	"Scope output",
	"Constant",
	"Nested value (JSON pointer)",
	"Object from sources",
];
const unresolved = (): DataSource => ({ source: "workflow_input", field: "" });

/** Only edit understood shapes. Unknown imports remain visible without normalization. */
function editable(source: unknown): source is DataSource {
	if (!source || typeof source !== "object" || Array.isArray(source)) return false;
	const s = source as Record<string, unknown>;
	const keys: Record<string, string[]> = {
		workflow_input: ["source", "field"],
		activity_output: ["source", "activity", "field"],
		scope_output: ["source", "scope", "field"],
		constant: ["source", "value"],
		projection: ["source", "from", "pointer", "value_type"],
		object: ["source", "fields"],
	};
	const expected = typeof s.source === "string" ? keys[s.source] : undefined;
	if (!expected || Object.keys(s).length !== expected.length || !expected.every((key) => Object.hasOwn(s, key)))
		return false;
	switch (s.source) {
		case "workflow_input":
			return typeof s.field === "string";
		case "activity_output":
			return typeof s.field === "string" && Number.isSafeInteger(s.activity) && Number(s.activity) >= 0;
		case "scope_output":
			return typeof s.field === "string" && typeof s.scope === "string";
		case "constant":
			return true;
		case "projection":
			return typeof s.pointer === "string" && valueTypes.includes(s.value_type as ValueType);
		case "object":
			return s.fields !== null && typeof s.fields === "object" && !Array.isArray(s.fields);
		default:
			return false;
	}
}

export function BindingSourceEditor(props: {
	name: string;
	schema: FieldSchema;
	source: DataSource | undefined;
	onChange: (source?: DataSource) => void;
	baseOnly?: boolean;
	member?: boolean;
}) {
	const supported = () =>
		props.source === undefined ||
		(editable(props.source) &&
			(!props.baseOnly || ["workflow_input", "activity_output", "scope_output"].includes(props.source.source)));
	const source = () => props.source;
	const choices = useBindingChoices();
	/** Known field names for the current source, or undefined to keep free text. */
	const fieldOptions = () => {
		const current = source();
		if (!choices || !current) return undefined;
		if (current.source === "workflow_input") return Object.keys(choices.definition().inputs.fields);
		if (current.source === "activity_output") return activityOutputFields(choices, current.activity);
		return undefined;
	};
	const choose = (kind: string) => {
		switch (kind) {
			case "workflow_input":
				props.onChange(unresolved());
				break;
			case "activity_output":
				props.onChange({ source: "activity_output", activity: 0, field: "" });
				break;
			case "scope_output":
				props.onChange({ source: "scope_output", scope: "", field: "" });
				break;
			case "constant":
				props.onChange({ source: "constant", value: null });
				break;
			case "object":
				props.onChange({ source: "object", fields: {} });
				break;
			case "projection":
				props.onChange({
					source: "projection",
					from: { source: "activity_output", activity: 0, field: "" },
					pointer: "/",
					value_type: props.schema.value_type,
				});
				break;
			default:
				props.onChange(props.member ? unresolved() : undefined);
		}
	};
	const patch = (changes: Record<string, string | number>) => {
		const current = source();
		if (current) props.onChange({ ...current, ...changes } as DataSource);
	};
	const literalType = (): ValueType => {
		const current = source();
		if (!props.member || current?.source !== "constant") return props.schema.value_type;
		const value = current.value;
		return value === null
			? "null"
			: Array.isArray(value)
				? "array"
				: typeof value === "number"
					? Number.isInteger(value)
						? "integer"
						: "number"
					: (typeof value as ValueType);
	};
	return (
		<Show
			when={supported()}
			fallback={
				<>
					<p role="alert">{props.name}: unsupported or malformed source; original value retained.</p>
					<pre aria-label={`${props.name} imported source`}>{JSON.stringify(source(), null, 2)}</pre>
				</>
			}
		>
			<Select
				aria-label={`${props.name} source`}
				value={source()?.source ?? ""}
				onChange={(event) => choose(event.currentTarget.value)}
			>
				<Show when={!props.baseOnly}>
					<option value="">{props.member ? "Unresolved" : "Unbound / default"}</option>
				</Show>
				<For each={kinds.filter((_, index) => !props.baseOnly || index < 3)}>
					{(kind) => <option value={kind}>{labels[kinds.indexOf(kind)]}</option>}
				</For>
			</Select>
			<Show when={source()?.source === "activity_output"}>
				<label>
					Activity
					<Show
						when={choices}
						fallback={
							<Input
								aria-label={`${props.name} activity ordinal`}
								type="number"
								min="0"
								value={(source() as { activity: number }).activity}
								onInput={(event) => patch({ activity: Number(event.currentTarget.value) })}
							/>
						}
					>
						{(available) => (
							<Select
								aria-label={`${props.name} activity`}
								value={String((source() as { activity: number }).activity)}
								onChange={(event) => patch({ activity: Number(event.currentTarget.value) })}
							>
								<For each={activityOptions(available())}>
									{(item) => (
										// Mark the option itself: a Select's value applied before its options exist falls back to the first.
										<option
											value={String(item.ordinal)}
											selected={item.ordinal === (source() as { activity: number }).activity}
										>
											{item.title}
										</option>
									)}
								</For>
							</Select>
						)}
					</Show>
				</label>
			</Show>
			<Show
				when={
					source()?.source === "workflow_input" ||
					source()?.source === "activity_output" ||
					source()?.source === "scope_output"
				}
			>
				<label>
					Source field
					<FieldChoice
						label={`${props.name} source field`}
						value={(source() as { field: string }).field}
						options={fieldOptions()}
						onChange={(field) => patch({ field })}
					/>
				</label>
			</Show>
			<Show when={source()?.source === "scope_output"}>
				<label>
					Scope
					<Input
						aria-label={`${props.name} scope`}
						value={(source() as { scope: string }).scope}
						onInput={(event) => patch({ scope: event.currentTarget.value })}
					/>
				</label>
			</Show>
			<Show when={source()?.source === "constant"}>
				<Show when={props.member}>
					<label>
						Constant type
						<Select
							aria-label={`${props.name} constant type`}
							value={literalType()}
							onChange={(event) => {
								const type = event.currentTarget.value;
								props.onChange({
									source: "constant",
									value:
										type === "string"
											? ""
											: type === "boolean"
												? false
												: type === "object"
													? {}
													: type === "array"
														? []
														: type === "number" || type === "integer"
															? 0
															: null,
								});
							}}
						>
							<For each={valueTypes.filter((type) => !type.endsWith("_reference"))}>
								{(type) => <option value={type}>{type}</option>}
							</For>
						</Select>
					</label>
				</Show>
				<SchemaFields
					schema={{ fields: { [props.name]: { ...props.schema, value_type: literalType() } } }}
					values={{ [props.name]: (source() as Extract<DataSource, { source: "constant" }>).value }}
					onChange={(values) =>
						props.onChange(
							Object.hasOwn(values, props.name)
								? { source: "constant", value: values[props.name] }
								: props.member
									? unresolved()
									: undefined,
						)
					}
				/>
			</Show>
			<Show when={source()?.source === "object"}>
				<ObjectMembers
					name={props.name}
					source={source() as Extract<DataSource, { source: "object" }>}
					onChange={props.onChange}
				/>
			</Show>
			<Show when={source()?.source === "projection"}>
				<BindingSourceEditor
					name={`${props.name} base`}
					schema={props.schema}
					baseOnly
					source={(source() as Extract<DataSource, { source: "projection" }>).from}
					onChange={(from) => {
						const current = source();
						if (current?.source === "projection" && from) props.onChange({ ...current, from });
					}}
				/>
				<label>
					JSON pointer
					<Input
						aria-label={`${props.name} JSON pointer`}
						value={(source() as Extract<DataSource, { source: "projection" }>).pointer}
						onInput={(event) => patch({ pointer: event.currentTarget.value })}
					/>
				</label>
				<label>
					Result type
					<Select
						aria-label={`${props.name} result type`}
						value={(source() as Extract<DataSource, { source: "projection" }>).value_type}
						onChange={(event) => patch({ value_type: event.currentTarget.value })}
					>
						<For each={valueTypes}>{(type) => <option value={type}>{type}</option>}</For>
					</Select>
				</label>
			</Show>
			<Show when={source()?.source === "object" || source()?.source === "projection"}>
				<AdvancedSource name={props.name} source={source()!} onChange={props.onChange} />
			</Show>
		</Show>
	);
}
/** A select over the known names; a name not in the list stays selectable so imports are never rewritten. */
function FieldChoice(props: {
	label: string;
	value: string;
	options: string[] | undefined;
	onChange: (value: string) => void;
}) {
	return (
		<Show
			when={props.options?.length ? props.options : undefined}
			fallback={
				<Input
					aria-label={props.label}
					value={props.value}
					onInput={(event) => props.onChange(event.currentTarget.value)}
				/>
			}
		>
			{(options) => (
				<Select
					aria-label={props.label}
					value={props.value}
					onChange={(event) => props.onChange(event.currentTarget.value)}
				>
					<option value="">Choose a field</option>
					<For each={props.value && !options().includes(props.value) ? [...options(), props.value] : options()}>
						{(name) => (
							<option value={name} selected={name === props.value}>
								{name}
							</option>
						)}
					</For>
				</Select>
			)}
		</Show>
	);
}

/** Optional wire inspection preserves the existing flat editor's advanced control. */
function AdvancedSource(props: { name: string; source: DataSource; onChange: (source: DataSource) => void }) {
	const [error, setError] = createSignal("");
	return (
		<details>
			<summary>Advanced source JSON</summary>
			<Textarea
				aria-label={`${props.name} composite source`}
				value={JSON.stringify(props.source, null, 2)}
				onChange={(event) => {
					try {
						const next: unknown = JSON.parse(event.currentTarget.value);
						if (!editable(next) || next.source !== props.source.source) throw new Error("source kind changed");
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
		</details>
	);
}

const memberSchema: FieldSchema = { value_type: "string", required: true, default: null };

function ObjectMembers(props: {
	name: string;
	source: Extract<DataSource, { source: "object" }>;
	onChange: (source: DataSource) => void;
}) {
	const [newName, setNewName] = createSignal("");
	const [error, setError] = createSignal("");
	const write = (fields: Record<string, DataSource>) => props.onChange({ ...props.source, fields });
	return (
		<fieldset>
			<legend>{props.name} object members</legend>
			<For each={Object.keys(props.source.fields)}>
				{(key) => {
					const [draft, setDraft] = createSignal(key);
					const name = () => `${props.name} member ${JSON.stringify(key)}`;
					return (
						<fieldset>
							<legend>{key}</legend>
							<Input
								aria-label={`${name()} name`}
								value={draft()}
								onInput={(event) => setDraft(event.currentTarget.value)}
							/>
							<Button
								aria-label={`Rename ${name()}`}
								onClick={() => {
									if (draft() === key) return;
									if (Object.hasOwn(props.source.fields, draft())) {
										setError("A member with that name already exists.");
										return;
									}
									const { [key]: value, ...rest } = props.source.fields;
									write({ ...rest, [draft()]: value });
									setError("");
								}}
							>
								Rename
							</Button>
							<Button
								aria-label={`Remove ${name()}`}
								onClick={() => {
									const { [key]: _removed, ...rest } = props.source.fields;
									write(rest);
								}}
							>
								Remove
							</Button>
							<BindingSourceEditor
								name={name()}
								schema={memberSchema}
								member
								source={props.source.fields[key]}
								onChange={(source) => write({ ...props.source.fields, [key]: source ?? unresolved() })}
							/>
						</fieldset>
					);
				}}
			</For>
			<Input
				aria-label={`${props.name} new member name`}
				value={newName()}
				onInput={(event) => setNewName(event.currentTarget.value)}
			/>
			<Button
				aria-label={`Add ${props.name} member`}
				onClick={() => {
					if (Object.hasOwn(props.source.fields, newName())) {
						setError("A member with that name already exists.");
						return;
					}
					write({ ...props.source.fields, [newName()]: unresolved() });
					setNewName("");
					setError("");
				}}
			>
				Add member
			</Button>
			<Show when={error()}>
				<span role="alert">{error()}</span>
			</Show>
		</fieldset>
	);
}
