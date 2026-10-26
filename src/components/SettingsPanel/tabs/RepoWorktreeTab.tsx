import { type Component, For, Show } from "solid-js";
import { t } from "../../../i18n";
import { isMacOS } from "../../../platform";
import type { SettingId } from "../../../services/settingsCatalogue";
import { settingOptions } from "../../../services/settingsCatalogue";
import type {
	AutoDeleteOnPrClose,
	MergeStrategy,
	OrphanCleanup,
	RepoDefaults,
	WorktreeAfterMerge,
	WorktreeStorage,
} from "../../../stores/repoDefaults";
import type { RepoSettings } from "../../../stores/repoSettings";
import { settingsStore } from "../../../stores/settings";
import { ColorSwatchPicker } from "../../shared/ColorSwatchPicker";
import { DEFAULT_COLOR_PRESETS } from "../../shared/colorPresets";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { Switch } from "../../ui/Switch";
import { ContextualHelp } from "../ContextualHelp";
import s from "../Settings.module.css";
import { SectionCard, SettingPageHeader, SettingRow } from "../SettingsPrimitives";
import r from "./RepositoryTabs.module.css";

export interface RepoTabProps {
	settings: RepoSettings;
	defaults: RepoDefaults;
	onUpdate: <K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => void;
}

/** Nullable controls retain an explicit inherited state instead of flattening it. */
const INHERIT = "__inherit__";
function BooleanOverride(props: {
	settingId?: SettingId;
	"data-setting-exclusion"?: string;
	label: string;
	value: boolean | null;
	fallback: boolean;
	onChange: (value: boolean | null) => void;
}) {
	return (
		<SettingRow label={props.label}>
			{(id, descriptionId) => (
				<Select
					data-setting-id={props.settingId}
					data-setting-exclusion={props["data-setting-exclusion"]}
					id={id}
					aria-describedby={descriptionId()}
					value={props.value === null ? INHERIT : String(props.value)}
					onChange={(e) => props.onChange(e.currentTarget.value === INHERIT ? null : e.currentTarget.value === "true")}
				>
					<option value={INHERIT}>Use global default ({props.fallback ? "On" : "Off"})</option>
					<option value="true">On</option>
					<option value="false">Off</option>
				</Select>
			)}
		</SettingRow>
	);
}

export const RepoWorktreeTab: Component<RepoTabProps> = (props) => {
	const branchOptions = [
		{ value: "automatic", label: t("repoWorktree.baseBranch.automatic", "Automatic") },
		{ value: "main", label: "main" },
		{ value: "master", label: "master" },
		{ value: "develop", label: "develop" },
	];

	const baseBranchValue = () => props.settings.baseBranch ?? INHERIT;

	const handleBaseBranchChange = (value: string) => {
		props.onUpdate("baseBranch", value === INHERIT ? null : value);
	};

	return (
		<div class={`${s.section} ${r.page}`}>
			<SettingPageHeader
				title={props.settings.displayName || "Repository"}
				scope="Repository"
				source={props.settings.path}
				breadcrumb="Repositories / Settings"
			/>
			<SectionCard title={t("repoWorktree.heading.repository", "Repository")}>
				<div class={r.group}>
					<label for="repo-field-1">{t("repoWorktree.label.displayName", "Display Name")}</label>
					<Input
						data-setting-id="repositories.repos.{key}.display_name"
						id="repo-field-1"
						type="text"
						value={props.settings.displayName ?? ""}
						onInput={(e) => props.onUpdate("displayName", e.currentTarget.value)}
						placeholder={t("repoWorktree.placeholder.displayName", "Custom name...")}
					/>
					<p class={s.hint}>{t("repoWorktree.hint.displayName", "Shown in sidebar instead of folder name")}</p>
				</div>

				<SettingRow
					label={t("repoWorktree.label.sidebarColor", "Sidebar Color")}
					hint={t("repoWorktree.hint.sidebarColor", "Color-code this repo in the sidebar")}
				>
					{() => (
						<ColorSwatchPicker
							settingId="repositories.repos.{key}.color"
							color={props.settings.color ?? ""}
							presets={DEFAULT_COLOR_PRESETS}
							onChange={(c) => props.onUpdate("color", c)}
						/>
					)}
				</SettingRow>

				<div class={r.group}>
					<div class={s.helpLabel}>
						<label for="repo-field-2">{t("repoWorktree.label.autoFetchInterval", "Auto-Fetch Interval")}</label>
						<ContextualHelp label={t("repoWorktree.label.autoFetchInterval", "Auto-Fetch Interval")} />
					</div>
					<Select
						data-setting-id="repositories.repos.{key}.auto_fetch_interval_minutes"
						id="repo-field-2"
						value={
							props.settings.autoFetchIntervalMinutes != null
								? String(props.settings.autoFetchIntervalMinutes)
								: INHERIT
						}
						onChange={(e) => {
							const v = e.currentTarget.value;
							props.onUpdate("autoFetchIntervalMinutes", v === INHERIT ? null : Number(v));
						}}
					>
						<option value={INHERIT}>
							{t("repoWorktree.autoFetch.useDefault", "Use global default ({default})", {
								default:
									props.defaults.autoFetchIntervalMinutes === 0
										? t("repoWorktree.autoFetch.disabled", "Disabled")
										: `${props.defaults.autoFetchIntervalMinutes} min`,
							})}
						</option>
						<option value="0">{t("repoWorktree.autoFetch.disabled", "Disabled")}</option>
						<option value="5">5 min</option>
						<option value="15">15 min</option>
						<option value="30">30 min</option>
						<option value="60">60 min</option>
					</Select>
					<p class={s.hint}>
						{t("repoWorktree.hint.autoFetch", "Periodically fetch from remote to keep branch stats fresh")}
					</p>
				</div>
			</SectionCard>
			<SectionCard
				title={t("repoWorktree.heading.worktreeConfiguration", "Worktree Configuration")}
				description="Use global default keeps the value inherited. Choosing an option creates a repository override."
			>
				<div class={r.group}>
					<label for="repo-field-3">{t("repoWorktree.label.branchFrom", "Branch From")}</label>
					<Select
						data-setting-id="repositories.repos.{key}.base_branch"
						id="repo-field-3"
						value={baseBranchValue()}
						onChange={(e) => handleBaseBranchChange(e.currentTarget.value)}
					>
						<option value={INHERIT}>
							{t("repoWorktree.baseBranch.useGlobalDefault", "Use global default ({default})", {
								default: props.defaults.baseBranch,
							})}
						</option>
						<For each={branchOptions}>{(opt) => <option value={opt.value}>{opt.label}</option>}</For>
					</Select>
					<p class={s.hint}>{t("repoWorktree.hint.branchFrom", "Base branch for new worktrees")}</p>
				</div>

				<div class={r.group}>
					<div class={s.helpLabel}>
						<label>{t("repoWorktree.label.fileHandling", "File Handling")}</label>
						<ContextualHelp label={t("repoWorktree.label.fileHandling", "File Handling")} />
					</div>

					<BooleanOverride
						settingId="repositories.repos.{key}.copy_ignored_files"
						label="Copy ignored files"
						value={props.settings.copyIgnoredFiles}
						fallback={props.defaults.copyIgnoredFiles}
						onChange={(v) => props.onUpdate("copyIgnoredFiles", v)}
					/>

					<BooleanOverride
						settingId="repositories.repos.{key}.copy_untracked_files"
						label="Copy untracked files"
						value={props.settings.copyUntrackedFiles}
						fallback={props.defaults.copyUntrackedFiles}
						onChange={(v) => props.onUpdate("copyUntrackedFiles", v)}
					/>
				</div>
			</SectionCard>
			<SectionCard title={t("repoWorktree.heading.worktreeSettings", "Worktree Settings")}>
				<div class={r.group}>
					<div class={s.helpLabel}>
						<label for="repo-field-4">{t("repoWorktree.label.worktreeStorage", "Storage Strategy")}</label>
						<ContextualHelp label={t("repoWorktree.label.worktreeStorage", "Storage Strategy")} />
					</div>
					<Select
						data-setting-id="repositories.repos.{key}.worktree_storage"
						id="repo-field-4"
						value={props.settings.worktreeStorage ?? INHERIT}
						onChange={(e) =>
							props.onUpdate(
								"worktreeStorage",
								e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as WorktreeStorage),
							)
						}
					>
						<option value={INHERIT}>
							{t("repoWorktree.worktreeStorage.useDefault", "Use global default ({default})", {
								default: props.defaults.worktreeStorage,
							})}
						</option>
						<For
							each={settingOptions("repositories.repos.{key}.worktree_storage", {
								sibling: t("repoWorktree.worktreeStorage.sibling", "Sibling directory (__wt)"),
								"app-dir": t("repoWorktree.worktreeStorage.appDir", "App config directory"),
								"inside-repo": t("repoWorktree.worktreeStorage.insideRepo", "Inside repository (.worktrees)"),
								"claude-code-default": t(
									"repoWorktree.worktreeStorage.claudeCodeDefault",
									"Claude Code default (.claude/worktrees)",
								),
							}).filter((option) => option.value !== "")}
						>
							{(option) => <option value={option.value}>{option.label}</option>}
						</For>
					</Select>
				</div>

				<div class={r.switchGroup}>
					<SettingRow label="Show all worktrees of this repo in one consolidated screen">
						{(id, descriptionId) => (
							<Switch
								data-setting-id="repositories.repos.{key}.auto_consolidate_worktrees"
								id={id}
								aria-describedby={descriptionId()}
								class={r.rowSwitch}
								role="switch"
								checked={props.settings.autoConsolidateWorktrees}
								onChange={(e) => props.onUpdate("autoConsolidateWorktrees", e.currentTarget.checked)}
							/>
						)}
					</SettingRow>
				</div>

				<div class={r.group}>
					<BooleanOverride
						settingId="repositories.repos.{key}.prompt_on_create"
						label="Prompt for branch name during creation"
						value={props.settings.promptOnCreate}
						fallback={props.defaults.promptOnCreate}
						onChange={(v) => props.onUpdate("promptOnCreate", v)}
					/>
				</div>

				<div class={r.group}>
					<BooleanOverride
						settingId="repositories.repos.{key}.delete_branch_on_remove"
						label="Delete local branch when removing worktree"
						value={props.settings.deleteBranchOnRemove}
						fallback={props.defaults.deleteBranchOnRemove}
						onChange={(v) => props.onUpdate("deleteBranchOnRemove", v)}
					/>
				</div>

				<div class={r.group}>
					<BooleanOverride
						settingId="repositories.repos.{key}.auto_archive_merged"
						label="Auto-archive merged worktrees"
						value={props.settings.autoArchiveMerged}
						fallback={props.defaults.autoArchiveMerged}
						onChange={(v) => props.onUpdate("autoArchiveMerged", v)}
					/>
				</div>

				<div class={r.group}>
					<div class={s.helpLabel}>
						<label for="repo-field-5">{t("repoWorktree.label.orphanCleanup", "Orphan Worktree Cleanup")}</label>
						<ContextualHelp label={t("repoWorktree.label.orphanCleanup", "Orphan Worktree Cleanup")} />
					</div>
					<Select
						data-setting-id="repositories.repos.{key}.orphan_cleanup"
						id="repo-field-5"
						value={props.settings.orphanCleanup ?? INHERIT}
						onChange={(e) =>
							props.onUpdate(
								"orphanCleanup",
								e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as OrphanCleanup),
							)
						}
					>
						<option value={INHERIT}>
							{t("repoWorktree.orphanCleanup.useDefault", "Use global default ({default})", {
								default: props.defaults.orphanCleanup,
							})}
						</option>
						<For
							each={settingOptions("repositories.repos.{key}.orphan_cleanup", {
								ask: t("repoWorktree.orphanCleanup.ask", "Ask before removing"),
								on: t("repoWorktree.orphanCleanup.on", "Auto-remove"),
								off: t("repoWorktree.orphanCleanup.off", "Keep (mark as detached)"),
							}).filter((option) => option.value !== "")}
						>
							{(option) => <option value={option.value}>{option.label}</option>}
						</For>
					</Select>
				</div>

				<div class={r.group}>
					<div class={s.helpLabel}>
						<label for="repo-field-6">{t("repoWorktree.label.prMergeStrategy", "PR Merge Strategy")}</label>
						<ContextualHelp label={t("repoWorktree.label.prMergeStrategy", "PR Merge Strategy")} />
					</div>
					<Select
						data-setting-id="repositories.repos.{key}.pr_merge_strategy"
						id="repo-field-6"
						value={props.settings.prMergeStrategy ?? INHERIT}
						onChange={(e) =>
							props.onUpdate(
								"prMergeStrategy",
								e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as MergeStrategy),
							)
						}
					>
						<option value={INHERIT}>
							{t("repoWorktree.mergeStrategy.useDefault", "Use global default ({default})", {
								default: props.defaults.prMergeStrategy,
							})}
						</option>
						<For
							each={settingOptions("repositories.repos.{key}.pr_merge_strategy", {
								merge: t("repoWorktree.mergeStrategy.merge", "Merge"),
								squash: t("repoWorktree.mergeStrategy.squash", "Squash"),
								rebase: t("repoWorktree.mergeStrategy.rebase", "Rebase"),
							}).filter((option) => option.value !== "")}
						>
							{(option) => <option value={option.value}>{option.label}</option>}
						</For>
					</Select>
				</div>

				<div class={r.group}>
					<div class={s.helpLabel}>
						<label for="repo-field-7">{t("repoWorktree.label.afterMerge", "After Merge Behavior")}</label>
						<ContextualHelp label={t("repoWorktree.label.afterMerge", "After Merge Behavior")} />
					</div>
					<Select
						data-setting-id="repositories.repos.{key}.after_merge"
						id="repo-field-7"
						value={props.settings.afterMerge ?? INHERIT}
						onChange={(e) =>
							props.onUpdate(
								"afterMerge",
								e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as WorktreeAfterMerge),
							)
						}
					>
						<option value={INHERIT}>
							{t("repoWorktree.afterMerge.useDefault", "Use global default ({default})", {
								default: props.defaults.afterMerge,
							})}
						</option>
						<For
							each={settingOptions("repositories.repos.{key}.after_merge", {
								archive: t("repoWorktree.afterMerge.archive", "Archive worktree"),
								delete: t("repoWorktree.afterMerge.delete", "Delete worktree"),
								ask: t("repoWorktree.afterMerge.ask", "Ask each time"),
							}).filter((option) => option.value !== "")}
						>
							{(option) => <option value={option.value}>{option.label}</option>}
						</For>
					</Select>
				</div>

				<div class={r.group}>
					<div class={s.helpLabel}>
						<label for="repo-field-8">{t("repoWorktree.label.autoDeleteOnPrClose", "Auto-Delete on PR Close")}</label>
						<ContextualHelp label={t("repoWorktree.label.autoDeleteOnPrClose", "Auto-Delete on PR Close")} />
					</div>
					<Select
						data-setting-id="repositories.repos.{key}.auto_delete_on_pr_close"
						id="repo-field-8"
						value={props.settings.autoDeleteOnPrClose ?? INHERIT}
						onChange={(e) =>
							props.onUpdate(
								"autoDeleteOnPrClose",
								e.currentTarget.value === INHERIT ? null : (e.currentTarget.value as AutoDeleteOnPrClose),
							)
						}
					>
						<option value={INHERIT}>
							{t("repoWorktree.autoDelete.useDefault", "Use global default ({default})", {
								default: props.defaults.autoDeleteOnPrClose,
							})}
						</option>
						<For
							each={settingOptions("repositories.repos.{key}.auto_delete_on_pr_close", {
								off: t("repoWorktree.autoDelete.off", "Off"),
								ask: t("repoWorktree.autoDelete.ask", "Ask before deleting"),
								auto: t("repoWorktree.autoDelete.auto", "Auto-delete silently"),
							}).filter((option) => option.value !== "")}
						>
							{(option) => <option value={option.value}>{option.label}</option>}
						</For>
					</Select>
					<p class={s.hint}>
						{t("repoWorktree.hint.autoDelete", "Delete local branch when its PR is merged or closed")}
					</p>
				</div>

				<div class={r.group}>
					<label>PR Visibility</label>
					<SettingRow label="Draft PRs">
						{(id, descriptionId) => (
							<Select
								id={id}
								aria-describedby={descriptionId()}
								data-setting-id="repositories.repos.{key}.pr_hide_drafts"
								value={props.settings.prHideDrafts === null ? INHERIT : String(props.settings.prHideDrafts)}
								onChange={(e) =>
									props.onUpdate(
										"prHideDrafts",
										e.currentTarget.value === INHERIT ? null : e.currentTarget.value === "true",
									)
								}
							>
								<option value={INHERIT}>
									Use global default ({settingsStore.state.prHideDrafts ? "Hide" : "Show"})
								</option>
								<option value="false">Show</option>
								<option value="true">Hide</option>
							</Select>
						)}
					</SettingRow>
					<SettingRow label="Conflicting PRs">
						{(id, descriptionId) => (
							<Select
								id={id}
								aria-describedby={descriptionId()}
								data-setting-id="repositories.repos.{key}.pr_hide_conflicting"
								value={props.settings.prHideConflicting === null ? INHERIT : String(props.settings.prHideConflicting)}
								onChange={(e) =>
									props.onUpdate(
										"prHideConflicting",
										e.currentTarget.value === INHERIT ? null : e.currentTarget.value === "true",
									)
								}
							>
								<option value={INHERIT}>
									Use global default ({settingsStore.state.prHideConflicting ? "Hide" : "Show"})
								</option>
								<option value="false">Show</option>
								<option value="true">Hide</option>
							</Select>
						)}
					</SettingRow>
					<SettingRow label="CI Failing PRs">
						{(id, descriptionId) => (
							<Select
								id={id}
								aria-describedby={descriptionId()}
								data-setting-id="repositories.repos.{key}.pr_hide_ci_failing"
								value={props.settings.prHideCiFailing === null ? INHERIT : String(props.settings.prHideCiFailing)}
								onChange={(e) =>
									props.onUpdate(
										"prHideCiFailing",
										e.currentTarget.value === INHERIT ? null : e.currentTarget.value === "true",
									)
								}
							>
								<option value={INHERIT}>
									Use global default ({settingsStore.state.prHideCiFailing ? "Hide" : "Show"})
								</option>
								<option value="false">Show</option>
								<option value="true">Hide</option>
							</Select>
						)}
					</SettingRow>
				</div>

				<Show when={isMacOS()}>
					<div class={r.group}>
						<label>{t("repoWorktree.label.terminal", "Terminal")}</label>

						<BooleanOverride
							data-setting-exclusion="transient.repositories.terminal_meta_hotkeys"
							label="Enable Cmd+1-9 terminal hotkeys"
							value={props.settings.terminalMetaHotkeys}
							fallback={true}
							onChange={(v) => props.onUpdate("terminalMetaHotkeys", v)}
						/>
					</div>
				</Show>
			</SectionCard>
		</div>
	);
};
