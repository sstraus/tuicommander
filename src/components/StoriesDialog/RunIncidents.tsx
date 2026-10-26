import { type Component, createEffect, createSignal, For, on, Show } from "solid-js";
import { invoke } from "../../invoke";
import { Button } from "../ui/Button";
import s from "./StoriesDialog.module.css";

interface Incident {
	runId: string;
	attemptId: string | null;
	storyId: string | null;
	nodeId: string | null;
	sessionId: string | null;
	taskId: string | null;
	source: string;
	cause: string;
	nextAction: string;
}

/** Suggestions are backend-owned text. Reading this section never executes recovery. */
export const RunIncidents: Component<{ project: string; runId: string; sequence: number }> = (props) => {
	const [incidents, setIncidents] = createSignal<Incident[]>([]);
	const [error, setError] = createSignal("");
	const [loading, setLoading] = createSignal(false);
	let request = 0;
	async function refresh() {
		const current = ++request;
		setLoading(true);
		setError("");
		try {
			const reply = await invoke<{ type: string; value: Incident[] }>("workflow_run_action", {
				project: props.project,
				action: { action: "incidents", run_id: props.runId },
			});
			if (reply.type !== "incidents") throw new Error("Invalid incident response");
			if (current === request) setIncidents(reply.value);
		} catch (cause) {
			if (current === request) {
				setIncidents([]);
				setError(String(cause));
			}
		} finally {
			if (current === request) setLoading(false);
		}
	}
	createEffect(
		on(
			() => [props.project, props.runId, props.sequence],
			() => {
				setIncidents([]);
				void refresh();
			},
		),
	);
	return (
		<section class={s.incidents} aria-label="Run incidents" aria-busy={loading()}>
			<div class={s.detailHeader}>
				<h4>Incidents and next steps</h4>
				<Button type="button" class={s.loadMore} disabled={loading()} onClick={() => void refresh()}>
					Refresh incidents
				</Button>
			</div>
			<p class={s.muted}>
				Recorded outcomes for this run. Suggestions require your action. Session and task evidence is available only
				while retained.
			</p>
			<Show when={error()}>
				<p class={s.error} role="alert">
					Could not load incidents: {error()}
				</p>
			</Show>
			<Show when={!loading() && !error() && incidents().length === 0}>
				<p class={s.muted}>No recorded incidents.</p>
			</Show>
			<For each={incidents()}>
				{(item) => (
					<article class={s.incident}>
						<span class={s.eyebrow}>{item.source.replaceAll("_", " ")}</span>
						<p class={s.incidentCause}>{item.cause}</p>
						<dl class={s.incidentIds}>
							<dt>Run</dt>
							<dd>{item.runId}</dd>
							<Show when={item.storyId}>
								<dt>Story</dt>
								<dd>{item.storyId}</dd>
							</Show>
							<Show when={item.attemptId}>
								<dt>Attempt</dt>
								<dd>
									{item.attemptId} · {item.nodeId}
								</dd>
							</Show>
							<Show when={item.sessionId}>
								<dt>Session</dt>
								<dd>{item.sessionId}</dd>
							</Show>
							<Show when={item.taskId}>
								<dt>Task</dt>
								<dd>{item.taskId}</dd>
							</Show>
						</dl>
						<p class={s.incidentNext}>
							<strong>Next action:</strong> {item.nextAction}
						</p>
					</article>
				)}
			</For>
		</section>
	);
};
