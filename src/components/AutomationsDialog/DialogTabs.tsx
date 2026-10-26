import { For } from "solid-js";
import { t } from "../../i18n";
import { Button } from "../ui/Button";
import s from "./AutomationsDialog.module.css";

export function DialogTabs(props: { selected: string; onSelect: (tab: string) => void }) {
	const ids = ["definitions", "runs"];
	const keydown = (event: KeyboardEvent) => {
		const current = ids.indexOf(props.selected);
		const next =
			event.key === "Home"
				? 0
				: event.key === "End"
					? 1
					: event.key === "ArrowRight"
						? (current + 1) % 2
						: event.key === "ArrowLeft"
							? (current + 1) % 2
							: -1;
		if (next < 0) return;
		event.preventDefault();
		props.onSelect(ids[next]);
		(event.currentTarget as HTMLElement).querySelector<HTMLButtonElement>(`#automations-tab-${ids[next]}`)?.focus();
	};
	return (
		<div class={s.tabs} role="tablist" aria-label={t("automations.title", "Automations")} onKeyDown={keydown}>
			<For each={ids}>
				{(id) => (
					<Button
						type="button"
						role="tab"
						id={`automations-tab-${id}`}
						aria-controls={`automations-panel-${id}`}
						aria-selected={props.selected === id}
						aria-pressed={props.selected === id}
						tabIndex={props.selected === id ? 0 : -1}
						onClick={() => props.onSelect(id)}
					>
						{id === "definitions" ? t("automations.list", "Automation definitions") : t("automations.runs", "Runs")}
					</Button>
				)}
			</For>
		</div>
	);
}
