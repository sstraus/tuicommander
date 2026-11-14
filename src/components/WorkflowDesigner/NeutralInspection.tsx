import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import { subscribeEvents } from "../../transport";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Textarea } from "../ui/Textarea";
import { RetainedCorrections } from "./CorrectionRegion";
import type { Inspection, Preview } from "./neutralInspectionTypes";
import type { useNeutralDraft } from "./useNeutralDraft";

export function NeutralInspection(props: { state: ReturnType<typeof useNeutralDraft>; project: string }) {
	const [sample, setSample] = createSignal("{}");
	const [subject, setSubject] = createSignal("null");
	const [preview, setPreview] = createSignal<Preview>();
	const [inspection, setInspection] = createSignal<Inspection>();
	const [run, setRun] = createSignal("");
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
				if (event?.repo_path === props.project && event.payload?.runId === run())
					void inspect().catch((cause) => {
						if (!disposed) props.state.setError(String(cause));
					});
			},
		},
		{
			onResync: () => {
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
									<summary>Activity {activity.activity} · sample input and effect intent</summary>
									<pre>{JSON.stringify(activity.inputs, null, 2)}</pre>
									<p>{activity.unavailable_reason}</p>
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
			<h4>Run inspection</h4>
			<label>
				Run ID
				<Input
					value={run()}
					onInput={(event) => {
						setRun(event.currentTarget.value);
						setInspection(undefined);
					}}
				/>
			</label>
			<Button disabled={!run()} onClick={() => void props.state.perform(() => inspect())}>
				Inspect retained run
			</Button>
			<Show when={inspection()}>
				{(result) => (
					<section aria-label="Retained run">
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
