import { Show } from "solid-js";
import { t } from "../../i18n";
import type { AutomationsStore } from "../../stores/automations";
import s from "./AutomationsDialog.module.css";
import { ScheduleFields } from "./ScheduleFields";

export function DefinitionEditor(props: { store: AutomationsStore }) {
	const store = props.store;
	return (
		<Show when={store.draft()}>
			{(draft) => (
				<div class={s.editor}>
					<fieldset class={s.group} disabled={store.busy()}>
						<legend>{t("automations.definition", "Definition")}</legend>
						<label>
							{t("automations.name", "Name")}
							<input value={draft().name} onInput={(event) => store.edit({ name: event.currentTarget.value })} />
						</label>
						<div class={s.row}>
							<label>
								{t("automations.repository", "Repository")}
								<input
									value={draft().repository}
									onInput={(event) => store.edit({ repository: event.currentTarget.value })}
								/>
							</label>
							<label>
								{t("automations.runConfig", "Run config")}
								<input
									value={draft().run_config}
									onInput={(event) => store.edit({ run_config: event.currentTarget.value })}
								/>
							</label>
						</div>
						<p class={s.hint}>
							{t(
								"automations.permissions",
								"Uses this machine's configured agent permissions. Prompts are literal text.",
							)}
						</p>
						<label>
							{t("automations.prompt", "Prompt")}
							<textarea
								rows="5"
								value={draft().prompt}
								onInput={(event) => store.edit({ prompt: event.currentTarget.value })}
							/>
						</label>
					</fieldset>
					<ScheduleFields store={store} />
					<fieldset class={s.group} disabled={store.busy()}>
						<legend>{t("automations.workspace", "Workspace and limits")}</legend>
						<label>
							{t("automations.workspaceMode", "Workspace")}
							<select
								value={draft().workspace.mode}
								onChange={(event) =>
									store.edit({
										workspace:
											event.currentTarget.value === "existing"
												? { mode: "existing" }
												: { mode: "new_per_run", base_branch: "" },
									})
								}
							>
								<option value="existing">{t("automations.existing", "Existing checkout")}</option>
								<option value="new_per_run">{t("automations.worktree", "New worktree per run")}</option>
							</select>
						</label>
						<Show when={draft().workspace.mode === "new_per_run"}>
							<label>
								{t("automations.baseBranch", "Base branch")}
								<input
									value={
										draft().workspace.mode === "new_per_run"
											? (draft().workspace as { base_branch: string }).base_branch
											: ""
									}
									onInput={(event) =>
										store.edit({ workspace: { mode: "new_per_run", base_branch: event.currentTarget.value } })
									}
								/>
							</label>
						</Show>
						<div class={s.row}>
							<label>
								{t("automations.maxDuration", "Maximum duration (seconds)")}
								<input
									type="number"
									min="1"
									value={draft().max_duration_secs}
									onInput={(event) => store.edit({ max_duration_secs: event.currentTarget.valueAsNumber })}
								/>
							</label>
							<label>
								{t("automations.grace", "Catch-up grace (seconds)")}
								<input
									type="number"
									min="1"
									value={draft().grace_secs}
									onInput={(event) => store.edit({ grace_secs: event.currentTarget.valueAsNumber })}
								/>
							</label>
						</div>
						<p class={s.hint}>
							{t(
								"automations.overlap",
								"Skip while a run is still open. Run now bypasses precheck, obeys capacity, and also works when paused.",
							)}
						</p>
					</fieldset>
					<fieldset class={s.group} disabled={store.busy()}>
						<legend>{t("automations.precheck", "Precheck")}</legend>
						<label class={s.check}>
							<input
								type="checkbox"
								checked={draft().precheck !== null}
								onChange={(event) =>
									store.edit({ precheck: event.currentTarget.checked ? { command: "", timeout_secs: 30 } : null })
								}
							/>
							{t("automations.enablePrecheck", "Run a shell precheck before scheduled runs")}
						</label>
						<Show when={draft().precheck}>
							{(precheck) => (
								<div class={s.row}>
									<label>
										{t("automations.precheckCommand", "Precheck command")}
										<input
											value={precheck().command}
											onInput={(event) =>
												store.edit({ precheck: { ...precheck(), command: event.currentTarget.value } })
											}
										/>
									</label>
									<label>
										{t("automations.precheckTimeout", "Precheck timeout (seconds)")}
										<input
											type="number"
											min="1"
											value={precheck().timeout_secs}
											onInput={(event) =>
												store.edit({ precheck: { ...precheck(), timeout_secs: event.currentTarget.valueAsNumber } })
											}
										/>
									</label>
								</div>
							)}
						</Show>
						<p class={s.hint}>
							{t(
								"automations.precheckHint",
								"Exit 0 runs the agent; other exits skip quietly. Precheck output is saved in history.",
							)}
						</p>
					</fieldset>
				</div>
			)}
		</Show>
	);
}
