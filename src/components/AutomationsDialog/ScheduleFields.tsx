import { createSignal, For, Show } from "solid-js";
import { t } from "../../i18n";
import type { AutomationsStore } from "../../stores/automations";
import s from "./AutomationsDialog.module.css";
import type { SchedulePreset } from "./contract";

/** Date formatting only; the backend supplies every occurrence. */
export function zonedTime(value: number | string, timezone: string): string {
	try {
		return new Intl.DateTimeFormat(undefined, { timeZone: timezone, dateStyle: "medium", timeStyle: "short" }).format(
			new Date(value),
		);
	} catch {
		return String(value);
	}
}
export function ScheduleFields(props: { store: AutomationsStore }) {
	const [cadence, setCadence] = createSignal(props.store.draft()?.once_local != null ? "once" : "custom");
	const [hour, setHour] = createSignal(9),
		[minute, setMinute] = createSignal(0),
		[weekday, setWeekday] = createSignal(1);
	const isCompleted = () => {
		const preview = props.store.schedulePreview();
		return preview !== undefined && "completed" in preview && preview.completed;
	};
	const apply = () => {
		const kind = cadence();
		if (kind === "custom" || kind === "once") return;
		const preset: SchedulePreset =
			kind === "hourly"
				? { kind, minute: minute() }
				: kind === "weekly"
					? { kind, hour: hour(), minute: minute(), weekday: weekday() }
					: { kind: kind === "daily" ? "daily" : "weekdays", hour: hour(), minute: minute() };
		void props.store.applyPreset(preset);
	};
	return (
		<fieldset class={s.group} disabled={props.store.busy()}>
			<legend>{t("automations.schedule", "Schedule")}</legend>
			<div class={s.row}>
				<label>
					{t("automations.cadence", "Cadence")}
					<select
						value={cadence()}
						onChange={(event) => {
							const value = event.currentTarget.value;
							setCadence(value);
							if (value === "once") props.store.edit({ cron: "", once_local: "" });
							else if (props.store.draft()?.once_local != null) props.store.edit({ once_local: null });
						}}
					>
						<option value="once">{t("automations.once", "Once")}</option>
						<option value="custom">{t("automations.custom", "Custom cron")}</option>
						<option value="hourly">{t("automations.hourly", "Hourly")}</option>
						<option value="daily">{t("automations.daily", "Daily")}</option>
						<option value="weekdays">{t("automations.weekdays", "Weekdays")}</option>
						<option value="weekly">{t("automations.weekly", "Weekly")}</option>
					</select>
				</label>
				<Show when={cadence() !== "custom" && cadence() !== "once"}>
					<Show when={cadence() !== "hourly"}>
						<label>
							{t("automations.hour", "Hour")}
							<input
								type="number"
								min="0"
								max="23"
								value={hour()}
								onInput={(event) => setHour(event.currentTarget.valueAsNumber)}
							/>
						</label>
					</Show>
					<label>
						{t("automations.minute", "Minute")}
						<input
							type="number"
							min="0"
							max="59"
							value={minute()}
							onInput={(event) => setMinute(event.currentTarget.valueAsNumber)}
						/>
					</label>
					<Show when={cadence() === "weekly"}>
						<label>
							{t("automations.weekday", "Weekday (Sunday = 0)")}
							<input
								type="number"
								min="0"
								max="6"
								value={weekday()}
								onInput={(event) => setWeekday(event.currentTarget.valueAsNumber)}
							/>
						</label>
					</Show>
					<button type="button" onClick={apply}>
						{t("automations.applyCadence", "Apply cadence")}
					</button>
				</Show>
			</div>
			<div class={s.row}>
				<Show
					when={cadence() === "once"}
					fallback={
						<label>
							{t("automations.cron", "Cron")}
							<input
								value={props.store.draft()?.cron ?? ""}
								onInput={(event) => props.store.edit({ cron: event.currentTarget.value })}
							/>
						</label>
					}
				>
					<label>
						{t("automations.onceTime", "Once local time")}
						<input
							type="datetime-local"
							step="1"
							value={props.store.draft()?.once_local ?? ""}
							onInput={(event) => {
								const value = event.currentTarget.value;
								// datetime-local omits zero seconds; chrono's wire format includes them.
								props.store.edit({ cron: "", once_local: value.length === 16 ? `${value}:00` : value });
							}}
						/>
					</label>
				</Show>

				<label>
					{t("automations.timezone", "Timezone")}
					<input
						placeholder={t("automations.localZone", "Local zone from backend")}
						value={props.store.draft()?.timezone ?? ""}
						onInput={(event) => props.store.edit({ timezone: event.currentTarget.value })}
					/>
				</label>
				<button type="button" disabled={props.store.previewLoading()} onClick={() => void props.store.preview()}>
					{t("automations.preview", "Preview schedule")}
				</button>
			</div>
			<Show when={props.store.previewError()}>
				<p role="alert" class={s.error}>
					{props.store.previewError()}
				</p>
			</Show>
			<Show when={isCompleted()}>
				<p class={s.hint}>{t("automations.onceCompleted", "This one-time occurrence has completed.")}</p>
			</Show>
			<Show when={props.store.schedulePreview()}>
				{(preview) => (
					<div class={s.preview} aria-live="polite">
						<strong>{preview().timezone}</strong>
						<For each={preview().occurrences}>
							{(instant) => <time dateTime={instant}>{zonedTime(instant, preview().timezone)}</time>}
						</For>
					</div>
				)}
			</Show>
		</fieldset>
	);
}
