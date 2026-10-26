import { createResource, createSignal, For, Show } from "solid-js";
import { invoke } from "../../../invoke";
import { workItemProviderRegistry } from "../../../plugins/workItemProviderRegistry";
import {
	loadWorkItemSetup,
	missingFields,
	saveWorkItemSetup,
	setupFor,
	WORK_ITEM_PLUGINS,
	type WorkItemPluginSetup,
} from "../../../plugins/workItemSetup";
import { appLogger } from "../../../stores/appLogger";
import { pluginStore } from "../../../stores/pluginStore";
import { SettingField } from "../../SettingsPanel/fields/SettingField";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Textarea } from "../../ui/Textarea";
import { InstallWorkItemPlugin } from "./InstallWorkItemPlugin";
import s from "./TasksSetup.module.css";

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

function SetupForm(props: { setup: WorkItemPluginSetup }) {
	const [values, setValues] = createSignal<Record<string, string>>({});
	const [loaded] = createResource(async () => {
		try {
			setValues(await loadWorkItemSetup(props.setup));
		} catch (cause) {
			setError(message(cause));
		}
		return true;
	});
	const [error, setError] = createSignal<string | null>(null);
	const [saved, setSaved] = createSignal(false);
	const [saving, setSaving] = createSignal(false);
	const unconfigured = () => workItemProviderRegistry.list(props.setup.providerId).length === 0;
	async function save(event: SubmitEvent) {
		event.preventDefault();
		setSaved(false);
		const missing = missingFields(props.setup, values());
		if (missing.length) {
			setError(`${missing.join(", ")} ${missing.length > 1 ? "are" : "is"} required.`);
			return;
		}
		setSaving(true);
		setError(null);
		try {
			await saveWorkItemSetup(props.setup, values());
			setSaved(true);
		} catch (cause) {
			appLogger.error("plugin", `Failed to save the ${props.setup.name} setup`, cause);
			setError(message(cause));
		} finally {
			setSaving(false);
		}
	}
	return (
		<form class={s.form} aria-label={`${props.setup.name} setup`} onSubmit={(event) => void save(event)}>
			<h3>{unconfigured() ? `${props.setup.name} needs ${props.setup.missing}.` : `${props.setup.name} settings`}</h3>
			<For each={props.setup.fields}>
				{(field) => (
					<SettingField label={field.label}>
						{(id) =>
							field.boards ? (
								<Textarea
									id={id}
									rows={3}
									placeholder={`${field.placeholder} (one per line)`}
									value={values()[field.key] ?? ""}
									disabled={!loaded() || saving()}
									onInput={(event) => setValues({ ...values(), [field.key]: event.currentTarget.value })}
								/>
							) : (
								<Input
									id={id}
									type="text"
									placeholder={field.placeholder}
									aria-required={field.required}
									value={values()[field.key] ?? ""}
									disabled={!loaded() || saving()}
									onInput={(event) => setValues({ ...values(), [field.key]: event.currentTarget.value })}
								/>
							)
						}
					</SettingField>
				)}
			</For>
			<Show when={error()}>
				{(text) => (
					<p class={s.error} role="alert">
						{text()}
					</p>
				)}
			</Show>
			<Show when={saved()}>
				<p class={s.muted} role="status">
					Saved.
				</p>
			</Show>
			<div class={s.actions}>
				<Button
					type="submit"
					variant="primary"
					disabled={!loaded() || saving()}
					aria-label={`Save ${props.setup.name}`}
				>
					{saving() ? "Saving…" : "Save"}
				</Button>
			</div>
		</form>
	);
}

/** Install prompts and setup forms for the work-item plugins: one plugin when `pluginId` is given, else all. */
export function TasksSetup(props: { pluginId?: string }) {
	const plugins = () => {
		const one = props.pluginId ? setupFor(props.pluginId) : undefined;
		return one ? [one] : WORK_ITEM_PLUGINS;
	};
	const [githubConnected] = createResource(async () => {
		try {
			return Boolean(await invoke<string | null>("get_github_viewer_login"));
		} catch {
			return false;
		}
	});
	return (
		<div class={s.setup}>
			<For each={plugins()}>
				{(setup) => (
					<Show
						when={pluginStore.getPlugin(setup.pluginId)}
						fallback={
							<InstallWorkItemPlugin
								setup={setup}
								note={setup.pluginId === "github-work" && githubConnected() ? "GitHub is connected." : undefined}
							/>
						}
					>
						<SetupForm setup={setup} />
					</Show>
				)}
			</For>
		</div>
	);
}
