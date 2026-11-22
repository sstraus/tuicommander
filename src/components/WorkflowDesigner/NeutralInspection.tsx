import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import { subscribeEvents } from "../../transport";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import { RetainedCorrections } from "./CorrectionRegion";
import type { JsonValue } from "./catalogTypes";
import { activityTitle } from "./neutralGraph";
import type { Inspection, Preview, RunView } from "./neutralInspectionTypes";
import { RunGraph } from "./RunGraph";
import { RunList } from "./RunList";
import { RunSummary } from "./RunSummary";
import { StartRun } from "./StartRun";
import type { useNeutralDraft } from "./useNeutralDraft";
import s from "./WorkflowDesigner.module.css";

const record = (value: JsonValue | undefined): Record<string, JsonValue> =>
	value && typeof value === "object" && !Array.isArray(value) ? value : {};

export function NeutralInspection(props: { state: ReturnType<typeof useNeutralDraft>; project: string }) {
	const [sample, setSample] = createSignal("{}");
	const [subject, setSubject] = createSignal("null");
	const [preview, setPreview] = createSignal<Preview>();
	const [inspection, setInspection] = createSignal<Inspection>();
	const [run, setRun] = createSignal("");
	const [runs, setRuns] = createSignal<RunView[]>([]);
	let listGeneration = 0;
	let listTimer: ReturnType<typeof setTimeout> | undefined;
	function scheduleRuns() {
		if (listTimer !== undefined) return;
		listTimer = setTimeout(() => {
			listTimer = undefined;
			void refreshRuns().catch((cause) => props.state.setError(String(cause)));
		}, 100);
	}
	async function refreshRuns() {
		const generation = ++listGeneration;
		const result = await props.state.call({ action: "list_runs" }, "runs");
		if (!disposed && generation === listGeneration) setRuns(result);
	}
	createEffect(() => {
		props.project;
		readGeneration++;
		setRun("");
		setInspection(undefined);
		void refreshRuns().catch((cause) => props.state.setError(String(cause)));
	});
	createEffect(
		on(
			() => props.state.definition(),
			() => setPreview(undefined),
		),
	);
	let readGeneration = 0;
	async function inspect(after_sequence = 0) {
		const request = ++readGeneration;
		const id = run();
		const result = await props.state.call({ action: "inspect", run_id: id, after_sequence, limit: 500 }, "inspection");
		if (!disposed && request === readGeneration && run() === id) setInspection(result);
	}
	let disposed = false;
	let unlisten: (() => void) | undefined;
	void subscribeEvents(
		{
			"workflow-run-changed": (payload) => {
				const event = payload as { repo_path?: unknown; payload?: { runId?: unknown } } | null;
				if (event?.repo_path === props.project) scheduleRuns();
				if (event?.repo_path === props.project && event.payload?.runId === run())
					void inspect().catch((cause) => {
						if (!disposed) props.state.setError(String(cause));
					});
			},
		},
		{
			onResync: () => {
				void refreshRuns().catch((cause) => props.state.setError(String(cause)));
				if (run())
					void inspect().catch((cause) => {
						if (!disposed) props.state.setError(String(cause));
					});
			},
		},
	)
		.then((stop) => {
			if (disposed) stop();
			else unlisten = stop;
		})
		.catch((cause) => props.state.setError(String(cause)));
	onCleanup(() => {
		disposed = true;
		clearTimeout(listTimer);
		unlisten?.();
	});
	return (
		<section aria-label="Preview and retained run inspection">
			<h4>Side-effect-free preview</h4>
			<label>
				Sample inputs (JSON)
				<Textarea
					value={sample()}
					onInput={(event) => {
						setSample(event.currentTarget.value);
						setPreview(undefined);
					}}
				/>
			</label>
			<label>
				Sample subject (JSON or null)
				<Textarea
					value={subject()}
					onInput={(event) => {
						setSubject(event.currentTarget.value);
						setPreview(undefined);
					}}
				/>
			</label>
			<Button
				onClick={() =>
					void props.state.perform(async () => {
						setPreview(
							await props.state.call(
								{
									action: "preview",
									definition: props.state.definition(),
									inputs: JSON.parse(sample()),
									subject: JSON.parse(subject()),
								},
								"preview",
							),
						);
					})
				}
			>
				Preview sample IO and effect intents
			</Button>
			<Show when={preview()}>
				{(result) => (
					<section aria-label="Resolved preview">
						<For each={result().validation.diagnostics}>
							{(item) => (
								<p>
									{item.path} · {item.message}
								</p>
							)}
						</For>
						<For each={result().activities}>
							{(activity) => (
								<details open>
									<summary>
										{activityTitle(props.state.definition(), props.state.editor().labels, activity.activity)} · sample
										input and effect intent
									</summary>
									<dl class={s.runSummary}>
										<For each={Object.entries(activity.inputs)}>
											{([name, datum]) => (
												<>
													<dt>{name}</dt>
													<dd>
														{datum.state.state === "present" ? JSON.stringify(datum.state.value) : datum.state.state}
													</dd>
												</>
											)}
										</For>
									</dl>
									<Show when={activity.unavailable_reason}>
										<p>{activity.unavailable_reason}</p>
									</Show>
									<h5>Effect preparation</h5>
									<pre>{JSON.stringify(activity.effect_intent, null, 2)}</pre>
									<h5>Output schema · {activity.outcomes.join(", ")}</h5>
									<pre>{JSON.stringify(activity.outputs, null, 2)}</pre>
								</details>
							)}
						</For>
					</section>
				)}
			</Show>
			<StartRun
				state={props.state}
				project={props.project}
				onStarted={(result) => {
					const id = record(result.snapshot.run).id;
					if (typeof id !== "string") return;
					readGeneration++;
					setRun(id);
					setInspection(result);
				}}
			/>
			<RunList
				runs={runs()}
				selected={run()}
				onSelect={(id) => {
					readGeneration++;
					setRun(id);
					setInspection(undefined);
					void props.state.perform(() => inspect());
				}}
			/>
			<Show when={inspection()}>
				{(result) => (
					<section aria-label="Retained run">
						<Show when={result().progress}>{(progress) => <RunGraph progress={progress()} />}</Show>
						<RunSummary
							inspection={result()}
							definition={result().progress?.publication.definition ?? props.state.definition()}
							labels={result().progress?.publication.editor.labels ?? props.state.editor().labels}
						/>
						<details>
							<summary>Raw snapshot and events</summary>
							<h5>Current node and execution status</h5>
							<pre>
								{JSON.stringify(
									{
										node: result().snapshot.node,
										execution: result().snapshot.execution,
										status: result().snapshot.status,
									},
									null,
									2,
								)}
							</pre>
							<h5>Output and activity results</h5>
							<pre>
								{JSON.stringify(
									{ outputs: result().snapshot.outputs, activity_outputs: result().snapshot.activity_outputs },
									null,
									2,
								)}
							</pre>
							<h5>Wait and subflow boundary</h5>
							<pre>
								{JSON.stringify(
									{
										wait_due_ms: result().snapshot.wait_due_ms,
										active_scope: result().snapshot.active_scope,
										parent: result().snapshot.parent,
										parent_fence: result().snapshot.parent_fence,
									},
									null,
									2,
								)}
							</pre>
							<RetainedCorrections scope={result().snapshot.active_scope} />
							<h5>Effects, failures and uncertainty</h5>
							<pre>{JSON.stringify(result().snapshot.effects, null, 2)}</pre>
							<h5>Recorded path events</h5>
							<pre>{JSON.stringify(result().events, null, 2)}</pre>
						</details>
						<Button
							disabled={result().events.length < 500}
							onClick={() => void props.state.perform(() => inspect(result().events.at(-1)?.sequence ?? 0))}
						>
							Next retained event page
						</Button>
					</section>
				)}
			</Show>
		</section>
	);
}
