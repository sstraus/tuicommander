import { createSignal, onMount, Show } from "solid-js";
import type { WorkItemPluginSetup } from "../../../plugins/workItemSetup";
import { appLogger } from "../../../stores/appLogger";
import { pluginStore } from "../../../stores/pluginStore";
import { registryStore } from "../../../stores/registryStore";
import { Button } from "../../ui/Button";
import s from "./TasksSetup.module.css";

/** Install button for one work-item plugin, fed by the plugin registry. */
export function InstallWorkItemPlugin(props: { setup: WorkItemPluginSetup; note?: string; compact?: boolean }) {
	const [installing, setInstalling] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	onMount(() => void registryStore.fetch());
	const entry = () => registryStore.state.entries.find((candidate) => candidate.id === props.setup.pluginId);
	async function install() {
		const found = entry();
		if (!found || installing()) return;
		setInstalling(true);
		setError(null);
		try {
			await pluginStore.installFromUrl(found.downloadUrl);
		} catch (cause) {
			appLogger.error("plugin", `Failed to install plugin "${props.setup.pluginId}"`, cause);
			setError(cause instanceof Error ? cause.message : String(cause));
		} finally {
			setInstalling(false);
		}
	}
	return (
		<div class={s.install}>
			<Show when={!props.compact && props.note}>
				<span>{props.note}</span>
			</Show>
			<Button type="button" disabled={!entry() || installing()} onClick={() => void install()}>
				{installing() ? "Installing…" : `Install ${props.setup.name}`}
			</Button>
			<Show when={!entry() && !registryStore.state.loading}>
				<span class={s.muted}>Not listed in the plugin registry.</span>
			</Show>
			<Show when={error()}>
				{(message) => (
					<span class={s.error} role="alert">
						Installation failed: {message()}
					</span>
				)}
			</Show>
		</div>
	);
}
