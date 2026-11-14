import { createSignal, For, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import type { AutomationsStore } from "../../stores/automations";
import { repositoriesStore } from "../../stores/repositories";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Textarea } from "../ui/Textarea";
import s from "./AutomationsDialog.module.css";
import { ScheduleFields } from "./ScheduleFields";
import { TemplatePicker } from "./TemplatePicker";

export function DefinitionEditor(props: { store: AutomationsStore; focusProfile?: boolean }) {
	const store = props.store;
	const [profiles, setProfiles] = createSignal<string[]>([]);
	const [profileError, setProfileError] = createSignal("");
	let picker: HTMLSelectElement | undefined;
	onMount(() => {
		if (props.focusProfile) picker?.focus();
		void store
			.profiles()
			.then(setProfiles)
			.catch((cause) => setProfileError(String(cause)));
	});
	const knownRepos = () => repositoriesStore.getOrderedRepos();
	return (
		<Show when={store.draft()}>
			{(draft) => (
				<div class={s.editor}>
					<fieldset class={s.group} disabled={store.busy()}>
						<legend>{t("automations.definition", "Definition")}</legend>
						<label>
							{t("automations.name", "Name")}
							<Input value={draft().name} onInput={(event) => store.edit({ name: event.currentTarget.value })} />
						</label>
						<div class={s.row}>
							<label>
								{t("automations.repository", "Repository")}
								<Select
									value={draft().repository}
									onChange={(event) => store.edit({ repository: event.currentTarget.value })}
								>
									<option value="">{t("automations.chooseRepository", "Choose a repository")}</option>
									<Show when={draft().repository && !knownRepos().some((repo) => repo.path === draft().repository)}>
										<option value={draft().repository}>{draft().repository}</option>
									</Show>
									<For each={knownRepos()}>{(repo) => <option value={repo.path}>{repo.displayName}</option>}</For>
								</Select>
							</label>
							<label>
								{t("automations.runConfig", "Run config")}
								<Select
									ref={picker}
									value={draft().run_config}
									onChange={(event) => store.edit({ run_config: event.currentTarget.value })}
								>
									<option value="">{t("automations.chooseProfile", "Choose a saved profile")}</option>
									{/* Keep the selected option mounted when the asynchronous catalogue arrives. */}
									<Show when={draft().run_config}>
										<option value={draft().run_config}>{draft().run_config}</option>
									</Show>
									<For each={profiles().filter((name) => name !== draft().run_config)}>
										{(name) => <option value={name}>{name}</option>}
									</For>
								</Select>
							</label>
						</div>
						<Show when={profileError()}>
							<p role="alert">{profileError()}</p>
						</Show>
						<p class={s.hint}>
							{t(
								"automations.permissions",
								"Uses this machine's configured agent permissions. Prompts are literal text.",
							)}
						</p>
						<label>
							{t("automations.prompt", "Prompt")}
							<Textarea
								rows="5"
								value={draft().prompt}
								onInput={(event) => store.edit({ prompt: event.currentTarget.value })}
							/>
						</label>
					</fieldset>
					<Show when={store.isNew()}>
						<TemplatePicker store={store} />
					</Show>
					<ScheduleFields store={store} />
					<fieldset class={s.group} disabled={store.busy()}>
						<legend>{t("automations.workspace", "Workspace and limits")}</legend>
						<label>
							{t("automations.workspaceMode", "Workspace")}
							<Select
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
							</Select>
						</label>
						<Show when={draft().workspace.mode === "new_per_run"}>
							<label>
								{t("automations.baseBranch", "Base branch")}
								<Input
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
								<Input
									type="number"
									min="1"
									value={draft().max_duration_secs}
									onInput={(event) => store.edit({ max_duration_secs: event.currentTarget.valueAsNumber })}
								/>
							</label>
							<label>
								{t("automations.grace", "Catch-up grace (seconds)")}
								<Input
									type="number"
									min="1"
									value={draft().grace_secs}
									onInput={(event) => store.edit({ grace_secs: event.currentTarget.valueAsNumber })}
								/>
							</label>
						</div>
						<label class={s.check}>
							<input
								type="checkbox"
								checked={draft().close_on_finish ?? false}
								onChange={(event) => store.edit({ close_on_finish: event.currentTarget.checked })}
							/>
							{t("automations.closeOnFinish", "Close the run tab after confirmed completion")}
						</label>
						<p class={s.hint}>
							{t(
								"automations.closeOnFinishHint",
								"Off by default. Failed, unknown and input-blocked runs stay open. Worktrees and other tabs are not removed.",
							)}
						</p>
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
									store.edit({
										precheck: event.currentTarget.checked
											? { command: "", timeout_secs: 30, inject_output: false }
											: null,
									})
								}
							/>
							{t("automations.enablePrecheck", "Run a shell precheck before scheduled runs")}
						</label>
						<Show when={draft().precheck}>
							{(precheck) => (
								<>
									<div class={s.row}>
										<label>
											{t("automations.precheckCommand", "Precheck command")}
											<Input
												value={precheck().command}
												onInput={(event) =>
													store.edit({ precheck: { ...precheck(), command: event.currentTarget.value } })
												}
											/>
										</label>
										<label>
											{t("automations.precheckTimeout", "Precheck timeout (seconds)")}
											<Input
												type="number"
												min="1"
												value={precheck().timeout_secs}
												onInput={(event) =>
													store.edit({ precheck: { ...precheck(), timeout_secs: event.currentTarget.valueAsNumber } })
												}
											/>
										</label>
									</div>
									<label class={s.check}>
										<input
											type="checkbox"
											checked={precheck().inject_output ?? false}
											onChange={(event) =>
												store.edit({ precheck: { ...precheck(), inject_output: event.currentTarget.checked } })
											}
										/>
										{t("automations.injectPrecheckOutput", "Include precheck stdout in the agent prompt")}
									</label>
									<p class={s.hint}>
										{t(
											"automations.precheckContextHint",
											"Off by default. Adds up to 16 KiB of escaped stdout as delimited data before your unchanged prompt. Truncation is marked; stderr is excluded. Run now bypasses precheck.",
										)}
									</p>
								</>
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
