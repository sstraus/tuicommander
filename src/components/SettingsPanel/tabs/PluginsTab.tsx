import { Component, For, Show, createSignal } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { pluginStore } from "../../../stores/pluginStore";
import { appLogger } from "../../../stores/appLogger";
import { registryStore, type RegistryEntry } from "../../../stores/registryStore";
import { mdTabsStore } from "../../../stores/mdTabs";
import { uiStore } from "../../../stores/ui";
import { invoke } from "../../../invoke";
import { isTauri } from "../../../transport";
import type { PluginState } from "../../../stores/pluginStore";
import type { LogEntry } from "../../../plugins/pluginLogger";
import s from "../Settings.module.css";
import ps from "./PluginsTab.module.css";

const PLUGIN_DOCS_URL = "https://github.com/sstraus/tuicommander/blob/main/docs/plugins.md";

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** Expandable log viewer for a single plugin */
const PluginLogViewer: Component<{ plugin: PluginState }> = (props) => {
  const entries = () => props.plugin.logger.getEntries();

  return (
    <div class={ps.logViewer}>
      <Show when={entries().length > 0} fallback={<p class={ps.logEmpty}>No log entries</p>}>
        <div class={ps.logEntries}>
          <For each={entries() as LogEntry[]}>
            {(entry) => (
              <div class={ps.logEntry} data-level={entry.level}>
                <span class={ps.logTime}>
                  {new Date(entry.timestamp).toLocaleTimeString()}
                </span>
                <span class={ps.logLevel}>{entry.level}</span>
                <span class={ps.logMessage}>{entry.message}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

/** Single plugin row in the installed list */
const PluginRow: Component<{ plugin: PluginState }> = (props) => {
  const [showLogs, setShowLogs] = createSignal(false);
  const [toggling, setToggling] = createSignal(false);
  const [uninstalling, setUninstalling] = createSignal(false);
  const [hasReadme, setHasReadme] = createSignal<boolean | null>(null);

  // Check if README.md exists for external plugins (lazy, on first render)
  if (!props.plugin.builtIn && isTauri()) {
    invoke<string | null>("get_plugin_readme_path", { id: props.plugin.id })
      .then((path) => setHasReadme(path !== null))
      .catch(() => setHasReadme(false));
  }

  const handleOpenReadme = async () => {
    const path = await invoke<string | null>("get_plugin_readme_path", { id: props.plugin.id });
    if (!path) return;
    mdTabsStore.add("", path);
    uiStore.setMarkdownPanelVisible(true);
  };

  const handleToggle = async () => {
    if (toggling()) return;
    setToggling(true);
    try {
      await pluginStore.setEnabled(props.plugin.id, !props.plugin.enabled);
    } finally {
      setToggling(false);
    }
  };

  const handleUninstall = async () => {
    if (props.plugin.builtIn || uninstalling()) return;
    if (!confirm(`Uninstall "${props.plugin.manifest?.name ?? props.plugin.id}"? This will remove all plugin files including data.`)) {
      return;
    }
    setUninstalling(true);
    try {
      await pluginStore.uninstall(props.plugin.id);
    } catch (err) {
      appLogger.error("plugin", `Failed to uninstall plugin "${props.plugin.id}"`, err);
    } finally {
      setUninstalling(false);
    }
  };

  const errorCount = () => props.plugin.logger.errorCount;
  const version = () => props.plugin.manifest?.version ?? "—";
  const description = () => props.plugin.manifest?.description ?? "";
  const capabilities = () => props.plugin.manifest?.capabilities ?? [];

  return (
    <div class={ps.pluginRow}>
      <div class={ps.pluginHeader}>
        <div class={ps.pluginInfo}>
          <div class={ps.pluginNameRow}>
            <span class={ps.pluginName}>{props.plugin.manifest?.name ?? props.plugin.id}</span>
            <span class={ps.pluginVersion}>v{version()}</span>
            <Show when={props.plugin.builtIn}>
              <span class={ps.badge} data-type="builtin">Built-in</span>
            </Show>
            <Show when={!props.plugin.builtIn && !props.plugin.loaded && props.plugin.enabled}>
              <span class={ps.badge} data-type="error">Error</span>
            </Show>
            <Show when={errorCount() > 0}>
              <span class={ps.badge} data-type="warning">{errorCount()} error{errorCount() > 1 ? "s" : ""}</span>
            </Show>
          </div>
          <Show when={description()}>
            <p class={ps.pluginDescription}>{description()}</p>
          </Show>
          <Show when={capabilities().length > 0}>
            <p class={ps.pluginCapabilities}>
              Capabilities: {capabilities().join(", ")}
            </p>
          </Show>
          <Show when={props.plugin.error}>
            <p class={ps.pluginError}>{props.plugin.error}</p>
          </Show>
        </div>

        <div class={ps.pluginActions}>
          <Show when={hasReadme()}>
            <button
              class={ps.docsBtn}
              onClick={handleOpenReadme}
              title="View plugin documentation"
            >
              ?
            </button>
          </Show>
          <label class={s.toggle}>
            <input
              type="checkbox"
              checked={props.plugin.enabled}
              disabled={toggling()}
              onChange={handleToggle}
            />
          </label>
          <button
            class={ps.logsBtn}
            classList={{ [ps.logsActive]: showLogs() }}
            onClick={() => setShowLogs(!showLogs())}
            title="View logs"
          >
            Logs
          </button>
          <Show when={!props.plugin.builtIn}>
            <button
              class={ps.uninstallBtn}
              onClick={handleUninstall}
              disabled={uninstalling()}
              title="Uninstall plugin"
            >
              {uninstalling() ? "..." : "Uninstall"}
            </button>
          </Show>
        </div>
      </div>

      <Show when={showLogs()}>
        <PluginLogViewer plugin={props.plugin} />
      </Show>
    </div>
  );
};

/** Single row in the Browse registry view */
const BrowseRow: Component<{ entry: RegistryEntry }> = (props) => {
  const [installing, setInstalling] = createSignal(false);

  const installed = () => pluginStore.getPlugin(props.entry.id);
  const isInstalled = () => !!installed();
  const updateAvailable = () => {
    const p = installed();
    if (!p?.manifest?.version) return false;
    return registryStore.hasUpdate(props.entry.id, p.manifest.version) !== null;
  };

  const handleInstall = async () => {
    if (installing()) return;
    setInstalling(true);
    try {
      await pluginStore.installFromUrl(props.entry.downloadUrl);
    } catch (err) {
      appLogger.error("plugin", `Failed to install plugin "${props.entry.id}"`, err);
      alert(`Installation failed: ${err}`);
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div class={ps.browseRow}>
      <div class={ps.pluginHeader}>
        <div class={ps.pluginInfo}>
          <div class={ps.pluginNameRow}>
            <span class={ps.pluginName}>{props.entry.name}</span>
            <span class={ps.pluginVersion}>v{props.entry.latestVersion}</span>
            <Show when={isInstalled() && !updateAvailable()}>
              <span class={ps.installedBadge}>Installed</span>
            </Show>
            <Show when={updateAvailable()}>
              <span class={ps.updateBadge}>Update available</span>
            </Show>
          </div>
          <Show when={props.entry.description}>
            <p class={ps.pluginDescription}>{props.entry.description}</p>
          </Show>
          <Show when={props.entry.author}>
            <p class={ps.pluginCapabilities}>by {props.entry.author}</p>
          </Show>
        </div>

        <div class={ps.pluginActions}>
          <Show when={!isInstalled() || updateAvailable()}>
            <button
              class={ps.installBtn}
              onClick={handleInstall}
              disabled={installing()}
            >
              {installing() ? "..." : updateAvailable() ? "Update" : "Install"}
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Main tab
// ---------------------------------------------------------------------------

export const PluginsTab: Component = () => {
  const plugins = () => pluginStore.getAll();
  const [installing, setInstalling] = createSignal(false);
  const [activeSubTab, setActiveSubTab] = createSignal<"installed" | "browse">("installed");

  // Fetch registry when Browse tab is first shown
  const handleBrowse = () => {
    setActiveSubTab("browse");
    registryStore.fetch();
  };

  const handleInstallFromFile = async () => {
    if (installing()) return;
    const selected = await open({
      title: "Install Plugin",
      filters: [{ name: "Plugin Archive", extensions: ["zip"] }],
      multiple: false,
    });
    if (!selected) return;

    setInstalling(true);
    try {
      await pluginStore.installFromZip(selected as string);
    } catch (err) {
      appLogger.error("plugin", "Failed to install plugin", err);
      alert(`Installation failed: ${err}`);
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div class={s.section}>
      <div class={ps.sectionHeader}>
        <h3>Plugins</h3>
        <button
          class={ps.docsLink}
          onClick={() => {
            if (isTauri()) {
              openUrl(PLUGIN_DOCS_URL).catch((err) => appLogger.error("plugin", "Failed to open URL", err));
            } else {
              window.open(PLUGIN_DOCS_URL, "_blank");
            }
          }}
          title="Open plugin authoring guide"
        >
          Documentation
        </button>
      </div>

      <div class={ps.subTabs}>
        <button
          class={ps.subTab}
          classList={{ [ps.subTabActive]: activeSubTab() === "installed" }}
          onClick={() => setActiveSubTab("installed")}
        >
          Installed
        </button>
        <button
          class={ps.subTab}
          classList={{ [ps.subTabActive]: activeSubTab() === "browse" }}
          onClick={handleBrowse}
        >
          Browse
        </button>
      </div>

      {/* Installed tab */}
      <Show when={activeSubTab() === "installed"}>
        <Show
          when={plugins().length > 0}
          fallback={
            <p class={s.hint}>No plugins installed. Place plugin folders in the plugins directory or install from a ZIP file.</p>
          }
        >
          <div class={ps.pluginList}>
            <For each={plugins() as PluginState[]}>
              {(plugin) => <PluginRow plugin={plugin} />}
            </For>
          </div>
        </Show>

        <Show when={isTauri()}>
          <div class={ps.installRow}>
            <button
              class={s.testBtn}
              onClick={handleInstallFromFile}
              disabled={installing()}
            >
              {installing() ? "Installing..." : "Install from file..."}
            </button>
          </div>
        </Show>
      </Show>

      {/* Browse tab */}
      <Show when={activeSubTab() === "browse"}>
        <div class={ps.browseHeader}>
          <p class={s.hint} style={{ margin: "0" }}>
            Discover plugins from the community registry.
          </p>
          <button
            class={ps.refreshBtn}
            onClick={() => registryStore.refresh()}
            disabled={registryStore.state.loading}
          >
            {registryStore.state.loading ? "Loading..." : "Refresh"}
          </button>
        </div>

        <Show when={registryStore.state.error}>
          <p class={ps.pluginError}>{registryStore.state.error}</p>
        </Show>

        <Show
          when={registryStore.state.entries.length > 0}
          fallback={
            <Show when={!registryStore.state.loading && !registryStore.state.error}>
              <p class={s.hint}>No plugins available in the registry yet.</p>
            </Show>
          }
        >
          <div class={ps.pluginList}>
            <For each={registryStore.state.entries}>
              {(entry) => <BrowseRow entry={entry} />}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
};
