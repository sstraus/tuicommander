import { Component, For, Show, createSignal } from "solid-js";
import { pluginStore } from "../../../stores/pluginStore";
import { setPluginEnabled } from "../../../plugins/pluginLoader";
import type { PluginState } from "../../../stores/pluginStore";
import type { LogEntry } from "../../../plugins/pluginLogger";
import s from "../Settings.module.css";
import ps from "./PluginsTab.module.css";

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

  const handleToggle = async () => {
    if (props.plugin.builtIn || toggling()) return;
    setToggling(true);
    try {
      await setPluginEnabled(props.plugin.id, !props.plugin.enabled);
    } finally {
      setToggling(false);
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
          <Show when={!props.plugin.builtIn}>
            <label class={s.toggle}>
              <input
                type="checkbox"
                checked={props.plugin.enabled}
                disabled={toggling()}
                onChange={handleToggle}
              />
            </label>
          </Show>
          <button
            class={ps.logsBtn}
            classList={{ [ps.logsActive]: showLogs() }}
            onClick={() => setShowLogs(!showLogs())}
            title="View logs"
          >
            Logs
          </button>
        </div>
      </div>

      <Show when={showLogs()}>
        <PluginLogViewer plugin={props.plugin} />
      </Show>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Main tab
// ---------------------------------------------------------------------------

export const PluginsTab: Component = () => {
  const plugins = () => pluginStore.getAll();

  return (
    <div class={s.section}>
      <h3>Plugins</h3>

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
    </div>
  );
};
