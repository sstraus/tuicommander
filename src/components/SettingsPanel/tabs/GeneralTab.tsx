import { Component, For, Show, createSignal, onMount } from "solid-js";
import { settingsStore, IDE_NAMES } from "../../../stores/settings";
import { appLogger } from "../../../stores/appLogger";
import { updaterStore } from "../../../stores/updater";
import { invoke } from "../../../invoke";
import { isTauri } from "../../../transport";
import type { IdeType, UpdateChannel } from "../../../stores/settings";
import { t } from "../../../i18n";
import s from "../Settings.module.css";

interface CliStatus {
  installed: boolean;
  path: string | null;
  version_match: boolean;
  prompt_dismissed: boolean;
}

export const GeneralTab: Component = () => {
  const [cliStatus, setCliStatus] = createSignal<CliStatus | null>(null);
  const [cliInstalling, setCliInstalling] = createSignal(false);

  const refreshCliStatus = async () => {
    if (!isTauri()) return;
    try {
      const status = await invoke<CliStatus>("get_cli_status");
      setCliStatus(status);
    } catch (err) {
      appLogger.error("app", "Failed to get CLI status", err);
    }
  };

  onMount(refreshCliStatus);

  const handleInstallCli = async () => {
    setCliInstalling(true);
    try {
      await invoke<string>("install_cli");
      await refreshCliStatus();
    } catch (err) {
      appLogger.error("app", "Failed to install CLI", err);
    } finally {
      setCliInstalling(false);
    }
  };

  const handleUninstallCli = async () => {
    try {
      await invoke("uninstall_cli");
      await refreshCliStatus();
    } catch (err) {
      appLogger.error("app", "Failed to uninstall CLI", err);
    }
  };

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

      <Show when={isTauri() && cliStatus()}>
        <h3>{t("general.heading.cli", "Command Line Interface")}</h3>

        <div class={s.group}>
          <Show when={cliStatus()!.installed} fallback={
            <>
              <p class={s.hint}>
                {t("general.hint.cliNotInstalled", "Install the tuic command to control TUICommander from the terminal. Open files, manage sessions, and use it as a tmux replacement.")}
              </p>
              <button
                class={s.testBtn}
                onClick={handleInstallCli}
                disabled={cliInstalling()}
                style={{ "margin-top": "8px" }}
              >
                {cliInstalling()
                  ? t("general.btn.installing", "Installing...")
                  : t("general.btn.installCli", "Install tuic CLI")}
              </button>
            </>
          }>
            <p class={s.hint} style={{ color: "var(--success)" }}>
              {t("general.hint.cliInstalled", "Installed at {path}", { path: cliStatus()!.path ?? "/usr/local/bin/tuic" })}
              {!cliStatus()!.version_match && (
                <span style={{ color: "var(--warning, #e5c07b)", "margin-left": "8px" }}>
                  {t("general.hint.cliOutdated", "(update pending — restart to apply)")}
                </span>
              )}
            </p>
            <button
              class={s.testBtn}
              onClick={handleUninstallCli}
              style={{ "margin-top": "8px" }}
            >
              {t("general.btn.uninstallCli", "Uninstall")}
            </button>
          </Show>
        </div>
      </Show>

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

      <h3>{t("general.heading.terminal", "Terminal")}</h3>

      <div class={s.group}>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={settingsStore.state.copyOnSelect}
            onChange={(e) => settingsStore.setCopyOnSelect(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.copyOnSelect", "Copy on select")}</span>
        </div>
        <p class={s.hint}>{t("general.hint.copyOnSelect", "Automatically copy selected text to clipboard")}</p>
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
          <p class={s.hint} style={{ color: "var(--success)" }}>
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

      <h3>{t("general.heading.experimental", "Experimental Features")}</h3>

      <div class={s.group}>
        <p class={s.hint} style={{ color: "var(--warning, #e5c07b)" }}>
          {t("general.hint.experimentalWarning", "These features are under active development and may be unstable.")}
        </p>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={settingsStore.state.experimentalFeaturesEnabled}
            onChange={(e) => settingsStore.setExperimentalFeaturesEnabled(e.currentTarget.checked)}
          />
          <span>{t("general.toggle.experimentalFeatures", "Enable experimental features")}</span>
        </div>
        <p class={s.hint}>
          {t("general.hint.experimentalFeatures", "Opt in to features under active development. Individual options appear below when enabled.")}
        </p>
      </div>

      <Show when={settingsStore.state.experimentalFeaturesEnabled}>
        <div class={s.group}>
          <div class={s.toggle}>
            <input
              type="checkbox"
              checked={settingsStore.state.aiChatEnabled}
              onChange={(e) => settingsStore.setAiChatEnabled(e.currentTarget.checked)}
            />
            <span>{t("general.toggle.aiChat", "AI Chat")}</span>
          </div>
          <p class={s.hint}>
            {t("general.hint.aiChat", "Enable the AI Chat panel, keyboard shortcut, and command palette entry.")}
          </p>
        </div>
        <div class={s.group}>
          <div class={s.toggle}>
            <input
              type="checkbox"
              checked={settingsStore.state.scrollbackReflow}
              onChange={(e) => settingsStore.setScrollbackReflow(e.currentTarget.checked)}
            />
            <span>{t("general.toggle.scrollbackReflow", "Scrollback reflow")}</span>
          </div>
          <p class={s.hint}>
            {t("general.hint.scrollbackReflow", "Reflow scrollback history when the terminal width changes. Keeps history readable after opening/closing side panels. New terminals only.")}
          </p>
        </div>
      </Show>

    </div>
  );
};
