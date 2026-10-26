import { Show } from "solid-js";
import { t } from "../../i18n";
import { pathBasename } from "../../utils/pathUtils";
import s from "./AutomationsDialog.module.css";
import type { AutomationListItem } from "./contract";
import { zonedTime } from "./ScheduleFields";
import { scheduleLabel } from "./scheduleLabel";

export function AutomationCard(props: {
	item: AutomationListItem;
	selected: boolean;
	disabled: boolean;
	onSelect: () => void;
}) {
	const definition = () => props.item.definition;
	return (
		<button
			data-control-layout="card"
			type="button"
			disabled={props.disabled}
			class={s.item}
			aria-pressed={props.selected}
			onClick={props.onSelect}
		>
			<span class={s.itemTitle}>
				<span>{definition().name}</span>
				<small class={s.badge} data-enabled={definition().enabled}>
					{definition().enabled ? t("automations.active", "Active") : t("automations.paused", "Paused")}
				</small>
			</span>
			<span class={s.repository} title={definition().repository}>
				{pathBasename(definition().repository.replace(/[\\/]+$/, "")) || definition().repository}
			</span>
			<span class={s.schedule}>
				{definition().once_local != null
					? `${t("automations.once", "Once")} ${definition().once_local}`
					: scheduleLabel(definition().cron)}
			</span>
			<Show when={definition().once_local == null && scheduleLabel(definition().cron) !== definition().cron}>
				<code class={s.cron}>{definition().cron}</code>
			</Show>
			<Show when={props.item.next_run_ms !== null}>
				<time dateTime={new Date(props.item.next_run_ms as number).toISOString()}>
					{t("automations.nextRunAt", "Next: {time}", {
						time: zonedTime(props.item.next_run_ms as number, definition().timezone),
					})}
				</time>
			</Show>
			<span>{definition().timezone}</span>
			<Show when={props.item.last_status}>
				<strong>{props.item.last_status}</strong>
			</Show>
		</button>
	);
}
