import { Component, For, Show, createSignal } from "solid-js";
import { repositoriesStore } from "../../../stores/repositories";
import type { RepoGroup } from "../../../stores/repositories";

/** Preset colors for groups */
export const GROUP_PRESET_COLORS = [
  { hex: "#4A9EFF", name: "Blue" },
  { hex: "#FF6B6B", name: "Red" },
  { hex: "#50C878", name: "Green" },
  { hex: "#FFB347", name: "Orange" },
  { hex: "#B19CD9", name: "Purple" },
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
      setNameError("Name cannot be empty");
      return;
    }
    const ok = repositoriesStore.renameGroup(props.group.id, name);
    if (!ok) {
      setNameError("Name already exists");
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
    <div class="group-settings-item">
      <div class="group-settings-row">
        <Show
          when={editing()}
          fallback={
            <span
              class="group-settings-name"
              onDblClick={() => setEditing(true)}
            >
              {props.group.name}
            </span>
          }
        >
          <input
            class="group-settings-name-input"
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
          class="group-delete-btn"
          onClick={() => repositoriesStore.deleteGroup(props.group.id)}
          title="Delete group"
        >
          ×
        </button>
      </div>
      <Show when={nameError()}>
        <div class="group-name-error">{nameError()}</div>
      </Show>
      <div class="group-color-picker">
        <For each={GROUP_PRESET_COLORS}>
          {(preset) => (
            <button
              class={`color-swatch ${props.group.color === preset.hex ? "active" : ""}`}
              style={{ background: preset.hex }}
              onClick={() => repositoriesStore.setGroupColor(props.group.id, preset.hex)}
              title={preset.name}
            />
          )}
        </For>
        <button
          class={`color-swatch clear ${!props.group.color ? "active" : ""}`}
          onClick={() => repositoriesStore.setGroupColor(props.group.id, "")}
          title="No color"
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
    repositoriesStore.createGroup("New Group");
  };

  return (
    <div class="settings-section">
      <h3>Repository Groups</h3>
      <p class="settings-hint">Organize your repositories into named groups with colors.</p>

      <Show when={groups().length === 0}>
        <div class="groups-empty">No groups yet. Create one to get started.</div>
      </Show>

      <For each={groups()}>
        {(group) => <GroupSettingsItem group={group} />}
      </For>

      <button class="groups-add-btn" onClick={handleAddGroup}>
        + Add Group
      </button>
    </div>
  );
};
