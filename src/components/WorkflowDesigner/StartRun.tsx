import { createSignal, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Textarea } from "../ui/Textarea";
import type { JsonValue } from "./catalogTypes";
import type { Inspection } from "./neutralInspectionTypes";
import { SchemaFields } from "./SchemaFields";
import type { useNeutralDraft } from "./useNeutralDraft";
import s from "./WorkflowDesigner.module.css";

/** Starts the latest publication of the open draft; the backend validates inputs and limits. */
export function StartRun(props: {
	state: ReturnType<typeof useNeutralDraft>;
	project: string;
	onStarted: (inspection: Inspection) => void;
}) {
	const [inputs, setInputs] = createSignal<Record<string, JsonValue>>({});
	const [subject, setSubject] = createSignal("null");
	const [activations, setActivations] = createSignal(100);
	const [concurrency, setConcurrency] = createSignal(4);
	const [minutes, setMinutes] = createSignal(10);
	const published = () => props.state.draft()?.latest_published_revision ?? 0;
	const pending = () => props.state.dirty() || (props.state.draft()?.revision ?? 0) > published();
	return (
		<section aria-label="Start a run">
			<h4>Start a run</h4>
			<Show
				when={published() > 0}
				fallback={<p>Publish the draft to start a run. A run always uses a published revision.</p>}
			>
				<p>
					Starts published revision {published()}.
					<Show when={pending()}> Edits made after that publication are not part of the run.</Show>
				</p>
				<SchemaFields schema={props.state.definition().inputs} values={inputs()} onChange={setInputs} />
				<Show when={props.state.definition().subject}>
					<label>
						Subject (JSON or null)
						<Textarea value={subject()} onInput={(event) => setSubject(event.currentTarget.value)} />
					</label>
				</Show>
				<div class={s.startRun}>
					<label>
						Max activations
						<Input
							type="number"
							min="1"
							value={activations()}
							onInput={(e) => setActivations(Number(e.currentTarget.value))}
						/>
					</label>
					<label>
						Max concurrency
						<Input
							type="number"
							min="1"
							value={concurrency()}
							onInput={(e) => setConcurrency(Number(e.currentTarget.value))}
						/>
					</label>
					<label>
						Max duration (minutes)
						<Input type="number" min="1" value={minutes()} onInput={(e) => setMinutes(Number(e.currentTarget.value))} />
					</label>
					<Button
						disabled={props.state.busy() || !props.state.draft()}
						onClick={() =>
							void props.state.perform(async () => {
								const draft = props.state.draft();
								if (!draft) return;
								props.onStarted(
									await props.state.call(
										{
											action: "start",
											request: {
												publication: { project: props.project, id: draft.id, revision: published() },
												request_id: crypto.randomUUID(),
												inputs: inputs(),
												subject: props.state.definition().subject ? JSON.parse(subject()) : null,
												limits: {
													max_activations: activations(),
													max_concurrency: concurrency(),
													max_duration_ms: Math.round(minutes() * 60_000),
												},
											},
										},
										"inspection",
									),
								);
							})
						}
					>
						Start run
					</Button>
				</div>
			</Show>
		</section>
	);
}
