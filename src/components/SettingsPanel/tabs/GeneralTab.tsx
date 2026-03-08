import { Component, For, Show } from "solid-js";
import { settingsStore, IDE_NAMES } from "../../../stores/settings";
import { appLogger } from "../../../stores/appLogger";
import { repoDefaultsStore } from "../../../stores/repoDefaults";
import { updaterStore } from "../../../stores/updater";
import type { IdeType, UpdateChannel } from "../../../stores/settings";
import type { WorktreeStorage, OrphanCleanup, MergeStrategy, WorktreeAfterMerge, AutoDeleteOnPrClose } from "../../../stores/repoDefaults";
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
        <label>{t("general.label.updateChannel", "Update Channel")}</label>
        <select
          value={settingsStore.state.updateChannel}
          onChange={(e) => settingsStore.setUpdateChannel(e.currentTarget.value as UpdateChannel)}
        >
          <option value="stable">{t("general.channel.stable", "Stable")}</option>
          <option value="beta">{t("general.channel.beta", "Beta")}</option>
          <option value="nightly">{t("general.channel.nightly", "Nightly")}</option>
        </select>
        <Show when={settingsStore.state.updateChannel !== "stable"}>
          <p class={s.hint} style={{ color: "var(--warning, #e5c07b)" }}>
            {t("general.hint.updateChannelWarning", "Beta and nightly builds may be unstable")}
          </p>
        </Show>
        <Show when={settingsStore.state.updateChannel === "stable"}>
          <p class={s.hint}>{t("general.hint.updateChannel", "Choose which release channel to receive updates from")}</p>
        </Show>
      </div>

      <div class={s.group}>
        <button
          class={s.testBtn}
          onClick={() => { updaterStore.checkForUpdate().catch((err: unknown) => appLogger.debug("app", "Update check failed", err)); }}
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

      <h3>{t("general.heading.worktreeDefaults", "Worktree Defaults")}</h3>
      <p class={s.hint} style={{ "margin-bottom": "12px" }}>
        {t("general.hint.worktreeDefaults", "Default worktree behavior for all repositories")}
      </p>

      <div class={s.group}>
        <label>{t("general.label.worktreeStorage", "Storage Strategy")}</label>
        <select
          value={repoDefaultsStore.state.worktreeStorage}
          onChange={(e) => repoDefaultsStore.setWorktreeStorage(e.currentTarget.value as WorktreeStorage)}
        >
          <option value="sibling">{t("general.worktreeStorage.sibling", "Sibling directory (__wt)")}</option>
          <option value="app-dir">{t("general.worktreeStorage.appDir", "App config directory")}</option>
          <option value="inside-repo">{t("general.worktreeStorage.insideRepo", "Inside repository (.worktrees)")}</option>
        </select>
        <p class={s.hint}>{t("general.hint.worktreeStorage", "Where to create worktree directories")}</p>
      </div>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={repoDefaultsStore.state.promptOnCreate}
            onChange={(e) => repoDefaultsStore.setPromptOnCreate(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.promptOnCreate", "Prompt for branch name during creation")}</span>
        </div>
        <p class={s.hint}>{t("general.hint.promptOnCreate", "Show dialog when creating worktrees from \"+\" button. When off, creates instantly with auto-generated name")}</p>
      </div>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={repoDefaultsStore.state.deleteBranchOnRemove}
            onChange={(e) => repoDefaultsStore.setDeleteBranchOnRemove(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.deleteBranchOnRemove", "Delete local branch when removing worktree")}</span>
        </div>
      </div>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={repoDefaultsStore.state.autoArchiveMerged}
            onChange={(e) => repoDefaultsStore.setAutoArchiveMerged(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.autoArchiveMerged", "Auto-archive merged worktrees")}</span>
        </div>
        <p class={s.hint}>{t("general.hint.autoArchiveMerged", "Move worktree to archive directory when its PR is merged")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.orphanCleanup", "Orphan Worktree Cleanup")}</label>
        <select
          value={repoDefaultsStore.state.orphanCleanup}
          onChange={(e) => repoDefaultsStore.setOrphanCleanup(e.currentTarget.value as OrphanCleanup)}
        >
          <option value="ask">{t("general.orphanCleanup.ask", "Ask before removing")}</option>
          <option value="on">{t("general.orphanCleanup.on", "Auto-remove")}</option>
          <option value="off">{t("general.orphanCleanup.off", "Keep (mark as detached)")}</option>
        </select>
        <p class={s.hint}>{t("general.hint.orphanCleanup", "Handle worktrees whose branch was deleted")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.prMergeStrategy", "PR Merge Strategy")}</label>
        <select
          value={repoDefaultsStore.state.prMergeStrategy}
          onChange={(e) => repoDefaultsStore.setPrMergeStrategy(e.currentTarget.value as MergeStrategy)}
        >
          <option value="merge">{t("general.mergeStrategy.merge", "Merge")}</option>
          <option value="squash">{t("general.mergeStrategy.squash", "Squash")}</option>
          <option value="rebase">{t("general.mergeStrategy.rebase", "Rebase")}</option>
        </select>
        <p class={s.hint}>{t("general.hint.prMergeStrategy", "Default merge strategy for worktree branches")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.afterMerge", "After Merge Behavior")}</label>
        <select
          value={repoDefaultsStore.state.afterMerge}
          onChange={(e) => repoDefaultsStore.setAfterMerge(e.currentTarget.value as WorktreeAfterMerge)}
        >
          <option value="archive">{t("general.afterMerge.archive", "Archive worktree")}</option>
          <option value="delete">{t("general.afterMerge.delete", "Delete worktree")}</option>
          <option value="ask">{t("general.afterMerge.ask", "Ask each time")}</option>
        </select>
        <p class={s.hint}>{t("general.hint.afterMerge", "What to do with the worktree after merging its branch")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.autoFetchInterval", "Auto-Fetch Interval")}</label>
        <select
          value={String(repoDefaultsStore.state.autoFetchIntervalMinutes)}
          onChange={(e) => repoDefaultsStore.setAutoFetchIntervalMinutes(Number(e.currentTarget.value))}
        >
          <option value="0">{t("general.autoFetch.disabled", "Disabled")}</option>
          <option value="5">{t("general.autoFetch.5min", "5 minutes")}</option>
          <option value="15">{t("general.autoFetch.15min", "15 minutes")}</option>
          <option value="30">{t("general.autoFetch.30min", "30 minutes")}</option>
          <option value="60">{t("general.autoFetch.60min", "60 minutes")}</option>
        </select>
        <p class={s.hint}>{t("general.hint.autoFetchInterval", "Periodically fetch from remote to detect upstream changes")}</p>
      </div>

      <div class={s.group}>
        <label>{t("general.label.autoDeleteOnPrClose", "Auto-Delete on PR Close")}</label>
        <select
          value={repoDefaultsStore.state.autoDeleteOnPrClose}
          onChange={(e) => repoDefaultsStore.setAutoDeleteOnPrClose(e.currentTarget.value as AutoDeleteOnPrClose)}
        >
          <option value="off">{t("general.autoDelete.off", "Off")}</option>
          <option value="ask">{t("general.autoDelete.ask", "Ask before deleting")}</option>
          <option value="auto">{t("general.autoDelete.auto", "Auto-delete silently")}</option>
        </select>
        <p class={s.hint}>{t("general.hint.autoDeleteOnPrClose", "Delete local branch when its PR is merged or closed on GitHub")}</p>
      </div>

    </div>
  );
};
