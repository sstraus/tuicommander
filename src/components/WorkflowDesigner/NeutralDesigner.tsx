import { createSignal, For, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { ActivityPresets } from "./ActivityPresets";
import { CatalogInspector } from "./CatalogInspector";
import { CorrectionRegionEditor } from "./CorrectionRegion";
import type { ActivityDescriptor } from "./catalogTypes";
import { DefinitionJson } from "./DefinitionJson";
import { MappingFields } from "./MappingFields";
import { NeutralGraphEditor } from "./NeutralGraphEditor";
import { NeutralInspection } from "./NeutralInspection";
import type { NeutralPublication } from "./neutralClient";
import { TriggerBindings } from "./TriggerBindings";
import { useNeutralDraft } from "./useNeutralDraft";
import s from "./WorkflowDesigner.module.css";

/** Neutral workflow authoring surface. */
export function NeutralDesigner(props: { project: string }) {
	const state = useNeutralDraft(() => props.project);
	const [selected, setSelected] = createSignal("start");
	const [pin, setPin] = createSignal<NeutralPublication>();
	const [triggersOpen, setTriggersOpen] = createSignal(false);
	const node = () => state.definition().control.nodes[selected()];
	const ordinal = () => {
		const current = node();
		return current?.type === "activity" ? current.activity : undefined;
	};
	const instance = () => {
		const index = ordinal();
		return index === undefined ? undefined : state.definition().instances[index];
	};
	const descriptor = () => {
		const id = instance()?.descriptor;
		return state
			.catalog()
			.activities.find(
				(item) => item.id.namespace === id?.namespace && item.id.name === id?.name && item.id.version === id?.version,
			);
	};
	function add(descriptor: ActivityDescriptor) {
		const definition = state.definition();
		const id = crypto.randomUUID();
		state.edit({
			...definition,
			instances: [...definition.instances, { descriptor: descriptor.id, config: {}, execution_policy: "" }],
			control: {
				...definition.control,
				nodes: { ...definition.control.nodes, [id]: { type: "activity", activity: definition.instances.length } },
			},
		});
		setSelected(id);
	}

	return (
		<section class={s.designer} aria-label="Catalog workflow designer">
			<header class={s.header}>
				<div>
					<h3>Catalog workflow designer</h3>
					<p class={s.muted}>Project-owned neutral definitions</p>
				</div>
				<div class={s.actions}>
					<Button disabled={state.busy()} onClick={() => void state.perform(state.refresh)}>
						Refresh catalog
					</Button>
					<Button disabled={state.busy()} onClick={() => void state.perform(state.save)}>
						Save neutral draft
					</Button>
					<Button
						disabled={state.busy()}
						onClick={() =>
							void state.perform(async () => {
								await state.validate();
							})
						}
					>
						Validate links and inputs
					</Button>
					<Button
						disabled={state.busy() || state.dirty() || !state.draft()}
						onClick={() => void state.perform(state.publish)}
					>
						Publish neutral draft
					</Button>
				</div>
			</header>
			<Show when={state.error() || state.catalogError()}>
				<p class={s.error} role="alert">
					{state.error() || state.catalogError()}
				</p>
			</Show>
			<For each={state.diagnostics()}>
				{(item) => (
					<p class={s.error}>
						{item.path} · {item.message}
					</p>
				)}
			</For>
			<fieldset disabled={state.busy()} class={s.body}>
				<aside class={s.sidebar} aria-label="Catalog palette">
					<label>
						Workflow name
						<Input
							value={state.name()}
							onInput={(event) => {
								state.setName(event.currentTarget.value);
								state.setDirty(true);
							}}
						/>
					</label>
					<h4>Drafts</h4>
					<Button disabled={state.dirty()} onClick={state.newDraft}>
						New blank draft
					</Button>
					<For each={state.templates()}>
						{(template) => (
							<Button
								disabled={state.dirty()}
								title={template.description}
								onClick={() => {
									state.fromTemplate(template);
									setSelected("start");
								}}
							>
								New from template: {template.name}
							</Button>
						)}
					</For>
					<For each={state.drafts()}>
						{(draft) => (
							<Button
								onClick={() => {
									state.choose(draft);
									setSelected("start");
								}}
							>
								{draft.name} · revision {draft.revision}
							</Button>
						)}
					</For>
					<h4>Activities</h4>
					<Show
						when={state.catalog().activities.length}
						fallback={<p>No catalog activities loaded. Refresh when the neutral backend is available.</p>}
					>
						<For each={state.catalog().activities}>
							{(item) => (
								<Button onClick={() => add(item)}>
									Add {item.id.name} · v{item.id.version}
								</Button>
							)}
						</For>
					</Show>
					<h4>Reusable metanodes</h4>
					<For each={state.catalog().metanodes}>
						{(item) => (
							<Button
								onClick={() =>
									void state.perform(async () => {
										setPin(
											await state.call(
												{ action: "get_publication", id: item.id, revision: item.revision },
												"publication",
											),
										);
									})
								}
							>
								Inspect {item.id} · revision {item.revision}
							</Button>
						)}
					</For>
				</aside>
				<NeutralGraphEditor
					definition={state.definition()}
					activities={state.catalog().activities}
					labels={state.editor().labels}
					selected={selected()}
					onSelect={setSelected}
					onChange={state.edit}
				/>
				<aside class={s.inspector} aria-label="Neutral node inspector">
					<Show when={descriptor()}>
						{(entry) => (
							<Show when={instance()}>
								{(current) => (
									<>
										<CatalogInspector
											descriptor={entry()}
											instance={current()}
											label={state.editor().labels[selected()] ?? ""}
											onLabel={(label) =>
												state.editEditor({
													...state.editor(),
													labels: { ...state.editor().labels, [selected()]: label },
												})
											}
											onChange={(next) => {
												const index = ordinal();
												if (index === undefined) return;
												state.edit({
													...state.definition(),
													instances: state.definition().instances.map((value, i) => (i === index ? next : value)),
												});
											}}
										/>

										<MappingFields
											schema={entry().inputs}
											bindings={state.definition().input_bindings[String(ordinal())] ?? {}}
											onChange={(bindings) =>
												state.edit({
													...state.definition(),
													input_bindings: { ...state.definition().input_bindings, [String(ordinal())]: bindings },
												})
											}
										/>
										<ActivityPresets
											busy={state.busy()}
											onSave={(id, revision) =>
												state.perform(async () => {
													await state.call(
														{ action: "save_preset", id, expected_revision: revision - 1, instance: current() },
														"preset",
													);
												})
											}
											onLoad={(id, revision) =>
												state.perform(async () => {
													const saved = await state.call({ action: "get_preset", id, revision }, "preset");
													const index = ordinal();
													if (index === undefined) return;
													state.edit({
														...state.definition(),
														instances: state
															.definition()
															.instances.map((value, i) => (i === index ? saved.instance : value)),
													});
												})
											}
										/>
									</>
								)}
							</Show>
						)}
					</Show>
					<NeutralInspection state={state} project={props.project} />
				</aside>
			</fieldset>
			<Show when={pin()}>
				{(publication) => (
					<section class={s.canvas} aria-label="Pinned metanode graph">
						<h4>
							{publication().id} · revision {publication().revision} · read only
						</h4>
						<pre>{JSON.stringify(publication().definition, null, 2)}</pre>
						<Button
							disabled={state.busy() || state.dirty()}
							onClick={() =>
								void state.perform(async () => {
									const draft = await state.call(
										{
											action: "clone_publication",
											id: publication().id,
											revision: publication().revision,
											name: `${publication().id} draft`,
										},
										"draft",
									);
									state.choose(draft);
									setPin(undefined);
									await state.refresh();
								})
							}
						>
							Customize in a new draft
						</Button>
					</section>
				)}
			</Show>
			<details onToggle={(event) => setTriggersOpen(event.currentTarget.open)}>
				<summary>Trigger bindings</summary>
				<Show when={triggersOpen()}>
					<TriggerBindings project={props.project} publications={state.catalog().metanodes} />
				</Show>
			</details>
			<CorrectionRegionEditor state={state} />
			<DefinitionJson
				definition={state.definition()}
				onApply={(value) =>
					state.perform(async () => {
						const checked = await state.call({ action: "validate", definition: value }, "validation");
						if (!checked.valid) {
							state.setError(checked.diagnostics.map((item) => `${item.path}: ${item.message}`).join("\n"));
							return;
						}
						state.edit(value as import("./neutralClient").NeutralDefinition);
					})
				}
				onError={state.setError}
			/>
		</section>
	);
}
