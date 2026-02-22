import { Component, For, Show } from "solid-js";
import { settingsStore, IDE_NAMES, FONT_FAMILIES } from "../../../stores/settings";
import { repoDefaultsStore } from "../../../stores/repoDefaults";
import { updaterStore } from "../../../stores/updater";
import { THEME_NAMES } from "../../../themes";
import type { IdeType, FontType } from "../../../stores/settings";
import { t } from "../../../i18n";
import s from "../Settings.module.css";

export const GeneralTab: Component = () => {
  return (
    <div class={s.section}>
      <h3>{t("general.heading.general", "General")}</h3>

      <div class={s.group}>
        <label>{t("general.label.language", "Language")}</label>
        <select
          value={settingsStore.state.language}
          onChange={(e) => settingsStore.setLanguage(e.currentTarget.value)}
        >
          <option value="en">{t("general.language.english", "English")}</option>
        </select>
        <p class={s.hint}>{t("general.hint.language", "Interface language")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.defaultIde", "Default IDE")}</label>
        <select
          value={settingsStore.state.ide}
          onChange={(e) => settingsStore.setIde(e.currentTarget.value as IdeType)}
        >
          <For each={Object.entries(IDE_NAMES)}>
            {([value, label]) => <option value={value}>{label}</option>}
          </For>
        </select>
        <p class={s.hint}>{t("general.hint.defaultIde", "IDE used to open repositories")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.shell", "Shell")}</label>
        <input
          type="text"
          value={settingsStore.state.shell ?? ""}
          onInput={(e) => settingsStore.setShell(e.currentTarget.value)}
          placeholder={t("general.placeholder.shell", "Default shell")}
        />
        <p class={s.hint}>{t("general.hint.shell", "Shell used in terminals (leave blank for system default)")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.splitTabMode", "Split Tab Mode")}</label>
        <select
          value={settingsStore.state.splitTabMode}
          onChange={(e) => {
            const value = e.currentTarget.value;
            if (value === "separate" || value === "unified") {
              settingsStore.setSplitTabMode(value);
            }
          }}
        >
          <option value="separate">{t("general.splitTabMode.separate", "Separate")}</option>
          <option value="unified">{t("general.splitTabMode.unified", "Unified")}</option>
        </select>
        <p class={s.hint}>{t("general.hint.splitTabMode", "How worktree tabs are arranged in the tab bar")}</p>
      </div>

      <h3>{t("general.heading.confirmations", "Confirmations")}</h3>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={settingsStore.state.confirmBeforeQuit}
            onChange={(e) => settingsStore.setConfirmBeforeQuit(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.confirmBeforeQuit", "Confirm before quitting")}</span>
        </div>
        <p class={s.hint}>{t("general.hint.confirmBeforeQuit", "Show a confirmation dialog when closing the app")}</p>
      </div>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={settingsStore.state.confirmBeforeClosingTab}
            onChange={(e) => settingsStore.setConfirmBeforeClosingTab(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.confirmBeforeClosingTab", "Confirm before closing a tab")}</span>
        </div>
        <p class={s.hint}>{t("general.hint.confirmBeforeClosingTab", "Show a confirmation dialog when closing a terminal tab")}</p>
      </div>

      <h3>{t("general.heading.powerManagement", "Power Management")}</h3>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={settingsStore.state.preventSleepWhenBusy}
            onChange={(e) => settingsStore.setPreventSleepWhenBusy(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.preventSleepWhenBusy", "Prevent sleep when busy")}</span>
        </div>
        <p class={s.hint}>{t("general.hint.preventSleepWhenBusy", "Keep the system awake while scripts are running")}</p>
      </div>

      <h3>{t("general.heading.updates", "Updates")}</h3>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={settingsStore.state.autoUpdateEnabled}
            onChange={(e) => settingsStore.setAutoUpdateEnabled(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.autoUpdateEnabled", "Automatically check for updates")}</span>
        </div>
        <p class={s.hint}>{t("general.hint.autoUpdateEnabled", "Download and install updates in the background")}</p>
      </div>

      <div class={s.group}>
        <button
          class={s.testBtn}
          onClick={() => { updaterStore.checkForUpdate().catch((err: unknown) => console.debug("Update check failed:", err)); }}
          disabled={updaterStore.state.checking || updaterStore.state.downloading}
        >
          {updaterStore.state.checking ? t("general.btn.checking", "Checking...") : t("general.btn.checkNow", "Check Now")}
        </button>
        <Show when={updaterStore.state.available && updaterStore.state.version}>
          <p class={s.hint} style={{ color: "var(--accent-green, #4ec9b0)" }}>
            {t("general.hint.updateAvailable", "Version {version} is available!", { version: updaterStore.state.version ?? "" })}
          </p>
        </Show>
        <Show when={!updaterStore.state.available && !updaterStore.state.checking && !updaterStore.state.error}>
          <p class={s.hint}>{t("general.hint.latestVersion", "You are on the latest version")}</p>
        </Show>
        <Show when={updaterStore.state.error}>
          <p class={s.hint} style={{ color: "var(--accent-red, #f44747)" }}>
            {updaterStore.state.error}
          </p>
        </Show>
      </div>

      <h3>{t("general.heading.gitIntegration", "Git Integration")}</h3>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={settingsStore.state.autoShowPrPopover}
            onChange={(e) => settingsStore.setAutoShowPrPopover(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.autoShowPrPopover", "Auto-show PR popover")}</span>
        </div>
        <p class={s.hint}>{t("general.hint.autoShowPrPopover", "Automatically open the PR panel when a branch has an associated pull request")}</p>
      </div>

      <h3>{t("general.heading.appearance", "Appearance")}</h3>

      <div class={s.group}>
        <label>{t("general.label.terminalTheme", "Terminal Theme")}</label>
        <select
          value={settingsStore.state.theme}
          onChange={(e) => settingsStore.setTheme(e.currentTarget.value)}
        >
          <For each={Object.entries(THEME_NAMES)}>
            {([value, label]) => <option value={value}>{label}</option>}
          </For>
        </select>
        <p class={s.hint}>{t("general.hint.terminalTheme", "Color theme for terminal output")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.terminalFont", "Terminal Font")}</label>
        <select
          value={settingsStore.state.font}
          onChange={(e) => settingsStore.setFont(e.currentTarget.value as FontType)}
        >
          <For each={Object.entries(FONT_FAMILIES)}>
            {([value, _label]) => <option value={value}>{value}</option>}
          </For>
        </select>
        <p class={s.hint}>{t("general.hint.terminalFont", "Monospace font for terminals")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.defaultFontSize", "Default Font Size")}</label>
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
        <p class={s.hint}>{t("general.hint.defaultFontSize", "Default font size for new terminals")}</p>
      </div>

      <h3>{t("general.heading.repoDefaults", "Repository Defaults")}</h3>
      <p class={s.hint} style={{ "margin-bottom": "12px" }}>
        {t("general.hint.repoDefaults", "These defaults apply to all repositories unless overridden per-repo")}
      </p>

      <div class={s.group}>
        <label>{t("general.label.defaultBaseBranch", "Default Base Branch")}</label>
        <select
          value={repoDefaultsStore.state.baseBranch}
          onChange={(e) => repoDefaultsStore.setBaseBranch(e.currentTarget.value)}
        >
          <option value="automatic">{t("general.baseBranch.automatic", "Automatic")}</option>
          <option value="main">main</option>
          <option value="master">master</option>
          <option value="develop">develop</option>
        </select>
        <p class={s.hint}>{t("general.hint.defaultBaseBranch", "Default base branch for new worktrees")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.fileHandlingDefaults", "File Handling Defaults")}</label>

        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={repoDefaultsStore.state.copyIgnoredFiles}
            onChange={(e) => repoDefaultsStore.setCopyIgnoredFiles(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.copyIgnoredFiles", "Copy ignored files")}</span>
        </div>

        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={repoDefaultsStore.state.copyUntrackedFiles}
            onChange={(e) => repoDefaultsStore.setCopyUntrackedFiles(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.copyUntrackedFiles", "Copy untracked files")}</span>
        </div>
      </div>

      <div class={s.group}>
        <label>{t("general.label.defaultSetupScript", "Default Setup Script")}</label>
        <textarea
          value={repoDefaultsStore.state.setupScript}
          onInput={(e) => repoDefaultsStore.setSetupScript(e.currentTarget.value)}
          placeholder="#!/bin/bash&#10;npm install"
          rows={4}
        />
        <p class={s.hint}>{t("general.hint.defaultSetupScript", "Shell script run when creating a new worktree")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.defaultRunScript", "Default Run Script")}</label>
        <textarea
          value={repoDefaultsStore.state.runScript}
          onInput={(e) => repoDefaultsStore.setRunScript(e.currentTarget.value)}
          placeholder="#!/bin/bash&#10;npm run dev"
          rows={4}
        />
        <p class={s.hint}>{t("general.hint.defaultRunScript", "Shell script run when launching the worktree")}</p>
      </div>

    </div>
  );
};
