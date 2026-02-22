import { Component, For, Show, createSignal } from "solid-js";
import { settingsStore, FONT_FAMILIES } from "../../../stores/settings";
import { uiStore } from "../../../stores/ui";
import { repositoriesStore } from "../../../stores/repositories";
import type { RepoGroup } from "../../../stores/repositories";
import type { FontType } from "../../../stores/settings";
import { THEME_NAMES } from "../../../themes";
import { t } from "../../../i18n";
import { cx } from "../../../utils";
import s from "../Settings.module.css";

/** Preset colors for groups and sidebar */
export const PRESET_COLORS = [
  { hex: "#4A9EFF", name: "Blue" },
  { hex: "#FF6B6B", name: "Red" },
  { hex: "#50C878", name: "Green" },
  { hex: "#FFB347", name: "Orange" },
  { hex: "#B19CD9", name: "Purple" },
  { hex: "#FF85A2", name: "Pink" },
  { hex: "#5BC0BE", name: "Teal" },
  { hex: "#FFD93D", name: "Yellow" },
];

/** Single group row in the settings list */
const GroupSettingsItem: Component<{
  group: RepoGroup;
}> = (props) => {
  const [editing, setEditing] = createSignal(false);
  const [editName, setEditName] = createSignal(props.group.name);
  const [nameError, setNameError] = createSignal("");

  const commitRename = () => {
    const name = editName().trim();
    if (!name) {
      setNameError(t("groups.error.nameEmpty", "Name cannot be empty"));
      return;
    }
    const ok = repositoriesStore.renameGroup(props.group.id, name);
    if (!ok) {
      setNameError(t("groups.error.nameExists", "A group with this name already exists"));
      return;
    }
    setNameError("");
    setEditing(false);
  };

  const cancelEdit = () => {
    setEditName(props.group.name);
    setNameError("");
    setEditing(false);
  };

  return (
    <div class={s.groupItem}>
      <div class={s.groupRow}>
        <Show
          when={editing()}
          fallback={
            <span
              class={s.groupName}
              onDblClick={() => setEditing(true)}
            >
              {props.group.name}
            </span>
          }
        >
          <input
            class={s.groupNameInput}
            value={editName()}
            onInput={(e) => {
              setEditName(e.currentTarget.value);
              setNameError("");
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitRename();
              if (e.key === "Escape") cancelEdit();
            }}
            autofocus
          />
        </Show>
        <button
          class={s.groupDeleteBtn}
          onClick={() => repositoriesStore.deleteGroup(props.group.id)}
          title={t("groups.btn.deleteGroup", "Delete group")}
        >
          ×
        </button>
      </div>
      <Show when={nameError()}>
        <div class={s.groupNameError}>{nameError()}</div>
      </Show>
      <div class={s.groupColorPicker}>
        <For each={PRESET_COLORS}>
          {(preset) => (
            <button
              class={cx(s.colorSwatch, props.group.color === preset.hex && s.active)}
              style={{ background: preset.hex }}
              onClick={() => repositoriesStore.setGroupColor(props.group.id, preset.hex)}
              title={preset.name}
            />
          )}
        </For>
        <button
          class={cx(s.colorSwatch, s.colorSwatchClear, !props.group.color && s.active)}
          onClick={() => repositoriesStore.setGroupColor(props.group.id, "")}
          title={t("groups.btn.noColor", "No color")}
        >
          ×
        </button>
      </div>
    </div>
  );
};

export const AppearanceTab: Component = () => {
  const groups = () =>
    repositoriesStore.state.groupOrder
      .map((id) => repositoriesStore.state.groups[id])
      .filter(Boolean);

  return (
    <div class={s.section}>
      <h3>{t("appearance.heading.theme", "Theme")}</h3>

      <div class={s.group}>
        <label>{t("appearance.label.terminalTheme", "Terminal Theme")}</label>
        <select
          value={settingsStore.state.theme}
          onChange={(e) => settingsStore.setTheme(e.currentTarget.value)}
        >
          <For each={Object.entries(THEME_NAMES)}>
            {([value, label]) => <option value={value}>{label}</option>}
          </For>
        </select>
        <p class={s.hint}>{t("appearance.hint.terminalTheme", "Color theme for terminal output and app chrome")}</p>
      </div>

      <h3>{t("appearance.heading.terminal", "Terminal")}</h3>

      <div class={s.group}>
        <label>{t("appearance.label.terminalFont", "Terminal Font")}</label>
        <select
          value={settingsStore.state.font}
          onChange={(e) => settingsStore.setFont(e.currentTarget.value as FontType)}
        >
          <For each={Object.entries(FONT_FAMILIES)}>
            {([value, _label]) => <option value={value}>{value}</option>}
          </For>
        </select>
        <p class={s.hint}>{t("appearance.hint.terminalFont", "Monospace font for terminals")}</p>
      </div>

      <div class={s.group}>
        <label>{t("appearance.label.defaultFontSize", "Default Font Size")}</label>
        <div class={s.slider}>
          <input
            type="range"
            min="8"
            max="32"
            value={settingsStore.state.defaultFontSize}
            onInput={(e) => settingsStore.setDefaultFontSize(parseInt(e.currentTarget.value))}
          />
          <span>{settingsStore.state.defaultFontSize}px</span>
        </div>
        <p class={s.hint}>{t("appearance.hint.defaultFontSize", "Default font size for new terminals")}</p>
      </div>

      <h3>{t("appearance.heading.tabs", "Tabs")}</h3>

      <div class={s.group}>
        <label>{t("appearance.label.splitTabMode", "Split Tab Mode")}</label>
        <select
          value={settingsStore.state.splitTabMode}
          onChange={(e) => {
            const value = e.currentTarget.value;
            if (value === "separate" || value === "unified") {
              settingsStore.setSplitTabMode(value);
            }
          }}
        >
          <option value="separate">{t("appearance.splitTabMode.separate", "Separate")}</option>
          <option value="unified">{t("appearance.splitTabMode.unified", "Unified")}</option>
        </select>
        <p class={s.hint}>{t("appearance.hint.splitTabMode", "How worktree tabs are arranged in the tab bar")}</p>
      </div>

      <div class={s.group}>
        <label>{t("appearance.label.maxTabNameLength", "Max Tab Name Length")}</label>
        <div class={s.slider}>
          <input
            type="range"
            min="10"
            max="60"
            value={settingsStore.state.maxTabNameLength}
            onInput={(e) => settingsStore.setMaxTabNameLength(parseInt(e.currentTarget.value))}
          />
          <span>{settingsStore.state.maxTabNameLength}</span>
        </div>
        <p class={s.hint}>{t("appearance.hint.maxTabNameLength", "Maximum characters shown in tab names before truncating")}</p>
      </div>

      <h3>{t("appearance.heading.groups", "Repository Groups")}</h3>
      <p class={s.hint}>{t("appearance.hint.groups", "Organize repositories into color-coded groups in the sidebar")}</p>

      <Show when={groups().length === 0}>
        <div class={s.groupsEmpty}>{t("groups.empty.noGroups", "No groups yet")}</div>
      </Show>

      <For each={groups()}>
        {(group) => <GroupSettingsItem group={group} />}
      </For>

      <button class={s.groupsAddBtn} onClick={() => repositoriesStore.createGroup(t("groups.defaultGroupName", "New Group"))}>
        {t("groups.btn.addGroup", "Add Group")}
      </button>

      <h3>{t("appearance.heading.layout", "Layout")}</h3>

      <div class={s.group}>
        <button
          class={s.testBtn}
          onClick={() => uiStore.resetLayout()}
        >
          {t("appearance.btn.resetLayout", "Reset Panel Sizes")}
        </button>
        <p class={s.hint}>{t("appearance.hint.resetLayout", "Reset sidebar and panel widths to default values")}</p>
      </div>
    </div>
  );
};
