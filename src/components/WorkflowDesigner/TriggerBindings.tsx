import { createSignal, For, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Switch } from "../ui/Switch";
import { Textarea } from "../ui/Textarea";
import type { NeutralPublication } from "./neutralClient";
import s from "./TriggerBindings.module.css";
import { TriggerMappings } from "./TriggerMappings";
import type { TriggerBindingDraft } from "./triggerTypes";
import { useTriggerBinding } from "./useTriggerBinding";

export function TriggerBindings(props: { project: string; publications: NeutralPublication[] }) {
	const state = useTriggerBinding(
		() => props.project,
		() => props.publications,
	);
	const [options, setOptions] = createSignal("");
	const [optionsError, setOptionsError] = createSignal("");
	return (
		<section class={s.panel} aria-label="Trigger bindings">
			<header class={s.header}>
				<div>
					<h4>Trigger bindings</h4>
					<p>Preview payload mappings before saving or enabling a binding.</p>
				</div>
				<div class={s.bindingList}>
					<Button disabled={state.busy()} onClick={state.newBinding}>
						New binding
					</Button>
					<Button disabled={state.busy()} onClick={() => void state.perform(state.refresh)}>
						Refresh bindings
					</Button>
				</div>
			</header>
			<Show when={state.error()}>
				<p role="alert">{state.error()}</p>
			</Show>
			<p>Loading a stored binding replaces unsaved binding edits.</p>
			<div class={s.bindingList}>
				<For each={state.bindings()}>
					{(binding) => (
						<Button disabled={state.busy()} onClick={() => state.choose(binding)}>
							Load {binding.draft.id} · revision {binding.revision} · {binding.enabled ? "enabled" : "disabled"}
						</Button>
					)}
				</For>
			</div>
			<fieldset disabled={state.busy()} class={s.form}>
				<div class={s.columns}>
					<div class={s.fields}>
						<label>
							Binding ID
							<Input
								value={state.draft().id}
								disabled={state.revision() > 0}
								onInput={(e) => state.edit({ ...state.draft(), id: e.currentTarget.value })}
							/>
						</label>
						<label>
							Published workflow
							<Select
								value={`${state.draft().publication.id}@${state.draft().publication.revision}`}
								onChange={(e) => {
									const value = props.publications.find(
										(item) => `${item.id}@${item.revision}` === e.currentTarget.value,
									);
									if (value)
										state.edit({
											...state.draft(),
											publication: { project: value.project, id: value.id, revision: value.revision },
											mapping: { inputs: {}, subject: {} },
										});
								}}
							>
								<option value="@0">Select a pinned workflow</option>
								<For each={props.publications}>
									{(item) => (
										<option value={`${item.id}@${item.revision}`}>
											{item.id} · revision {item.revision}
										</option>
									)}
								</For>
							</Select>
						</label>
						<label>
							Trigger source
							<Select value={state.draft().source} disabled>
								<option value="github">GitHub</option>
							</Select>
						</label>
						<label>
							Resource kind
							<Select
								value={state.draft().kind}
								onChange={(e) =>
									state.edit({ ...state.draft(), kind: e.currentTarget.value, mapping: { inputs: {}, subject: {} } })
								}
							>
								<option value="issue">Issue</option>
								<option value="pull_request">Pull request</option>
							</Select>
						</label>
						<label>
							Resource prefix
							<Input
								placeholder="github:owner/repository/issue/"
								value={state.draft().resource_prefix}
								onInput={(e) => state.edit({ ...state.draft(), resource_prefix: e.currentTarget.value })}
							/>
						</label>
						<label>
							Events (comma separated)
							<Input
								value={state.draft().events.join(", ")}
								onChange={(e) =>
									state.edit({
										...state.draft(),
										events: e.currentTarget.value
											.split(",")
											.map((v) => v.trim())
											.filter(Boolean),
									})
								}
							/>
						</label>
						<details>
							<summary>Filters and run limits</summary>
							<p>JSON fields: filters, ignored_actors, limits. Validated by the backend on preview and save.</p>
							<Textarea
								aria-label="Trigger options JSON"
								value={options()}
								placeholder={JSON.stringify(
									{
										filters: state.draft().filters,
										ignored_actors: state.draft().ignored_actors,
										limits: state.draft().limits,
									},
									null,
									2,
								)}
								onInput={(e) => setOptions(e.currentTarget.value)}
							/>
							<Button
								onClick={() => {
									try {
										const value = JSON.parse(options()) as Pick<
											TriggerBindingDraft,
											"filters" | "ignored_actors" | "limits"
										>;
										state.edit({
											...state.draft(),
											filters: value.filters,
											ignored_actors: value.ignored_actors,
											limits: value.limits,
										});
										setOptionsError("");
									} catch {
										setOptionsError("Enter a JSON object with filters, ignored_actors and limits.");
									}
								}}
							>
								Apply trigger options
							</Button>
							<Show when={optionsError()}>
								<p role="alert">{optionsError()}</p>
							</Show>
						</details>
					</div>
					<div class={s.fields}>
						<Show when={state.publication()} fallback={<p>Select a published workflow to map its typed inputs.</p>}>
							{(publication) => (
								<>
									<TriggerMappings
										title="Workflow inputs"
										source={state.schema()}
										target={publication().definition.inputs}
										mapping={state.draft().mapping.inputs}
										onChange={(inputs) =>
											state.edit({ ...state.draft(), mapping: { ...state.draft().mapping, inputs } })
										}
									/>
									<Show when={publication().definition.subject}>
										{(subject) => (
											<TriggerMappings
												title="Workflow subject"
												source={state.schema()}
												target={subject().fields}
												mapping={state.draft().mapping.subject}
												onChange={(value) =>
													state.edit({ ...state.draft(), mapping: { ...state.draft().mapping, subject: value } })
												}
											/>
										)}
									</Show>
								</>
							)}
						</Show>
						<label>
							Normalized event snapshot
							<Textarea
								value={state.sample()}
								onInput={(e) => state.setSample(e.currentTarget.value)}
								placeholder="Paste a normalized trigger event JSON snapshot"
							/>
						</label>
						<Button disabled={!state.publication() || !state.sample()} onClick={() => void state.previewMapping()}>
							Preview trigger mapping
						</Button>
						<Show when={state.preview()}>
							{(preview) => (
								<section aria-label="Trigger mapping preview">
									<h5>Mapped payload</h5>
									<pre>{JSON.stringify({ inputs: preview().inputs, subject: preview().subject }, null, 2)}</pre>
									<h5>Overlapping enabled bindings</h5>
									<For each={Object.entries(preview().overlaps)} fallback={<p>No overlapping enabled bindings.</p>}>
										{([id, revision]) => (
											<p>
												{id} · revision {revision}
											</p>
										)}
									</For>
									<label>
										<Switch
											role="checkbox"
											checked={state.acknowledged()}
											onChange={(e) => state.setAcknowledged(e.currentTarget.checked)}
										/>
										I acknowledge these overlapping binding revisions before enabling.
									</label>
								</section>
							)}
						</Show>
					</div>
				</div>
				<footer class={s.header}>
					<p>
						Revision {state.revision()} · {state.enabled() ? "enabled" : "disabled"}
						{state.dirty() ? " · unsaved changes" : ""}
					</p>
					<div class={s.bindingList}>
						<Button disabled={!state.draft().id || !state.publication()} onClick={() => void state.save()}>
							Save disabled binding
						</Button>
						<Button
							disabled={
								state.dirty() || !state.revision() || (!state.enabled() && (!state.preview() || !state.acknowledged()))
							}
							onClick={() => void state.toggle()}
						>
							{state.enabled() ? "Disable binding" : "Enable binding"}
						</Button>
					</div>
				</footer>
			</fieldset>
		</section>
	);
}
