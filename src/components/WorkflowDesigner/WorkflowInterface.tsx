import { createSignal, For, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { type ObjectSchema, type ValueType, VALUE_TYPES as valueTypes } from "./catalogTypes";
import { type InputBindings, MappingFields } from "./MappingFields";
import type { NeutralDefinition } from "./neutralClient";
import s from "./WorkflowDesigner.module.css";

/** Edit the backend's shallow field schema without inventing nested schema semantics. */
export function WorkflowSchemaEditor(props: {
	title: string;
	schema: ObjectSchema;
	/** `renamed` reports a rename (`to`) or a removal (`to: null`) so dependent mappings can follow. */
	onChange: (schema: ObjectSchema, renamed?: { from: string; to: string | null }) => void;
}) {
	const [name, setName] = createSignal("");
	const [kind, setKind] = createSignal<ValueType>("string");
	const [error, setError] = createSignal("");
	function add() {
		const key = name().trim();
		if (!key || Object.hasOwn(props.schema.fields, key)) {
			setError("Use a nonempty, unique field name.");
			return;
		}
		props.onChange({
			fields: { ...props.schema.fields, [key]: { value_type: kind(), required: true, default: null } },
		});
		setName("");
		setError("");
	}
	return (
		<section aria-label={props.title}>
			<h4>{props.title}</h4>
			<For each={Object.keys(props.schema.fields)}>
				{(key) => {
					const field = () => props.schema.fields[key];
					return (
						<div class={s.interfaceField}>
							<FieldName
								title={props.title}
								name={key}
								taken={(next) => Object.hasOwn(props.schema.fields, next)}
								onRename={(next) => {
									// Rebuild in order so the renamed field keeps its position.
									const fields = Object.fromEntries(
										Object.entries(props.schema.fields).map(([name, value]) => [name === key ? next : name, value]),
									);
									props.onChange({ fields }, { from: key, to: next });
								}}
							/>
							<label>
								Type
								<Select
									aria-label={`${props.title} ${key} type`}
									value={field()?.value_type}
									onChange={(event) =>
										props.onChange({
											fields: {
												...props.schema.fields,
												[key]: { ...field(), value_type: event.currentTarget.value as ValueType },
											},
										})
									}
								>
									<For each={valueTypes}>{(type) => <option value={type}>{type}</option>}</For>
								</Select>
							</label>
							<label>
								Presence
								<Select
									aria-label={`${props.title} ${key} presence`}
									value={field()?.required ? "required" : "optional"}
									onChange={(event) =>
										props.onChange({
											fields: {
												...props.schema.fields,
												[key]: { ...field(), required: event.currentTarget.value === "required" },
											},
										})
									}
								>
									<option value="required">Required</option>
									<option value="optional">Optional</option>
								</Select>
							</label>
							<Button
								aria-label={`Remove ${props.title.toLowerCase()} field ${key}`}
								onClick={() => {
									const { [key]: _removed, ...fields } = props.schema.fields;
									props.onChange({ fields }, { from: key, to: null });
								}}
							>
								Remove
							</Button>
							<Show when={field() && field().default !== null}>
								<small>
									Default: {JSON.stringify(field()?.default ?? null)} · retained; edit in Advanced definition.
								</small>
							</Show>
						</div>
					);
				}}
			</For>
			<div class={s.interfaceAdd}>
				<label>
					New field name
					<Input
						aria-label={`${props.title} new field name`}
						value={name()}
						onInput={(event) => setName(event.currentTarget.value)}
					/>
				</label>
				<label>
					Type
					<Select
						aria-label={`${props.title} new field type`}
						value={kind()}
						onChange={(event) => setKind(event.currentTarget.value as ValueType)}
					>
						<For each={valueTypes}>{(type) => <option value={type}>{type}</option>}</For>
					</Select>
				</label>
				<Button onClick={add}>Add {props.title.toLowerCase()} field</Button>
			</div>
			<Show when={error()}>
				<p role="alert">{error()}</p>
			</Show>
		</section>
	);
}

/** Rename commits on blur or Enter, so a half-typed name never moves mappings. */
function FieldName(props: {
	title: string;
	name: string;
	taken: (name: string) => boolean;
	onRename: (name: string) => void;
}) {
	const [error, setError] = createSignal("");
	const commit = (input: HTMLInputElement) => {
		const next = input.value.trim();
		if (next === props.name) return setError("");
		if (!next || props.taken(next)) {
			setError("Use a nonempty, unique field name.");
			input.value = props.name;
			return;
		}
		setError("");
		props.onRename(next);
	};
	return (
		<label>
			Name
			<Input
				aria-label={`${props.title} field ${props.name} name`}
				value={props.name}
				onBlur={(event) => commit(event.currentTarget)}
				onKeyDown={(event) => {
					if (event.key === "Enter") commit(event.currentTarget);
				}}
			/>
			<Show when={error()}>
				<span role="alert">{error()}</span>
			</Show>
		</label>
	);
}

/** Follow a return field rename or removal in the return mappings. */
function renameBinding(bindings: InputBindings, change?: { from: string; to: string | null }): InputBindings {
	if (!change || !Object.hasOwn(bindings, change.from)) return bindings;
	const { [change.from]: moved, ...rest } = bindings;
	return change.to === null ? rest : { ...rest, [change.to]: moved };
}

export function WorkflowInterface(props: {
	definition: NeutralDefinition;
	onChange: (definition: NeutralDefinition) => void;
}) {
	return (
		<section class={s.interfaceEditor} aria-label="Workflow interface">
			<p>
				Declare the workflow's typed inputs and returns. Backend validation checks compatibility; a reference is data,
				not execution authority.
			</p>
			<WorkflowSchemaEditor
				title="Inputs"
				schema={props.definition.inputs}
				onChange={(inputs) => props.onChange({ ...props.definition, inputs })}
			/>
			<WorkflowSchemaEditor
				title="Returns"
				schema={props.definition.outputs}
				onChange={(outputs, change) =>
					props.onChange({
						...props.definition,
						outputs,
						output_bindings: renameBinding(props.definition.output_bindings, change),
					})
				}
			/>
			<MappingFields
				title="Return mappings"
				schema={props.definition.outputs}
				bindings={props.definition.output_bindings}
				onChange={(output_bindings) => props.onChange({ ...props.definition, output_bindings })}
			/>
			<p>
				Return mappings describe values, not proof that their producer runs on every control path. Validate and preview
				the intended paths before publication.
			</p>
			<h4>Subject</h4>
			<label>
				Subject requirement
				<Select
					value={props.definition.subject ? (props.definition.subject.required ? "required" : "optional") : "none"}
					onChange={(event) =>
						props.onChange({
							...props.definition,
							subject:
								event.currentTarget.value === "none"
									? null
									: {
											required: event.currentTarget.value === "required",
											fields: props.definition.subject?.fields ?? { fields: {} },
										},
						})
					}
				>
					<option value="none">No subject</option>
					<option value="optional">Optional subject</option>
					<option value="required">Required subject</option>
				</Select>
			</label>
			<Show when={props.definition.subject}>
				{(subject) => (
					<WorkflowSchemaEditor
						title="Subject fields"
						schema={subject().fields}
						onChange={(fields) => props.onChange({ ...props.definition, subject: { ...subject(), fields } })}
					/>
				)}
			</Show>
		</section>
	);
}
