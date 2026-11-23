import { createEffect, createSignal, For, Show } from "solid-js";
import { workflowsUi } from "../../stores/workflowsUi";
import { randomUuid } from "../../utils/randomId";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { SegmentedControl } from "../ui/SegmentedControl";
import { ActivityPresets } from "./ActivityPresets";
import { BindingChoicesContext } from "./bindingChoices";
import { CatalogInspector } from "./CatalogInspector";
import { CorrectionRegionEditor } from "./CorrectionRegion";
import type { ActivityDescriptor } from "./catalogTypes";
import { DefinitionJson } from "./DefinitionJson";
import { MappingFields } from "./MappingFields";
import { NeutralControlInspector } from "./NeutralControlInspector";
import { NeutralGraphEditor } from "./NeutralGraphEditor";
import { NeutralInspection } from "./NeutralInspection";
import { NeutralScopeInspector } from "./NeutralScopeInspector";
import type { ControlNode, NeutralPublication } from "./neutralClient";
import { SubflowPalette } from "./SubflowPalette";
import { TriggerBindings } from "./TriggerBindings";
import { useNeutralDraft } from "./useNeutralDraft";
import s from "./WorkflowDesigner.module.css";
import { WorkflowInterface } from "./WorkflowInterface";

/** Neutral workflow authoring surface. */
export function NeutralDesigner(props: { project: string }) {
	const state = useNeutralDraft(() => props.project);
	const [selected, setSelectedNode] = createSignal("start");
	const [selectedBody, setSelectedBody] = createSignal<number>();
	const setSelected = (id: string) => {
		setSelectedBody(undefined);
		setSelectedNode(id);
	};
	const [pin, setPin] = createSignal<NeutralPublication>();
	const [triggersOpen, setTriggersOpen] = createSignal(false);
	const node = () => state.definition().control.nodes[selected()];
	const ordinal = () => {
		if (selectedBody() !== undefined) return selectedBody();
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
	const [search, setSearch] = createSignal("");
	const [view, setView] = createSignal<"design" | "interface" | "inspect" | "advanced">("design");
	const [inspectionOpened, setInspectionOpened] = createSignal(false);
	// A run opened from elsewhere (PR review start) lands on Preview & runs.
	createEffect(() => {
		if (!workflowsUi.run()) return;
		setInspectionOpened(true);
		setView("inspect");
	});
	const [documentKey, setDocumentKey] = createSignal(0);
	function addControl(node: ControlNode) {
		const definition = state.definition();
		const id = randomUuid();
		state.edit({
			...definition,
			control: { ...definition.control, nodes: { ...definition.control.nodes, [id]: node } },
		});
		setSelected(id);
	}
	function add(descriptor: ActivityDescriptor) {
		const definition = state.definition();
		const id = randomUuid();
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
		<BindingChoicesContext.Provider
			value={{
				definition: state.definition,
				activities: () => state.catalog().activities,
				labels: () => state.editor().labels,
			}}
		>
			{/* Toasts default to the bottom-left while a dialog is open, which is this palette; keep them over the canvas. */}
			<section class={s.designer} aria-label="Catalog workflow designer" data-toast-placement="center">
				<header class={s.header}>
					<div>
						<h3>Catalog workflow designer</h3>
						<p class={s.muted}>
							{state.dirty()
								? "Unsaved changes"
								: state.draft()
									? `Saved · revision ${state.draft()?.revision}`
									: "New draft"}{" "}
							·{" "}
							{state.draft()?.latest_published_revision
								? `Published revision ${state.draft()?.latest_published_revision}`
								: "Not published"}
						</p>
					</div>
					<div class={s.actions}>
						<Button disabled={state.busy()} onClick={() => void state.perform(state.refresh)}>
							Refresh catalog
						</Button>
						<Button
							data-ready={state.dirty() ? "true" : undefined}
							disabled={state.busy()}
							onClick={() => void state.perform(state.save)}
						>
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
				<div class={s.workspaceTabs}>
					<SegmentedControl
						aria-label="Workflow workspace"
						value={view()}
						onChange={(next) => {
							if (next === "inspect") setInspectionOpened(true);
							setView(next);
						}}
						options={[
							{ id: "design", label: "Design" },
							{ id: "interface", label: "Inputs & returns" },
							{ id: "inspect", label: "Preview & runs" },
							{ id: "advanced", label: "Advanced definition" },
						]}
					/>
				</div>
				<fieldset disabled={state.busy()} class={s.body} hidden={view() !== "design"}>
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
						<Button
							disabled={state.dirty()}
							onClick={() => {
								state.newDraft();
								setSelected("start");
								setDocumentKey((key) => key + 1);
							}}
						>
							New blank draft
						</Button>
						<div class={s.palette}>
							<For each={state.templates()}>
								{(template) => (
									<Button
										disabled={state.dirty()}
										title={`New from template: ${template.name} · ${template.description}`}
										onClick={() => {
											state.fromTemplate(template);
											setDocumentKey((key) => key + 1);
											setSelected("start");
										}}
									>
										<span class={s.catalogName}>New from template: {template.name}</span>
									</Button>
								)}
							</For>
							<For each={state.drafts()}>
								{(draft) => (
									<Button
										disabled={state.dirty()}
										title={`${draft.name} · revision ${draft.revision}`}
										onClick={() => {
											state.choose(draft);
											setDocumentKey((key) => key + 1);
											setSelected("start");
										}}
									>
										<span class={s.catalogName}>
											{draft.name} · revision {draft.revision}
										</span>
									</Button>
								)}
							</For>
						</div>
						<h4>Control palette</h4>
						<div class={s.palette}>
							<Button onClick={() => addControl({ type: "wait", duration_ms: 1000 })}>Add wait</Button>
							<Button onClick={() => addControl({ type: "condition", condition: "" })}>Add condition</Button>
							<Button onClick={() => addControl({ type: "fork", scope: "" })}>Add fork</Button>
							<Button onClick={() => addControl({ type: "collection", scope: "" })}>Add collection</Button>
							<Button onClick={() => addControl({ type: "loop", scope: "", condition: "" })}>Add loop</Button>
							<Button onClick={() => addControl({ type: "end", outcome: "completed" })}>Add end</Button>
						</div>
						<h4>Activities</h4>
						<label>
							Search catalog
							<Input type="search" value={search()} onInput={(event) => setSearch(event.currentTarget.value)} />
						</label>
						<div class={s.palette}>
							<Show
								when={state.catalog().activities.length}
								fallback={<p>No catalog activities loaded. Refresh when the neutral backend is available.</p>}
							>
								<For
									each={state
										.catalog()
										.activities.filter((item) =>
											`${item.id.namespace} ${item.id.name} ${item.purpose}`
												.toLowerCase()
												.includes(search().toLowerCase()),
										)}
								>
									{(item) => (
										<Button onClick={() => add(item)} title={`${item.id.namespace}/${item.id.name} · ${item.purpose}`}>
											<span class={s.catalogName}>
												Add {item.id.name} · v{item.id.version}
											</span>
										</Button>
									)}
								</For>
							</Show>
						</div>
						<h4>Published workflows</h4>
						<SubflowPalette
							state={state}
							onAdd={add}
							onInspect={(item) =>
								void state.perform(async () => {
									setView("advanced");
									setPin(
										await state.call(
											{ action: "get_publication", id: item.id, revision: item.revision },
											"publication",
										),
									);
								})
							}
						/>
					</aside>
					<NeutralGraphEditor
						documentKey={documentKey()}
						definition={state.definition()}
						activities={state.catalog().activities}
						labels={state.editor().labels}
						selected={selected()}
						selectedBody={selectedBody()}
						onInspectBody={setSelectedBody}
						onSelect={setSelected}
						onChange={state.edit}
					/>
					<aside class={s.inspector} aria-label="Neutral node inspector">
						<NeutralControlInspector
							definition={state.definition()}
							activities={state.catalog().activities}
							selected={selected()}
							labels={state.editor().labels}
							onChange={state.edit}
							onLabel={(label) =>
								state.editEditor({ ...state.editor(), labels: { ...state.editor().labels, [selected()]: label } })
							}
						/>
						<NeutralScopeInspector
							definition={state.definition()}
							activities={state.catalog().activities}
							nodeId={selected()}
							documentKey={documentKey()}
							onChange={(definition) => {
								setSelectedBody(undefined);
								state.edit(definition);
							}}
							onInspectBody={setSelectedBody}
						/>
						<Show when={selectedBody() !== undefined}>
							<p>Editing scoped body · activity {selectedBody()}</p>
							<Button onClick={() => setSelectedBody(undefined)}>Back to scope</Button>
						</Show>
						<Show when={descriptor()}>
							{(entry) => (
								<Show when={instance()}>
									{(current) => (
										<>
											<CatalogInspector
												descriptor={entry()}
												instance={current()}
												label={state.editor().labels[selected()] ?? ""}
												hideLabel={selectedBody() !== undefined}
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
					</aside>
				</fieldset>
				<fieldset disabled={state.busy()} hidden={view() !== "interface"} class={s.workspacePanel}>
					<WorkflowInterface definition={state.definition()} onChange={state.edit} />
				</fieldset>
				<fieldset disabled={state.busy()} hidden={view() !== "inspect"} class={s.workspacePanel}>
					<Show when={inspectionOpened()}>
						<NeutralInspection state={state} project={props.project} />
					</Show>
				</fieldset>
				<div hidden={view() !== "advanced"} class={s.workspacePanel}>
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
											setSelected("start");
											setDocumentKey((key) => key + 1);
											setView("design");
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
								setSelected("start");
								setDocumentKey((key) => key + 1);
							})
						}
						onError={state.setError}
					/>
				</div>
			</section>
		</BindingChoicesContext.Provider>
	);
}
