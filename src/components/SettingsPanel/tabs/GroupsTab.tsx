import { Component, For, Show, createSignal } from "solid-js";
import { repositoriesStore } from "../../../stores/repositories";
import type { RepoGroup } from "../../../stores/repositories";
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

export const GroupsTab: Component = () => {
  const groups = () =>
    repositoriesStore.state.groupOrder
      .map((id) => repositoriesStore.state.groups[id])
      .filter(Boolean);

  const handleAddGroup = () => {
    repositoriesStore.createGroup(t("groups.defaultGroupName", "New Group"));
  };

  return (
    <div class={s.section}>
      <h3>{t("groups.heading.repositoryGroups", "Repository Groups")}</h3>
      <p class={s.hint}>{t("groups.hint.organize", "Organize repositories into color-coded groups in the sidebar")}</p>

      <Show when={groups().length === 0}>
        <div class={s.groupsEmpty}>{t("groups.empty.noGroups", "No groups yet")}</div>
      </Show>

      <For each={groups()}>
        {(group) => <GroupSettingsItem group={group} />}
      </For>

      <button class={s.groupsAddBtn} onClick={handleAddGroup}>
        {t("groups.btn.addGroup", "Add Group")}
      </button>
    </div>
  );
};
