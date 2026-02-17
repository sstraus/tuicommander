import { Component, For } from "solid-js";
import { settingsStore, IDE_NAMES, FONT_FAMILIES } from "../../../stores/settings";
import { THEME_NAMES } from "../../../themes";
import type { IdeType, FontType } from "../../../stores/settings";

export const GeneralTab: Component = () => {
  return (
    <div class="settings-section">
      <h3>General</h3>

      <div class="settings-group">
        <label>Default IDE</label>
        <select
          value={settingsStore.state.ide}
          onChange={(e) => settingsStore.setIde(e.currentTarget.value as IdeType)}
        >
          <For each={Object.entries(IDE_NAMES)}>
            {([value, label]) => <option value={value}>{label}</option>}
          </For>
        </select>
        <p class="settings-hint">Preferred IDE for opening repository actions</p>
      </div>

      <div class="settings-group">
        <label>Shell</label>
        <input
          type="text"
          value={settingsStore.state.shell ?? ""}
          onInput={(e) => settingsStore.setShell(e.currentTarget.value)}
          placeholder="System default ($SHELL)"
        />
        <p class="settings-hint">Custom shell path for new terminals. Leave empty to use system default.</p>
      </div>

      <div class="settings-group">
        <label>Split Tab Mode</label>
        <select
          value={settingsStore.state.splitTabMode}
          onChange={(e) => {
            const value = e.currentTarget.value;
            if (value === "separate" || value === "unified") {
              settingsStore.setSplitTabMode(value);
            }
          }}
        >
          <option value="separate">Separate tabs</option>
          <option value="unified">Unified tab</option>
        </select>
        <p class="settings-hint">Separate creates a tab per split pane. Unified shows both panes under one tab.</p>
      </div>

      <div class="settings-group">
        <label>Keyboard Shortcuts</label>
        <p class="settings-hint">
          <strong>Cmd+T:</strong> New terminal • <strong>Cmd+W:</strong> Close tab • <strong>Cmd+1-9:</strong> Switch tabs • <strong>Cmd+Plus/Minus:</strong> Zoom
        </p>
      </div>

      <h3>Confirmations</h3>

      <div class="settings-group">
        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={settingsStore.state.confirmBeforeQuit}
            onChange={(e) => settingsStore.setConfirmBeforeQuit(e.currentTarget.checked)}
          />
          <span>Confirm before quitting</span>
        </div>
        <p class="settings-hint">Show confirmation when closing the app with active terminals</p>
      </div>

      <div class="settings-group">
        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={settingsStore.state.confirmBeforeClosingTab}
            onChange={(e) => settingsStore.setConfirmBeforeClosingTab(e.currentTarget.checked)}
          />
          <span>Confirm before closing tab</span>
        </div>
        <p class="settings-hint">Ask before closing a terminal tab</p>
      </div>

      <h3>Git Integration</h3>

      <div class="settings-group">
        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={settingsStore.state.autoShowPrPopover}
            onChange={(e) => settingsStore.setAutoShowPrPopover(e.currentTarget.checked)}
          />
          <span>Auto-show PR detail popover</span>
        </div>
        <p class="settings-hint">Automatically show the PR detail popover when selecting a branch with an open PR</p>
      </div>

      <h3>Appearance</h3>

      <div class="settings-group">
        <label>Terminal Theme</label>
        <select
          value={settingsStore.state.theme}
          onChange={(e) => settingsStore.setTheme(e.currentTarget.value)}
        >
          <For each={Object.entries(THEME_NAMES)}>
            {([value, label]) => <option value={value}>{label}</option>}
          </For>
        </select>
        <p class="settings-hint">Color theme for terminal panels. Changes apply immediately.</p>
      </div>

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
