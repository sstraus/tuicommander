import { Component, Show, createEffect, createSignal } from "solid-js";
import { repoSettingsStore, type RepoSettings } from "../../stores/repoSettings";
import { repositoriesStore } from "../../stores/repositories";
import { SettingsShell } from "./SettingsShell";
import type { SettingsShellTab } from "./SettingsShell";
import { DictationSettings } from "./DictationSettings";
import {
  GeneralTab,
  NotificationsTab,
  ServicesTab,
  RepoWorktreeTab,
  RepoScriptsTab,
} from "./tabs";

/** Context determines which tabs are shown */
export type SettingsContext =
  | { kind: "global" }
  | { kind: "repo"; repoPath: string; displayName: string };

export interface SettingsPanelProps {
  visible: boolean;
  onClose: () => void;
  initialTab?: string;
  context?: SettingsContext;
}

/** Legacy type alias for backward compatibility with App.tsx */
export type SettingsTab = "general" | "notifications" | "dictation" | "services";

const GLOBAL_TABS: SettingsShellTab[] = [
  { key: "general", label: "General" },
  { key: "notifications", label: "Notifications" },
  { key: "dictation", label: "Dictation" },
  { key: "services", label: "Services" },
];

const REPO_TABS: SettingsShellTab[] = [
  { key: "repo-worktree", label: "Worktree" },
  { key: "repo-scripts", label: "Scripts" },
];

/** Separator pseudo-tab rendered as a divider in the tab bar */
const SEPARATOR: SettingsShellTab = { key: "__sep__", label: "─" };

function buildTabs(ctx: SettingsContext): SettingsShellTab[] {
  if (ctx.kind === "repo") {
    return [...REPO_TABS, SEPARATOR, ...GLOBAL_TABS];
  }
  return GLOBAL_TABS;
}

function defaultTab(ctx: SettingsContext): string {
  return ctx.kind === "repo" ? "repo-worktree" : "general";
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

  const tabs = () => buildTabs(ctx());

  // Repo settings helpers (only used when kind=repo)
  const repoSettings = () => {
    const c = ctx();
    if (c.kind !== "repo") return null;
    return repoSettingsStore.getOrCreate(c.repoPath, c.displayName);
  };

  const updateRepoSetting = <K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => {
    const c = ctx();
    if (c.kind !== "repo") return;
    repoSettingsStore.update(c.repoPath, { [key]: value });
    // displayName lives in both stores — keep repositoriesStore in sync
    if (key === "displayName") {
      repositoriesStore.setDisplayName(c.repoPath, value as string);
    }
  };

  const repoFooter = () => {
    const c = ctx();
    if (c.kind !== "repo") return undefined;
    return (
      <div class="settings-footer">
        <button
          class="settings-footer-reset"
          onClick={() => repoSettingsStore.reset(c.repoPath)}
        >
          Reset to Defaults
        </button>
        <button class="settings-footer-done" onClick={props.onClose}>
          Done
        </button>
      </div>
    );
  };

  return (
    <SettingsShell
      visible={props.visible}
      onClose={props.onClose}
      title={ctx().kind === "repo" ? (ctx() as { displayName: string }).displayName : "Settings"}
      subtitle={ctx().kind === "repo" ? (ctx() as { repoPath: string }).repoPath : undefined}
      icon={ctx().kind === "repo" ? "📁" : undefined}
      tabs={tabs()}
      activeTab={activeTab()}
      onTabChange={setActiveTab}
      footer={repoFooter()}
    >
      {/* Repo tabs */}
      <Show when={activeTab() === "repo-worktree" && repoSettings()}>
        {(s) => <RepoWorktreeTab settings={s()} onUpdate={updateRepoSetting} />}
      </Show>
      <Show when={activeTab() === "repo-scripts" && repoSettings()}>
        {(s) => <RepoScriptsTab settings={s()} onUpdate={updateRepoSetting} />}
      </Show>

      {/* Global tabs */}
      <Show when={activeTab() === "general"}>
        <GeneralTab />
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
    </SettingsShell>
  );
};

export default SettingsPanel;
