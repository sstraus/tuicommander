import { Component, For, Show } from "solid-js";
import type { RepoSettings } from "../../../stores/repoSettings";
import type { RepoDefaults, WorktreeStorage, OrphanCleanup, MergeStrategy, WorktreeAfterMerge } from "../../../stores/repoDefaults";
import { PRESET_COLORS } from "./AppearanceTab";
import { isMacOS } from "../../../platform";
import { t } from "../../../i18n";
import { cx } from "../../../utils";
import s from "../Settings.module.css";

export interface RepoTabProps {
  settings: RepoSettings;
  defaults: RepoDefaults;
  onUpdate: <K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => void;
}

/** Returns the effective (resolved) value for a nullable boolean field */
function effectiveBool(override: boolean | null, fallback: boolean): boolean {
  return override ?? fallback;
}

/** "inherit" sentinel value for nullable dropdowns */
const INHERIT = "__inherit__";

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
    <div class={s.section}>
      <h3>{t("repoWorktree.heading.repository", "Repository")}</h3>

      <div class={s.group}>
        <label>{t("repoWorktree.label.displayName", "Display Name")}</label>
        <input
          type="text"
          value={props.settings.displayName ?? ""}
          onInput={(e) => props.onUpdate("displayName", e.currentTarget.value)}
          placeholder={t("repoWorktree.placeholder.displayName", "Custom name...")}
        />
        <p class={s.hint}>{t("repoWorktree.hint.displayName", "Shown in sidebar instead of folder name")}</p>
      </div>

      <div class={s.group}>
        <label>{t("repoWorktree.label.sidebarColor", "Sidebar Color")}</label>
        <div class={s.groupColorPicker}>
          <For each={PRESET_COLORS}>
            {(preset) => (
              <button
                class={cx(s.colorSwatch, props.settings.color === preset.hex && s.active)}
                style={{ background: preset.hex }}
                onClick={() => props.onUpdate("color", preset.hex)}
                title={preset.name}
              />
            )}
          </For>
          <label
            class={cx(s.colorSwatch, s.colorSwatchCustom, props.settings.color && !PRESET_COLORS.some((p) => p.hex === props.settings.color) && s.active)}
            style={{
              background: props.settings.color && !PRESET_COLORS.some((p) => p.hex === props.settings.color)
                ? props.settings.color
                : "var(--bg-tertiary)",
            }}
            title={t("repoWorktree.btn.customColor", "Custom color")}
          >
            <input
              type="color"
              value={props.settings.color || "#999999"}
              onInput={(e) => props.onUpdate("color", e.currentTarget.value)}
            />
            <Show when={!props.settings.color || PRESET_COLORS.some((p) => p.hex === props.settings.color)}>
              <span class={s.colorSwatchLabel}>⋯</span>
            </Show>
          </label>
          <button
            class={cx(s.colorSwatch, s.colorSwatchClear, !props.settings.color && s.active)}
            onClick={() => props.onUpdate("color", "")}
            title={t("repoWorktree.btn.defaultColor", "Use default color")}
          >
            ×
          </button>
        </div>
        <p class={s.hint}>{t("repoWorktree.hint.sidebarColor", "Color-code this repo in the sidebar")}</p>
      </div>

      <h3>{t("repoWorktree.heading.worktreeConfiguration", "Worktree Configuration")}</h3>

      <div class={s.group}>
        <label>{t("repoWorktree.label.branchFrom", "Branch From")}</label>
        <select
          value={baseBranchValue()}
          onChange={(e) => handleBaseBranchChange(e.currentTarget.value)}
        >
          <option value={INHERIT}>
            {t("repoWorktree.baseBranch.useGlobalDefault", "Use global default ({default})", { default: props.defaults.baseBranch })}
          </option>
          <For each={branchOptions}>
            {(opt) => <option value={opt.value}>{opt.label}</option>}
          </For>
        </select>
        <p class={s.hint}>{t("repoWorktree.hint.branchFrom", "Base branch for new worktrees")}</p>
      </div>

      <div class={s.group}>
        <label>{t("repoWorktree.label.fileHandling", "File Handling")}</label>

        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={effectiveBool(props.settings.copyIgnoredFiles, props.defaults.copyIgnoredFiles)}
            onChange={(e) => props.onUpdate("copyIgnoredFiles", e.currentTarget.checked)}
          />
          <span>
            {t("repoWorktree.toggle.copyIgnoredFiles", "Copy ignored files")}
            <Show when={props.settings.copyIgnoredFiles === null}>
              <span class={s.hintInline}> {t("repoWorktree.hint.globalDefault", "(Global Default)")}</span>
            </Show>
          </span>
        </div>

        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={effectiveBool(props.settings.copyUntrackedFiles, props.defaults.copyUntrackedFiles)}
            onChange={(e) => props.onUpdate("copyUntrackedFiles", e.currentTarget.checked)}
          />
          <span>
            {t("repoWorktree.toggle.copyUntrackedFiles", "Copy untracked files")}
            <Show when={props.settings.copyUntrackedFiles === null}>
              <span class={s.hintInline}> {t("repoWorktree.hint.globalDefault", "(Global Default)")}</span>
            </Show>
          </span>
        </div>
      </div>

      <h3>{t("repoWorktree.heading.worktreeSettings", "Worktree Settings")}</h3>

      <div class={s.group}>
        <label>{t("repoWorktree.label.worktreeStorage", "Storage Strategy")}</label>
        <select
          value={props.settings.worktreeStorage ?? INHERIT}
          onChange={(e) => props.onUpdate("worktreeStorage", e.currentTarget.value === INHERIT ? null : e.currentTarget.value as WorktreeStorage)}
        >
          <option value={INHERIT}>
            {t("repoWorktree.worktreeStorage.useDefault", "Use global default ({default})", { default: props.defaults.worktreeStorage })}
          </option>
          <option value="sibling">{t("repoWorktree.worktreeStorage.sibling", "Sibling directory (__wt)")}</option>
          <option value="app-dir">{t("repoWorktree.worktreeStorage.appDir", "App config directory")}</option>
          <option value="inside-repo">{t("repoWorktree.worktreeStorage.insideRepo", "Inside repository (.worktrees)")}</option>
        </select>
      </div>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={effectiveBool(props.settings.promptOnCreate, props.defaults.promptOnCreate)}
            onChange={(e) => props.onUpdate("promptOnCreate", e.currentTarget.checked)}
          />
          <span>
            {t("repoWorktree.toggle.promptOnCreate", "Prompt for branch name during creation")}
            <Show when={props.settings.promptOnCreate === null}>
              <span class={s.hintInline}> {t("repoWorktree.hint.globalDefault", "(Global Default)")}</span>
            </Show>
          </span>
        </div>
      </div>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={effectiveBool(props.settings.deleteBranchOnRemove, props.defaults.deleteBranchOnRemove)}
            onChange={(e) => props.onUpdate("deleteBranchOnRemove", e.currentTarget.checked)}
          />
          <span>
            {t("repoWorktree.toggle.deleteBranchOnRemove", "Delete local branch when removing worktree")}
            <Show when={props.settings.deleteBranchOnRemove === null}>
              <span class={s.hintInline}> {t("repoWorktree.hint.globalDefault", "(Global Default)")}</span>
            </Show>
          </span>
        </div>
      </div>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={effectiveBool(props.settings.autoArchiveMerged, props.defaults.autoArchiveMerged)}
            onChange={(e) => props.onUpdate("autoArchiveMerged", e.currentTarget.checked)}
          />
          <span>
            {t("repoWorktree.toggle.autoArchiveMerged", "Auto-archive merged worktrees")}
            <Show when={props.settings.autoArchiveMerged === null}>
              <span class={s.hintInline}> {t("repoWorktree.hint.globalDefault", "(Global Default)")}</span>
            </Show>
          </span>
        </div>
      </div>

      <div class={s.group}>
        <label>{t("repoWorktree.label.orphanCleanup", "Orphan Worktree Cleanup")}</label>
        <select
          value={props.settings.orphanCleanup ?? INHERIT}
          onChange={(e) => props.onUpdate("orphanCleanup", e.currentTarget.value === INHERIT ? null : e.currentTarget.value as OrphanCleanup)}
        >
          <option value={INHERIT}>
            {t("repoWorktree.orphanCleanup.useDefault", "Use global default ({default})", { default: props.defaults.orphanCleanup })}
          </option>
          <option value="ask">{t("repoWorktree.orphanCleanup.ask", "Ask before removing")}</option>
          <option value="on">{t("repoWorktree.orphanCleanup.on", "Auto-remove")}</option>
          <option value="off">{t("repoWorktree.orphanCleanup.off", "Keep (mark as detached)")}</option>
        </select>
      </div>

      <div class={s.group}>
        <label>{t("repoWorktree.label.prMergeStrategy", "PR Merge Strategy")}</label>
        <select
          value={props.settings.prMergeStrategy ?? INHERIT}
          onChange={(e) => props.onUpdate("prMergeStrategy", e.currentTarget.value === INHERIT ? null : e.currentTarget.value as MergeStrategy)}
        >
          <option value={INHERIT}>
            {t("repoWorktree.mergeStrategy.useDefault", "Use global default ({default})", { default: props.defaults.prMergeStrategy })}
          </option>
          <option value="merge">{t("repoWorktree.mergeStrategy.merge", "Merge")}</option>
          <option value="squash">{t("repoWorktree.mergeStrategy.squash", "Squash")}</option>
          <option value="rebase">{t("repoWorktree.mergeStrategy.rebase", "Rebase")}</option>
        </select>
      </div>

      <div class={s.group}>
        <label>{t("repoWorktree.label.afterMerge", "After Merge Behavior")}</label>
        <select
          value={props.settings.afterMerge ?? INHERIT}
          onChange={(e) => props.onUpdate("afterMerge", e.currentTarget.value === INHERIT ? null : e.currentTarget.value as WorktreeAfterMerge)}
        >
          <option value={INHERIT}>
            {t("repoWorktree.afterMerge.useDefault", "Use global default ({default})", { default: props.defaults.afterMerge })}
          </option>
          <option value="archive">{t("repoWorktree.afterMerge.archive", "Archive worktree")}</option>
          <option value="delete">{t("repoWorktree.afterMerge.delete", "Delete worktree")}</option>
          <option value="ask">{t("repoWorktree.afterMerge.ask", "Ask each time")}</option>
        </select>
      </div>

      <Show when={isMacOS()}>
        <div class={s.group}>
          <label>{t("repoWorktree.label.terminal", "Terminal")}</label>

          <div class={s.toggle}>
            <input
              type="checkbox"
              checked={effectiveBool(props.settings.terminalMetaHotkeys, true)}
              onChange={(e) => props.onUpdate("terminalMetaHotkeys", e.currentTarget.checked)}
            />
            <span>
              {t("repoWorktree.toggle.terminalMetaHotkeys", "Enable Cmd+1-9 terminal hotkeys")}
              <Show when={props.settings.terminalMetaHotkeys === null}>
                <span class={s.hintInline}> {t("repoWorktree.hint.terminalMetaDefault", "(Default: On)")}</span>
              </Show>
            </span>
          </div>
        </div>
      </Show>
    </div>
  );
};
