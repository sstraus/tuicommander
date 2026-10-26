import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import type { AutomationsStore } from "../../stores/automations";
import { Button } from "../ui/Button";
import { Select } from "../ui/Select";
import s from "./AutomationsDialog.module.css";
import type { AutomationTemplate } from "./contract";

export function TemplatePicker(props: { store: AutomationsStore }) {
	const [templates, setTemplates] = createSignal<AutomationTemplate[]>([]);
	const [selected, setSelected] = createSignal("");
	const [error, setError] = createSignal("");
	const missing = () => {
		const draft = props.store.draft();
		if (!draft?.repository.trim())
			return t("automations.templateNeedsRepository", "Choose a repository before applying a template.");
		if (!draft.run_config.trim())
			return t("automations.templateNeedsRunConfig", "Enter a run config before applying a template.");
		return "";
	};
	let disposed = false;
	onCleanup(() => {
		disposed = true;
	});
	const load = async () => {
		setError("");
		try {
			const values = await props.store.templates();
			if (!disposed) setTemplates(values);
		} catch (cause) {
			if (!disposed) setError(cause instanceof Error ? cause.message : String(cause));
		}
	};
	onMount(() => {
		void load();
	});
	return (
		<fieldset class={s.group} disabled={props.store.busy()}>
			<legend>{t("automations.templates", "Built-in templates")}</legend>
			<Show when={error()}>
				<p role="alert" class={s.error}>
					{error()}
				</p>
				<Button type="button" onClick={() => void load()}>
					{t("automations.refresh", "Refresh")}
				</Button>
			</Show>
			<label>
				{t("automations.template", "Template")}
				<Select value={selected()} onChange={(event) => setSelected(event.currentTarget.value)}>
					<option value="">{t("automations.chooseTemplate", "Choose a template")}</option>
					<For each={templates()}>
						{(item) => <option value={item.id}>{t("automations.templateNames." + item.id, item.name)}</option>}
					</For>
				</Select>
			</label>
			<p class={s.hint}>
				{t(
					"automations.templateHint",
					"Choose a repository and enter a run config above, then apply a template. Review the draft and save it paused; Resume enables scheduled runs.",
				)}
			</p>
			<p role="alert" class={s.reserved}>
				{missing()}
			</p>
			<Button
				type="button"
				disabled={!selected() || !!missing()}
				onClick={() => void props.store.applyTemplate(selected())}
			>
				{t("automations.applyTemplate", "Apply template")}
			</Button>
		</fieldset>
	);
}
