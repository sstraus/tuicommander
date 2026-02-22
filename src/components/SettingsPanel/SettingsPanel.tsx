import { Component, Show, createEffect, createSignal } from "solid-js";
import { repoSettingsStore, type RepoSettings } from "../../stores/repoSettings";
import { repoDefaultsStore } from "../../stores/repoDefaults";
import { repositoriesStore } from "../../stores/repositories";
import { uiStore } from "../../stores/ui";
import { shortenHomePath } from "../../platform";
import { SettingsShell } from "./SettingsShell";
import type { SettingsShellTab } from "./SettingsShell";
import { DictationSettings } from "./DictationSettings";
import {
  GeneralTab,
  NotificationsTab,
  ServicesTab,
  AppearanceTab,
  KeyboardShortcutsTab,
  PluginsTab,
  AboutTab,
  RepoWorktreeTab,
  RepoScriptsTab,
} from "./tabs";
import { t } from "../../i18n";
import s from "./Settings.module.css";

/** Context for initial selection when opening the panel */
export type SettingsContext =
  | { kind: "global" }
  | { kind: "repo"; repoPath: string };

export interface SettingsPanelProps {
  visible: boolean;
  onClose: () => void;
  initialTab?: string;
  context?: SettingsContext;
}

const GLOBAL_TABS: SettingsShellTab[] = [
  { key: "general", label: t("settings.general", "General") },
  { key: "appearance", label: t("settings.appearance", "Appearance") },
  { key: "shortcuts", label: t("settings.keyboardShortcuts", "Keyboard Shortcuts") },
  { key: "notifications", label: t("settings.notifications", "Notifications") },
  { key: "dictation", label: t("settings.dictation", "Dictation") },
  { key: "services", label: t("settings.services", "Services") },
  { key: "plugins", label: t("settings.plugins", "Plugins") },
  { key: "about", label: t("settings.about", "About") },
];

function defaultTab(ctx: SettingsContext): string {
  if (ctx.kind === "repo") return `repo:${ctx.repoPath}`;
  return "general";
}

/** Build the full nav from global sections + configured repos */
function buildNavItems(): SettingsShellTab[] {
  const repos = repositoriesStore.state.repoOrder
    .map((path) => repositoriesStore.state.repositories[path])
    .filter(Boolean);

  const items: SettingsShellTab[] = [...GLOBAL_TABS];

  if (repos.length > 0) {
    items.push({ key: "__sep__", label: "─" });
    items.push({ key: "__label__:Repositories", label: t("settings.repositories", "REPOSITORIES") });
    for (const repo of repos) {
      const label = repo.displayName || repo.path.split("/").pop() || repo.path;
      const color =
        repoSettingsStore.get(repo.path)?.color ||
        repositoriesStore.getGroupForRepo(repo.path)?.color ||
        undefined;
      items.push({ key: `repo:${repo.path}`, label, color });
    }
  }

  return items;
}

export const SettingsPanel: Component<SettingsPanelProps> = (props) => {
  const ctx = () => props.context ?? { kind: "global" as const };
  const [activeTab, setActiveTab] = createSignal(props.initialTab ?? defaultTab(ctx()));

  // Reset active tab when context changes or panel opens
  createEffect(() => {
    if (props.visible) {
      setActiveTab(props.initialTab ?? defaultTab(ctx()));
    }
  });

  /** Repo path if a repo nav item is currently active, null otherwise */
  const activeRepoPath = (): string | null => {
    const tab = activeTab();
    return tab.startsWith("repo:") ? tab.slice(5) : null;
  };

  const repoSettings = (path: string) =>
    repoSettingsStore.getOrCreate(path, shortenHomePath(path));

  const updateRepoSetting =
    (repoPath: string) =>
    <K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => {
      repoSettingsStore.update(repoPath, { [key]: value });
      if (key === "displayName") {
        repositoriesStore.setDisplayName(repoPath, value as string);
      }
    };

  const footer = () => {
    const path = activeRepoPath();
    return (
      <div class={s.footer}>
        <Show when={path} fallback={<span />}>
          {(p) => (
            <button
              class={s.footerReset}
              onClick={() => repoSettingsStore.reset(p())}
            >
              {t("settings.resetToDefaults", "Reset to Defaults")}
            </button>
          )}
        </Show>
        <button class={s.footerDone} onClick={props.onClose}>
          {t("settings.done", "Done")}
        </button>
      </div>
    );
  };

  return (
    <SettingsShell
      visible={props.visible}
      onClose={props.onClose}
      title={t("settings.title", "Settings")}
      tabs={buildNavItems()}
      activeTab={activeTab()}
      onTabChange={setActiveTab}
      navWidth={uiStore.state.settingsNavWidth}
      onNavWidthChange={uiStore.setSettingsNavWidth}
      onNavWidthPersist={uiStore.persistUIPrefs}
      footer={footer()}
    >
      {/* Repo settings (shown when a repo nav item is active) */}
      <Show when={activeRepoPath()} keyed>
        {(path) => {
          const settings = repoSettings(path);
          const onUpdate = updateRepoSetting(path);
          return (
            <>
              <RepoWorktreeTab settings={settings} defaults={repoDefaultsStore.state} onUpdate={onUpdate} />
              <RepoScriptsTab settings={settings} defaults={repoDefaultsStore.state} onUpdate={onUpdate} />
            </>
          );
        }}
      </Show>

      {/* Global sections */}
      <Show when={activeTab() === "general"}>
        <GeneralTab />
      </Show>
      <Show when={activeTab() === "appearance"}>
        <AppearanceTab />
      </Show>
      <Show when={activeTab() === "notifications"}>
        <NotificationsTab />
      </Show>
      <Show when={activeTab() === "dictation"}>
        <DictationSettings />
      </Show>
      <Show when={activeTab() === "services"}>
        <ServicesTab />
      </Show>
      <Show when={activeTab() === "plugins"}>
        <PluginsTab />
      </Show>
      <Show when={activeTab() === "shortcuts"}>
        <KeyboardShortcutsTab />
      </Show>
      <Show when={activeTab() === "about"}>
        <AboutTab />
      </Show>
    </SettingsShell>
  );
};
