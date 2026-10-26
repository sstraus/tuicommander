import { createSignal, For, Show } from "solid-js";
import { t } from "../../i18n";
import type { AutomationsStore } from "../../stores/automations";
import { terminalsStore } from "../../stores/terminals";
import { getSessionConnection } from "../../transportRuntime";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { Button } from "../ui/Button";
import s from "./AutomationsDialog.module.css";
import type { AutomationRun } from "./contract";
import { zonedTime } from "./ScheduleFields";
import { WorktreeCleanup } from "./WorktreeCleanup";

/** Never resurrect a historical session or navigate to a different daemon. */
export function focusAutomationRun(run: AutomationRun, connectionId?: string): boolean {
	if (!run.session_id) return false;
	const id = terminalsStore.getTerminalForSession(run.session_id);
	const terminal = id ? terminalsStore.get(id) : undefined;
	if (
		!id ||
		!terminal ||
		terminal.suspended ||
		terminal.shellState === "exited" ||
		getSessionConnection(run.session_id) !== connectionId
	)
		return false;
	navigateToTerminal(id);
	return true;
}

export function RunOutput(props: { run: AutomationRun }) {
	return (
		<section class={s.group} aria-label={t("automations.savedOutput", "Saved run output")}>
			<h3>
				{props.run.definition.name} — {props.run.status}
			</h3>
			<p>{t("automations.sessionUnavailable", "Live session unavailable. No replacement run was started.")}</p>
			<dl>
				<dt>{t("automations.repository", "Repository")}</dt>
				<dd>{props.run.definition.repository}</dd>
				<dt>{t("automations.recordedWorkspace", "Workspace")}</dt>
				<dd>{props.run.workspace ?? t("automations.noWorkspace", "No workspace was recorded.")}</dd>
				<dt>{t("automations.runConfig", "Run config")}</dt>
				<dd>{props.run.definition.run_config}</dd>
			</dl>
			<For
				each={[
					{ label: "stdout", output: props.run.stdout },
					{ label: "stderr", output: props.run.stderr },
				]}
			>
				{({ label, output }) => (
					<div>
						<h4>{label}</h4>
						<pre class={s.output}>{output.text || t("automations.noOutput", "No saved output.")}</pre>
						<Show when={output.truncated}>
							<p>{t("automations.outputTruncated", "Saved output was truncated by the backend.")}</p>
						</Show>
					</div>
				)}
			</For>
		</section>
	);
}

export function RunsView(props: { store: AutomationsStore; connectionId?: string; onNavigate: () => void }) {
	const [selected, setSelected] = createSignal<AutomationRun>();
	return (
		<section class={s.history} aria-label={t("automations.runs", "Runs")}>
			<div class={s.row}>
				<h3>{t("automations.runs", "Runs")}</h3>
				<Button type="button" disabled={props.store.runsLoading()} onClick={() => void props.store.loadRuns()}>
					{t("automations.refreshHistory", "Refresh history")}
				</Button>
			</div>
			<div class={s.summaries}>
				<For each={["24h", "7d"]}>
					{(window) => (
						<section class={s.group}>
							<h4>{window}</h4>
							<Show when={props.store.summaries()[window]}>
								{(summary) => (
									<>
										<strong>
											{t("automations.totalRuns", "Total runs")}: {summary().total}
										</strong>
										<For each={Object.entries(summary().by_status)}>
											{([status, count]) => (
												<span>
													{status}: {count}
												</span>
											)}
										</For>
									</>
								)}
							</Show>
						</section>
					)}
				</For>
			</div>
			<Show when={props.store.runsLoading()}>
				<p role="status">{t("automations.loadingHistory", "Loading history…")}</p>
			</Show>
			<Show when={!props.store.runsLoading() && props.store.allRuns().length === 0}>
				<p>{t("automations.noRuns", "No recent runs.")}</p>
			</Show>
			<ol>
				<For each={props.store.allRuns()}>
					{(run) => (
						<li>
							<strong>{run.definition.name}</strong>
							<time>{zonedTime(run.created_ms, run.definition.timezone)}</time>
							<strong class={s.runStatus} data-status={run.status}>
								{run.status}
							</strong>
							<span>{run.definition.repository}</span>
							<Show when={run.reason}>
								<p>{run.reason}</p>
							</Show>
							<WorktreeCleanup run={run} connectionId={props.connectionId} />
							<Button
								type="button"
								onClick={() => {
									if (focusAutomationRun(run, props.connectionId)) props.onNavigate();
									else setSelected(run);
								}}
							>
								{t("automations.viewRun", "View run")}
							</Button>
						</li>
					)}
				</For>
			</ol>
			<div class={s.row}>
				<Button
					type="button"
					disabled={props.store.runsLoading() || props.store.runsOffset() === 0}
					onClick={() => void props.store.loadRuns(Math.max(0, props.store.runsOffset() - 20))}
				>
					{t("automations.previousRuns", "Previous runs")}
				</Button>
				<Button
					type="button"
					disabled={props.store.runsLoading() || !props.store.hasMoreRuns()}
					onClick={() => void props.store.loadRuns(props.store.runsOffset() + 20)}
				>
					{t("automations.nextRuns", "Next runs")}
				</Button>
			</div>
			<Show when={selected()}>{(run) => <RunOutput run={run()} />}</Show>
		</section>
	);
}
