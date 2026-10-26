import { type Component, createEffect, createSignal, Show, untrack } from "solid-js";
import type { SettingId } from "../../../services/settingsCatalogue";
import { reportSettingsConflicts } from "../../../services/settingsRefresh";
import { Button } from "../../ui/Button";
import { Textarea } from "../../ui/Textarea";
import s from "../Settings.module.css";
import a from "./AgentsTab.module.css";

/** The marker tokens the TUIC parser needs that `text` no longer contains. */
export function missingMarkers(text: string, required: readonly string[]): string[] {
	return required.filter((marker) => !text.includes(marker));
}

/**
 * Edits user additions to launch instructions that TUIC passes to an agent at launch. `value`
 * undefined means "no override": show a read-only effective preview until Add instructions.
 * Saves on blur, so a half-typed edit never reaches a launch.
 */
export const LaunchInstructionsEditor: Component<{
	id: string;
	settingId: SettingId;
	label: string;
	value: string | undefined;
	fallback: string | undefined;
	inheritanceLabel: string;
	customizeLabel: string;
	resetLabel: string;
	requiredMarkers: readonly string[];
	onSave: (text: string | undefined) => void;
}> = (props) => {
	const [draft, setDraft] = createSignal(props.value ?? "");
	let acknowledged = props.value ?? "";
	createEffect(() => {
		const incoming = props.value ?? "";
		const current = untrack(draft);
		if (current === acknowledged) setDraft(incoming);
		else if (incoming !== acknowledged && incoming !== current) reportSettingsConflicts(["/"]);
		acknowledged = incoming;
	});
	const save = () => {
		if (draft() !== (props.value ?? "")) props.onSave(draft());
	};
	return (
		<div class={a.expandedSection}>
			<label class={a.expandedLabel} for={props.id}>
				{props.label}
			</label>
			<Show
				when={props.value !== undefined}
				fallback={
					<>
						<p class={s.hint}>{props.inheritanceLabel}</p>
						<pre id={props.id} class={a.instructionsPreview} aria-label={props.label}>
							{props.fallback ?? "Loading preview…"}
						</pre>
						<Button
							type="button"
							data-setting-id={props.settingId}
							disabled={props.fallback === undefined}
							onClick={() => props.onSave("")}
						>
							{props.customizeLabel}
						</Button>
					</>
				}
			>
				<p class={s.hint}>User additions only — enabled modules and global additions still apply.</p>
				<Textarea
					data-setting-id={props.settingId}
					id={props.id}
					class={`${a.formInput} ${a.mono}`}
					rows={10}
					value={draft()}
					onInput={(e) => setDraft(e.currentTarget.value)}
					onBlur={save}
					onClick={(e) => e.stopPropagation()}
				/>
			</Show>
			<Show when={props.value !== undefined}>
				<Button
					data-setting-id={props.settingId}
					type="button"
					onClick={() => {
						setDraft("");
						props.onSave(undefined);
					}}
				>
					{props.resetLabel}
				</Button>
			</Show>
			<Show when={props.value !== undefined}>
				<pre class={a.instructionsPreview} aria-label={`${props.label} effective preview`}>
					{props.fallback ?? "Loading preview…"}
				</pre>
			</Show>
			<p class={s.hint}>
				Appended after generated modules at every launch and resume. Empty adds nothing. <code>{"{VER}"}</code> becomes
				the TUIC version.
			</p>
		</div>
	);
};
