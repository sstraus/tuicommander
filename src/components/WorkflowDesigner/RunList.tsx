import { For, Show } from "solid-js";
import type { RunView } from "./neutralInspectionTypes";
import s from "./WorkflowDesigner.module.css";

/** Ordering and waiting state are supplied by the run ledger. */
export function RunList(props: { runs: RunView[]; selected: string; onSelect: (id: string) => void }) {
	return (
		<section aria-label="Active and recent runs">
			<h4>Active and recent runs</h4>
			<Show when={props.runs.length} fallback={<p>No retained runs for this project.</p>}>
				<For each={props.runs}>
					{(run) => (
						<button
							type="button"
							data-control-layout="row"
							class={s.runRow}
							aria-pressed={props.selected === run.id}
							onClick={() => props.onSelect(run.id)}
						>
							<strong>
								{run.name} · revision {run.publication.revision}
							</strong>
							<span>
								{run.waiting_on_approval
									? "Waiting for approval"
									: run.resource_waits?.length
										? `Waiting on resource ${run.resource_waits[0].resource}, held by run ${run.resource_waits[0].owner_run}`
										: run.status.type}{" "}
								· {run.current_node}
							</span>
							<small>
								{run.id} · updated {new Date(run.updated_ms).toLocaleString()}
							</small>
						</button>
					)}
				</For>
			</Show>
		</section>
	);
}
