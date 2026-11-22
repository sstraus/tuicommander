import { createEffect, createSignal, For, on, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import {
	type ActivityDescriptor,
	type JsonValue,
	type ObjectSchema,
	type ValueType,
	VALUE_TYPES as workflowValueTypes,
} from "./catalogTypes";
import { type DataSource, type InputBindings, MappingFields } from "./MappingFields";
import type { NeutralDefinition } from "./neutralClient";
import { WorkflowSchemaEditor } from "./WorkflowInterface";

type RecordValue = Record<string, JsonValue>;
function record(value: JsonValue | undefined): RecordValue {
	return value !== null && typeof value === "object" && !Array.isArray(value) ? value : {};
}
function schema(value: JsonValue | undefined): ObjectSchema {
	const fields = record(value).fields;
	return fields !== null && typeof fields === "object" && !Array.isArray(fields)
		? (value as unknown as ObjectSchema)
		: { fields: {} };
}
function uniqueKey(table: Record<string, JsonValue>, prefix: string) {
	let index = 1;
	while (Object.hasOwn(table, `${prefix}_${index}`)) index++;
	return `${prefix}_${index}`;
}
const field = (value_type: ValueType) => ({ value_type, required: true, default: null });

/** Edits exact neutral scope declarations; never schedules or derives scope results. */
export function NeutralScopeInspector(props: {
	definition: NeutralDefinition;
	activities: ActivityDescriptor[];
	nodeId: string;
	documentKey?: number;
	onChange: (definition: NeutralDefinition) => void;
	onInspectBody: (ordinal: number) => void;
}) {
	const node = () => props.definition.control.nodes[props.nodeId];
	const scopeKind = () => {
		const n = node();
		return n?.type === "collection"
			? "collections"
			: n?.type === "loop"
				? "loops"
				: n?.type === "fork"
					? "forks"
					: undefined;
	};
	const scopeKey = () => {
		const n = node();
		return n && "scope" in n ? n.scope : "";
	};
	const contract = () => {
		const kind = scopeKind();
		return kind ? record(props.definition[kind][scopeKey()]) : {};
	};
	const hasScope = () => {
		const kind = scopeKind();
		return !!kind && Object.hasOwn(props.definition[kind], scopeKey());
	};
	function setContract(next: RecordValue, definition = props.definition) {
		const kind = scopeKind();
		if (!kind) return;
		const key = scopeKey() || uniqueKey(definition[kind], node()?.type ?? "scope");
		const n = definition.control.nodes[props.nodeId];
		if (!n || !("scope" in n)) return;
		props.onChange({
			...definition,
			[kind]: { ...definition[kind], [key]: next },
			control: { ...definition.control, nodes: { ...definition.control.nodes, [props.nodeId]: { ...n, scope: key } } },
		});
	}
	function patch(name: string, value: JsonValue | undefined) {
		const next = { ...contract() };
		if (value === undefined) delete next[name];
		else next[name] = value;
		setContract(next);
	}
	function createScope() {
		const kind = scopeKind();
		// Never default to an ordinal another owner holds: take an unowned one or a not-yet-existing next ordinal.
		const body = { body: unowned()[0] ?? props.definition.instances.length };
		if (kind === "forks") setContract({ branches: {}, max_concurrency: 1 });
		if (kind === "collections")
			setContract({
				collection: { source: "workflow_input", field: "" },
				item_type: "string",
				...body,
				item_input: "item",
				index_input: "index",
				max_items: 10,
				max_concurrency: 1,
				empty: "complete",
				failure: "blocked",
			});
		if (kind === "loops") setContract({ ...body, max_iterations: 3, carried: { fields: {} }, initial: {}, result: {} });
	}
	const [branch, setBranch] = createSignal("");
	createEffect(
		on(
			() => [props.nodeId, props.documentKey],
			() => setBranch(""),
		),
	);
	const bodyName = (ordinal: number) =>
		`${props.definition.instances[ordinal]?.descriptor.name ?? "Missing activity"} · activity ${ordinal}`;
	/** Every activity ordinal already owned by an ordinary node or by any scope body/branch. */
	const owned = () => {
		const result = new Set<number>();
		for (const n of Object.values(props.definition.control.nodes)) if (n.type === "activity") result.add(n.activity);
		for (const c of Object.values(props.definition.collections)) result.add(Number(record(c).body));
		for (const l of Object.values(props.definition.loops)) result.add(Number(record(l).body));
		for (const f of Object.values(props.definition.forks))
			for (const b of Object.values(record(record(f).branches))) result.add(Number(record(b).activity));
		return result;
	};
	const unowned = () =>
		props.definition.instances.map((_, ordinal) => ordinal).filter((ordinal) => !owned().has(ordinal));
	function appendBody(index: number, name?: string) {
		const descriptor = props.activities[index];
		if (!descriptor) return;
		if (scopeKind() === "forks" && (!name?.trim() || Object.hasOwn(record(contract().branches), name.trim()))) return;
		const instance = { descriptor: descriptor.id, config: {}, execution_policy: "" };
		const current = Number(contract().body);
		// A single-body scope replaces its body in place, so the old instance never becomes unowned.
		const replace = scopeKind() !== "forks" && Number.isInteger(current) && !!props.definition.instances[current];
		const ordinal = replace ? current : props.definition.instances.length;
		const definition = {
			...props.definition,
			instances: replace
				? props.definition.instances.map((item, i) => (i === current ? instance : item))
				: [...props.definition.instances, instance],
		};
		// Loop results read the old body's outputs; the replacement body must be bound again explicitly.
		setContract(
			scopeKind() === "forks"
				? {
						...contract(),
						branches: { ...record(contract().branches), [name!.trim()]: { activity: ordinal, required: true } },
					}
				: { ...contract(), body: ordinal, ...(replace && scopeKind() === "loops" ? { result: {} } : {}) },
			definition,
		);
		setBranch("");
		props.onInspectBody(ordinal);
	}
	function bodyPicker(value: () => number, change: (ordinal: number) => void, label: string) {
		return (
			<label>
				{label}
				<Select
					aria-label={label}
					value={Number.isInteger(value()) ? String(value()) : ""}
					onChange={(event) => {
						if (event.currentTarget.value !== "") change(Number(event.currentTarget.value));
					}}
				>
					<option value="">Choose a scoped activity</option>
					<Show when={Number.isInteger(value()) && !unowned().includes(value())}>
						<option value={String(value())}>{bodyName(value())} · current body</option>
					</Show>
					<For each={unowned()}>{(ordinal) => <option value={String(ordinal)}>{bodyName(ordinal)}</option>}</For>
				</Select>
			</label>
		);
	}
	const bodyDescriptor = () => {
		const instance = props.definition.instances[Number(contract().body)];
		return props.activities.find(
			(item) =>
				item.id.namespace === instance?.descriptor.namespace &&
				item.id.name === instance?.descriptor.name &&
				item.id.version === instance?.descriptor.version,
		);
	};
	return (
		<>
			<Show when={node() && "condition" in node()}>
				<ConditionContractEditor
					definition={props.definition}
					nodeId={props.nodeId}
					documentKey={props.documentKey}
					onChange={props.onChange}
				/>
			</Show>
			<Show when={scopeKind()}>
				<section aria-label="Scope contract inspector">
					<h4>Bounded scope</h4>
					<Show
						when={hasScope()}
						fallback={
							<>
								<p>
									Create the scope contract, then choose its body and bounds. Incomplete contracts cannot be published.
								</p>
								<Button onClick={createScope}>Create scope contract</Button>
							</>
						}
					>
						<Show when={scopeKind() !== "forks"}>
							{bodyPicker(
								() => Number(contract().body),
								(ordinal) => patch("body", ordinal),
								"Scope body",
							)}
							<Button
								disabled={!props.definition.instances[Number(contract().body)]}
								onClick={() => props.onInspectBody(Number(contract().body))}
							>
								Configure scope body
							</Button>
							<label>
								New body from catalog
								<Select
									aria-label="New scope body"
									value=""
									onChange={(event) => {
										if (event.currentTarget.value !== "") appendBody(Number(event.currentTarget.value));
									}}
								>
									<option value="">Add a new scoped activity</option>
									<For each={props.activities}>
										{(item, index) => (
											<option value={String(index())}>
												{item.id.namespace}/{item.id.name} · v{item.id.version}
											</option>
										)}
									</For>
								</Select>
							</label>
						</Show>
						<Show when={scopeKind() === "collections"}>
							<MappingFields
								schema={{ fields: { collection: field("array") } }}
								bindings={contract().collection ? { collection: contract().collection as unknown as DataSource } : {}}
								onChange={(bindings) => patch("collection", bindings.collection as unknown as JsonValue | undefined)}
							/>
							<label>
								Item type
								<Select
									value={String(contract().item_type ?? "")}
									onChange={(event) => patch("item_type", event.currentTarget.value)}
								>
									<For each={workflowValueTypes}>{(type) => <option value={type}>{type}</option>}</For>
								</Select>
							</label>
							<label>
								Item input
								<Input
									value={String(contract().item_input ?? "")}
									onInput={(event) => patch("item_input", event.currentTarget.value)}
								/>
							</label>
							<label>
								Index input
								<Input
									value={String(contract().index_input ?? "")}
									onInput={(event) => patch("index_input", event.currentTarget.value)}
								/>
							</label>
							<label>
								Maximum items
								<Input
									type="number"
									min="1"
									value={Number(contract().max_items)}
									onInput={(event) => patch("max_items", Number(event.currentTarget.value))}
								/>
							</label>
							<label>
								Empty collection
								<Select
									value={String(contract().empty)}
									onChange={(event) => patch("empty", event.currentTarget.value)}
								>
									<option value="complete">Complete</option>
									<option value="blocked">Blocked</option>
								</Select>
							</label>
							<label>
								Item failure
								<Select
									value={String(contract().failure)}
									onChange={(event) => patch("failure", event.currentTarget.value)}
								>
									<option value="collect">Collect results</option>
									<option value="blocked">Block scope</option>
								</Select>
							</label>
							<p>
								Results retain ordered index/state/outcome/outputs/absent envelopes, including failures and skipped
								items. They are not flattened outputs.
							</p>
						</Show>
						<Show when={scopeKind() === "loops"}>
							<label>
								Maximum iterations
								<Input
									type="number"
									min="1"
									value={Number(contract().max_iterations)}
									onInput={(event) => patch("max_iterations", Number(event.currentTarget.value))}
								/>
							</label>
							<WorkflowSchemaEditor
								title="Carried state"
								schema={schema(contract().carried)}
								onChange={(value) => patch("carried", value as unknown as JsonValue)}
							/>
							<h5>Initial carried values</h5>
							<MappingFields
								schema={schema(contract().carried)}
								bindings={record(contract().initial) as unknown as InputBindings}
								onChange={(value) => patch("initial", value as unknown as JsonValue)}
							/>
							<h5>Current-body carried results</h5>
							<p>
								Each result must read an output of this iteration's body directly. Object wrappers, projections and
								other producers are not loop results.
							</p>
							<For each={Object.keys(schema(contract().carried).fields)}>
								{(name) => {
									const source = () => record(record(contract().result)[name]);
									return (
										<label>
											{name} result output
											<Select
												aria-label={`${name} loop result output`}
												value={
													source().source === "activity_output" && source().activity === contract().body
														? String(source().field ?? "")
														: ""
												}
												onChange={(event) => {
													const result = { ...record(contract().result) };
													if (event.currentTarget.value)
														result[name] = {
															source: "activity_output",
															activity: Number(contract().body),
															field: event.currentTarget.value,
														};
													else delete result[name];
													patch("result", result);
												}}
											>
												<option value="">Unbound / reselect a direct body output</option>
												<For each={Object.entries(bodyDescriptor()?.outputs.fields ?? {})}>
													{([key, value]) => (
														<option value={key}>
															{key} · {value.value_type}
														</option>
													)}
												</For>
											</Select>
											<Show
												when={
													Object.keys(source()).length > 0 &&
													(source().source !== "activity_output" || source().activity !== contract().body)
												}
											>
												<small>Existing source is not a direct current-body result; reselect before validation.</small>
											</Show>
										</label>
									);
								}}
							</For>
						</Show>
						<Show when={scopeKind() === "forks"}>
							<h5>Parallel branches · all join</h5>
							<p>The join waits for every declared branch. A failed required branch blocks the scope.</p>
							<For each={Object.entries(record(contract().branches))}>
								{([name, value]) => {
									const current = () => record(record(contract().branches)[name] ?? value);
									const update = (next: RecordValue) =>
										patch("branches", { ...record(contract().branches), [name]: next });
									return (
										<fieldset>
											<legend>{name}</legend>
											{bodyPicker(
												() => Number(current().activity),
												(activity) => update({ ...current(), activity }),
												`${name} branch body`,
											)}
											<label>
												Join requirement
												<Select
													value={current().required === false ? "optional" : "required"}
													onChange={(event) =>
														update({ ...current(), required: event.currentTarget.value === "required" })
													}
												>
													<option value="required">Required success</option>
													<option value="optional">Optional success</option>
												</Select>
											</label>
											<Button onClick={() => props.onInspectBody(Number(current().activity))}>
												Configure {name} body
											</Button>
										</fieldset>
									);
								}}
							</For>
							<label>
								New branch name
								<Input value={branch()} onInput={(event) => setBranch(event.currentTarget.value)} />
							</label>
							<label>
								New branch from catalog
								<Select
									aria-label="New fork branch body"
									disabled={!branch().trim() || Object.hasOwn(record(contract().branches), branch().trim())}
									value=""
									onChange={(event) => {
										if (event.currentTarget.value !== "") appendBody(Number(event.currentTarget.value), branch());
									}}
								>
									<option value="">Add a new branch activity</option>
									<For each={props.activities}>
										{(item, index) => (
											<option value={String(index())}>
												{item.id.namespace}/{item.id.name} · v{item.id.version}
											</option>
										)}
									</For>
								</Select>
							</label>
						</Show>
						<Show when={scopeKind() === "forks" || scopeKind() === "collections"}>
							<label>
								Maximum concurrency
								<Input
									type="number"
									min="1"
									value={Number(contract().max_concurrency)}
									onInput={(event) => patch("max_concurrency", Number(event.currentTarget.value))}
								/>
							</label>
						</Show>
					</Show>
				</section>
			</Show>
		</>
	);
}

function ConditionContractEditor(props: {
	definition: NeutralDefinition;
	nodeId: string;
	documentKey?: number;
	onChange: (definition: NeutralDefinition) => void;
}) {
	const node = () => props.definition.control.nodes[props.nodeId];
	const key = () => {
		const n = node();
		return n && "condition" in n ? n.condition : "";
	};
	const value = () => record(props.definition.conditions[key()]);
	function update(next: RecordValue) {
		const n = node();
		if (!n || !("condition" in n)) return;
		const name = key() || uniqueKey(props.definition.conditions, "condition");
		props.onChange({
			...props.definition,
			conditions: { ...props.definition.conditions, [name]: next },
			control: {
				...props.definition.control,
				nodes: { ...props.definition.control.nodes, [props.nodeId]: { ...n, condition: name } },
			},
		});
	}
	const sourceType = (name: string): ValueType => {
		const source = record(value()[name]);
		if (source.source === "projection") return String(source.value_type) as ValueType;
		if (source.source !== "constant") return "string";
		const constant = source.value;
		return constant === null
			? "null"
			: Array.isArray(constant)
				? "array"
				: typeof constant === "number"
					? Number.isInteger(constant)
						? "integer"
						: "number"
					: typeof constant === "boolean"
						? "boolean"
						: typeof constant === "object"
							? "object"
							: "string";
	};
	const [types, setTypes] = createSignal<Record<string, ValueType>>({});
	createEffect(
		on(
			() => [props.nodeId, key(), props.documentKey],
			() => setTypes({}),
		),
	);
	return (
		<section aria-label="Condition contract inspector">
			<h4>Condition</h4>
			<label>
				Operator
				<Select
					aria-label="Condition operator"
					value={String(value().operator ?? "")}
					onChange={(event) => {
						const source = { source: "workflow_input", field: "" };
						const operator = event.currentTarget.value;
						if (operator === "exists") update({ operator, value: source });
						if (operator === "compare")
							update({ operator, comparison: "equal", left: source, right: { source: "constant", value: "" } });
						if (operator === "contains")
							update({ operator, collection: source, value: { source: "constant", value: "" } });
					}}
				>
					<option value="">Choose predicate</option>
					<option value="exists">Value exists</option>
					<option value="compare">Compare values</option>
					<option value="contains">Array contains value</option>
				</Select>
			</label>
			<Show when={value().operator === "compare"}>
				<label>
					Comparison
					<Select
						value={String(value().comparison)}
						onChange={(event) => update({ ...value(), comparison: event.currentTarget.value })}
					>
						<option value="equal">Equal</option>
						<option value="less">Less</option>
						<option value="less_or_equal">Less or equal</option>
						<option value="greater">Greater</option>
						<option value="greater_or_equal">Greater or equal</option>
					</Select>
				</label>
			</Show>
			<For
				each={
					value().operator === "compare"
						? ["left", "right"]
						: value().operator === "contains"
							? ["collection", "value"]
							: value().operator === "exists"
								? ["value"]
								: []
				}
			>
				{(name) => (
					<>
						<label>
							{name} value type
							<Select
								aria-label={`${name} condition value type`}
								value={types()[`${key()}/${name}`] ?? (name === "collection" ? "array" : sourceType(name))}
								onChange={(event) =>
									setTypes({ ...types(), [`${key()}/${name}`]: event.currentTarget.value as ValueType })
								}
							>
								<For each={workflowValueTypes}>{(type) => <option value={type}>{type}</option>}</For>
							</Select>
						</label>
						<MappingFields
							schema={{
								fields: {
									[name]: field(types()[`${key()}/${name}`] ?? (name === "collection" ? "array" : sourceType(name))),
								},
							}}
							bindings={value()[name] ? { [name]: value()[name] as unknown as DataSource } : {}}
							onChange={(bindings) => {
								const next = { ...value() };
								if (bindings[name]) next[name] = bindings[name] as unknown as JsonValue;
								else delete next[name];
								update(next);
							}}
						/>
					</>
				)}
			</For>
			<p>
				Unknown remains a separate route; absent is not null. Exists(null) is true. Backend validation checks operand
				types and source availability.
			</p>
		</section>
	);
}
