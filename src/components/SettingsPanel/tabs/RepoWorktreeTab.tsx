import { Component, For, Show } from "solid-js";
import type { RepoSettings } from "../../../stores/repoSettings";
import type { RepoDefaults } from "../../../stores/repoDefaults";
import { PRESET_COLORS } from "./GroupsTab";
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

export const RepoWorktreeTab: Component<RepoTabProps> = (props) => {
  const branchOptions = [
    { value: "automatic", label: t("repoWorktree.baseBranch.automatic", "Automatic") },
    { value: "main", label: "main" },
    { value: "master", label: "master" },
    { value: "develop", label: "develop" },
  ];

  /** "inherit" sentinel value for the baseBranch dropdown */
  const INHERIT = "__inherit__";

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
