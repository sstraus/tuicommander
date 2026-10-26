import { For, Show } from "solid-js";
import { t } from "../../i18n";
import type { AutomationsStore } from "../../stores/automations";
import { terminalsStore } from "../../stores/terminals";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { Button } from "../ui/Button";
import s from "./AutomationsDialog.module.css";
import { zonedTime } from "./ScheduleFields";

/** Saved run identity and owning session remain available after definition deletion. */
export function DetailHistory(props: { store: AutomationsStore }) {
	return (
		<section class={s.history} aria-label={t("automations.history", "Recent runs")}>
			<div class={s.row}>
				<h3>{t("automations.history", "Recent runs")}</h3>
				<Button type="button" disabled={props.store.historyLoading()} onClick={() => void props.store.loadHistory()}>
					{t("automations.refreshHistory", "Refresh history")}
				</Button>
			</div>
			<Show when={props.store.historyLoading()}>
				<p role="status">{t("automations.loadingHistory", "Loading history…")}</p>
			</Show>
			<Show when={!props.store.historyLoading() && props.store.runs().length === 0}>
				<p class={s.hint}>{t("automations.noRuns", "No recent runs.")}</p>
			</Show>
			<ol>
				<For each={props.store.runs()}>
					{(run) => (
						<li>
							<time>{zonedTime(run.created_ms, run.definition.timezone)}</time>
							<strong>{run.status}</strong>
							<span>{run.id}</span>
							<Show when={run.session_id}>
								<Button
									type="button"
									disabled={!terminalsStore.findBySessionId(run.session_id!)}
									onClick={() => {
										const terminal = terminalsStore.findBySessionId(run.session_id!);
										if (terminal) navigateToTerminal(terminal);
									}}
								>
									{t("automations.openSession", "Open owning session")}
								</Button>
							</Show>
							{/* DEFERRED (2026-10-10) — live browser screenshot verification timed out; coordinator must verify this run view before landing. */}
							<Show when={run.stdout.text}>
								<pre>{run.stdout.text}</pre>
								<Show when={run.stdout.truncated}>
									<span>Output truncated</span>
								</Show>
							</Show>
							<span>{run.trigger.kind}</span>
							<Show when={run.reason}>
								<p>{run.reason}</p>
							</Show>
						</li>
					)}
				</For>
			</ol>
		</section>
	);
}
