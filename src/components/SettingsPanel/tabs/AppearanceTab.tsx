import { Component, For } from "solid-js";
import { settingsStore, FONT_FAMILIES } from "../../../stores/settings";
import type { FontType } from "../../../stores/settings";

export const AppearanceTab: Component = () => {
  return (
    <div class="settings-section">
      <h3>Appearance Settings</h3>

      <div class="settings-group">
        <label>Terminal Font</label>
        <select
          value={settingsStore.state.font}
          onChange={(e) => settingsStore.setFont(e.currentTarget.value as FontType)}
        >
          <For each={Object.entries(FONT_FAMILIES)}>
            {([value, _label]) => <option value={value}>{value}</option>}
          </For>
        </select>
        <p class="settings-hint">Monospace font used for terminal display</p>
      </div>

      <div class="settings-group">
        <label>Default Font Size</label>
        <div class="settings-slider">
          <input
            type="range"
            min="8"
            max="32"
            value={settingsStore.state.defaultFontSize}
            onInput={(e) => settingsStore.setDefaultFontSize(parseInt(e.currentTarget.value))}
          />
          <span>{settingsStore.state.defaultFontSize}px</span>
        </div>
        <p class="settings-hint">Default size for new terminals. Use <strong>Cmd+Plus/Minus</strong> to zoom individually, <strong>Cmd+0</strong> to reset</p>
      </div>
    </div>
  );
};
