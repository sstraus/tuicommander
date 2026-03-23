import { Component, For, Show } from "solid-js";
import { settingsStore, IDE_NAMES } from "../../../stores/settings";
import { appLogger } from "../../../stores/appLogger";
import { updaterStore } from "../../../stores/updater";
import type { IdeType, UpdateChannel } from "../../../stores/settings";
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
          <option value="nightly">{t("general.channel.nightly", "Nightly")}</option>
        </select>
        <Show when={settingsStore.state.updateChannel !== "stable"}>
          <p class={s.hint} style={{ color: "var(--warning, #e5c07b)" }}>
            {t("general.hint.updateChannelWarning", "Nightly builds may be unstable")}
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
        <Show when={!updaterStore.state.available && !updaterStore.state.checking && !updaterStore.state.error && !updaterStore.state.noRelease}>
          <p class={s.hint}>{t("general.hint.latestVersion", "You are on the latest version")}</p>
        </Show>
        <Show when={updaterStore.state.noRelease}>
          <p class={s.hint} style={{ color: "var(--fg-muted)" }}>
            {t("general.hint.noRelease", "No {channel} releases published yet", { channel: settingsStore.state.updateChannel })}
          </p>
        </Show>
        <Show when={updaterStore.state.error}>
          <p class={s.hint} style={{ color: "var(--accent-red, #f44747)" }}>
            {updaterStore.state.error}
          </p>
        </Show>
      </div>


    </div>
  );
};
