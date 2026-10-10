import { For, Show } from "solid-js";
import { t } from "../../i18n";
import type { AutomationsStore } from "../../stores/automations";
import s from "./AutomationsDialog.module.css";
import { zonedTime } from "./ScheduleFields";

/** Recent history only. Aggregates and output/session navigation belong to Step 10. */
export function DetailHistory(props: { store: AutomationsStore }) {
	return (
		<section class={s.history} aria-label={t("automations.history", "Recent runs")}>
			<div class={s.row}>
				<h3>{t("automations.history", "Recent runs")}</h3>
				<button type="button" disabled={props.store.historyLoading()} onClick={() => void props.store.loadHistory()}>
					{t("automations.refreshHistory", "Refresh history")}
				</button>
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
