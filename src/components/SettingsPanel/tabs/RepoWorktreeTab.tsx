import { Component, For, Show } from "solid-js";
import type { RepoSettings } from "../../../stores/repoSettings";
import { PRESET_COLORS } from "./GroupsTab";

export interface RepoTabProps {
  settings: RepoSettings;
  onUpdate: <K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => void;
}

export const RepoWorktreeTab: Component<RepoTabProps> = (props) => {
  const branchOptions = [
    { value: "automatic", label: "Automatic (origin/main or origin/master)" },
    { value: "main", label: "main" },
    { value: "master", label: "master" },
    { value: "develop", label: "develop" },
  ];

  return (
    <div class="settings-section">
      <h3>Repository</h3>

      <div class="settings-group">
        <label>Display Name</label>
        <input
          type="text"
          value={props.settings.displayName ?? ""}
          onInput={(e) => props.onUpdate("displayName", e.currentTarget.value)}
          placeholder="Repository name"
        />
        <p class="settings-hint">Name shown in sidebar and tabs</p>
      </div>

      <div class="settings-group">
        <label>Sidebar Color</label>
        <div class="group-color-picker">
          <For each={PRESET_COLORS}>
            {(preset) => (
              <button
                class={`color-swatch ${props.settings.color === preset.hex ? "active" : ""}`}
                style={{ background: preset.hex }}
                onClick={() => props.onUpdate("color", preset.hex)}
                title={preset.name}
              />
            )}
          </For>
          <label
            class={`color-swatch custom ${props.settings.color && !PRESET_COLORS.some((p) => p.hex === props.settings.color) ? "active" : ""}`}
            style={{
              background: props.settings.color && !PRESET_COLORS.some((p) => p.hex === props.settings.color)
                ? props.settings.color
                : "var(--bg-tertiary)",
            }}
            title="Custom color"
          >
            <input
              type="color"
              value={props.settings.color || "#999999"}
              onInput={(e) => props.onUpdate("color", e.currentTarget.value)}
            />
            <Show when={!props.settings.color || PRESET_COLORS.some((p) => p.hex === props.settings.color)}>
              <span class="color-swatch-label">⋯</span>
            </Show>
          </label>
          <button
            class={`color-swatch clear ${!props.settings.color ? "active" : ""}`}
            onClick={() => props.onUpdate("color", "")}
            title="Default"
          >
            ×
          </button>
        </div>
        <p class="settings-hint">Custom color for repository name in sidebar</p>
      </div>

      <h3>Worktree Configuration</h3>

      <div class="settings-group">
        <label>Branch new workspaces from</label>
        <select
          value={props.settings.baseBranch ?? "automatic"}
          onChange={(e) => props.onUpdate("baseBranch", e.currentTarget.value)}
        >
          <For each={branchOptions}>
            {(opt) => <option value={opt.value}>{opt.label}</option>}
          </For>
        </select>
        <p class="settings-hint">Base branch for new worktrees</p>
      </div>

      <div class="settings-group">
        <label>File Handling</label>

        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={props.settings.copyIgnoredFiles ?? false}
            onChange={(e) => props.onUpdate("copyIgnoredFiles", e.currentTarget.checked)}
          />
          <span>Copy ignored files to new worktrees</span>
        </div>

        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={props.settings.copyUntrackedFiles ?? false}
            onChange={(e) => props.onUpdate("copyUntrackedFiles", e.currentTarget.checked)}
          />
          <span>Copy untracked files to new worktrees</span>
        </div>
      </div>
    </div>
  );
};
