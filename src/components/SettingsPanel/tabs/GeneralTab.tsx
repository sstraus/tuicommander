import { Component, For, Show } from "solid-js";
import { settingsStore, IDE_NAMES, FONT_FAMILIES } from "../../../stores/settings";
import { repoDefaultsStore } from "../../../stores/repoDefaults";
import { updaterStore } from "../../../stores/updater";
import { THEME_NAMES } from "../../../themes";
import type { IdeType, FontType } from "../../../stores/settings";

export const GeneralTab: Component = () => {
  return (
    <div class="settings-section">
      <h3>General</h3>

      <div class="settings-group">
        <label>Language</label>
        <select
          value={settingsStore.state.language}
          onChange={(e) => settingsStore.setLanguage(e.currentTarget.value)}
        >
          <option value="en">English</option>
        </select>
        <p class="settings-hint">UI display language. More languages coming soon.</p>
      </div>

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

      <h3>Power Management</h3>

      <div class="settings-group">
        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={settingsStore.state.preventSleepWhenBusy}
            onChange={(e) => settingsStore.setPreventSleepWhenBusy(e.currentTarget.checked)}
          />
          <span>Prevent sleep while agents are working</span>
        </div>
        <p class="settings-hint">Keep the system awake when any terminal session is busy (similar to caffeinate on macOS)</p>
      </div>

      <h3>Updates</h3>

      <div class="settings-group">
        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={settingsStore.state.autoUpdateEnabled}
            onChange={(e) => settingsStore.setAutoUpdateEnabled(e.currentTarget.checked)}
          />
          <span>Check for updates automatically</span>
        </div>
        <p class="settings-hint">Silently check for new versions on app startup</p>
      </div>

      <div class="settings-group">
        <button
          class="settings-test-btn"
          onClick={() => { updaterStore.checkForUpdate().catch((err: unknown) => console.debug("Update check failed:", err)); }}
          disabled={updaterStore.state.checking || updaterStore.state.downloading}
        >
          {updaterStore.state.checking ? "Checking..." : "Check Now"}
        </button>
        <Show when={updaterStore.state.available && updaterStore.state.version}>
          <p class="settings-hint" style={{ color: "var(--accent-green, #4ec9b0)" }}>
            Version {updaterStore.state.version} is available.
          </p>
        </Show>
        <Show when={!updaterStore.state.available && !updaterStore.state.checking && !updaterStore.state.error}>
          <p class="settings-hint">You're on the latest version.</p>
        </Show>
        <Show when={updaterStore.state.error}>
          <p class="settings-hint" style={{ color: "var(--accent-red, #f44747)" }}>
            {updaterStore.state.error}
          </p>
        </Show>
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

      <h3>Repository Defaults</h3>
      <p class="settings-hint" style={{ "margin-bottom": "12px" }}>
        Default settings applied to all repositories. Override individually in each repo's settings.
      </p>

      <div class="settings-group">
        <label>Default Base Branch</label>
        <select
          value={repoDefaultsStore.state.baseBranch}
          onChange={(e) => repoDefaultsStore.setBaseBranch(e.currentTarget.value)}
        >
          <option value="automatic">Automatic (origin/main or origin/master)</option>
          <option value="main">main</option>
          <option value="master">master</option>
          <option value="develop">develop</option>
        </select>
        <p class="settings-hint">Default base branch when creating new worktrees</p>
      </div>

      <div class="settings-group">
        <label>File Handling Defaults</label>

        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={repoDefaultsStore.state.copyIgnoredFiles}
            onChange={(e) => repoDefaultsStore.setCopyIgnoredFiles(e.currentTarget.checked)}
          />
          <span>Copy ignored files to new worktrees</span>
        </div>

        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={repoDefaultsStore.state.copyUntrackedFiles}
            onChange={(e) => repoDefaultsStore.setCopyUntrackedFiles(e.currentTarget.checked)}
          />
          <span>Copy untracked files to new worktrees</span>
        </div>
      </div>

      <div class="settings-group">
        <label>Default Setup Script</label>
        <textarea
          value={repoDefaultsStore.state.setupScript}
          onInput={(e) => repoDefaultsStore.setSetupScript(e.currentTarget.value)}
          placeholder="#!/bin/bash&#10;npm install"
          rows={4}
        />
        <p class="settings-hint">Executed once after a new worktree is created (e.g., npm install)</p>
      </div>

      <div class="settings-group">
        <label>Default Run Script</label>
        <textarea
          value={repoDefaultsStore.state.runScript}
          onInput={(e) => repoDefaultsStore.setRunScript(e.currentTarget.value)}
          placeholder="#!/bin/bash&#10;npm run dev"
          rows={4}
        />
        <p class="settings-hint">On-demand script launchable from the toolbar (e.g., npm run dev)</p>
      </div>

    </div>
  );
};
