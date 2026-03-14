import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";
import { repoDefaultsStore } from "./repoDefaults";
import type { WorktreeStorage, OrphanCleanup, MergeStrategy, WorktreeAfterMerge, AutoDeleteOnPrClose } from "./repoDefaults";

/** Per-repository settings — overridable fields are nullable (null = inherit from global defaults) */
export interface RepoSettings {
  path: string;
  displayName: string;
  /** null = inherit from repoDefaultsStore */
  baseBranch: string | null;
  /** null = inherit from repoDefaultsStore */
  copyIgnoredFiles: boolean | null;
  /** null = inherit from repoDefaultsStore */
  copyUntrackedFiles: boolean | null;
  /** null = inherit from repoDefaultsStore */
  setupScript: string | null;
  /** null = inherit from repoDefaultsStore */
  runScript: string | null;
  /** null = inherit from repoDefaultsStore */
  archiveScript: string | null;
  color: string;
  /** null = inherit global default (true on macOS). When false: left-Option sends composition chars instead of meta sequences */
  terminalMetaHotkeys: boolean | null;
  /** null = inherit from repoDefaultsStore */
  worktreeStorage: WorktreeStorage | null;
  /** null = inherit from repoDefaultsStore */
  promptOnCreate: boolean | null;
  /** null = inherit from repoDefaultsStore */
  deleteBranchOnRemove: boolean | null;
  /** null = inherit from repoDefaultsStore */
  autoArchiveMerged: boolean | null;
  /** null = inherit from repoDefaultsStore */
  orphanCleanup: OrphanCleanup | null;
  /** null = inherit from repoDefaultsStore */
  prMergeStrategy: MergeStrategy | null;
  /** null = inherit from repoDefaultsStore */
  afterMerge: WorktreeAfterMerge | null;
  /** Auto-fetch interval in minutes (null = inherit, 0 = disabled) */
  autoFetchIntervalMinutes: number | null;
  /** Auto-delete local branch on PR merge/close (null = inherit, off/ask/auto) */
  autoDeleteOnPrClose: AutoDeleteOnPrClose | null;
}

/** Fully resolved settings with no nulls — use getEffective() to obtain */
export interface EffectiveRepoSettings {
  path: string;
  displayName: string;
  baseBranch: string;
  copyIgnoredFiles: boolean;
  copyUntrackedFiles: boolean;
  setupScript: string;
  runScript: string;
  archiveScript: string;
  color: string;
  terminalMetaHotkeys: boolean;
  worktreeStorage: WorktreeStorage;
  promptOnCreate: boolean;
  deleteBranchOnRemove: boolean;
  autoArchiveMerged: boolean;
  orphanCleanup: OrphanCleanup;
  prMergeStrategy: MergeStrategy;
  afterMerge: WorktreeAfterMerge;
  autoFetchIntervalMinutes: number;
  autoDeleteOnPrClose: AutoDeleteOnPrClose;
}

/** Fields that can be overridden per-repo (all others are repo-specific) */
const OVERRIDABLE_NULL_DEFAULTS: Pick<
  RepoSettings,
  | "baseBranch" | "copyIgnoredFiles" | "copyUntrackedFiles" | "setupScript" | "runScript" | "archiveScript"
  | "terminalMetaHotkeys" | "worktreeStorage" | "promptOnCreate" | "deleteBranchOnRemove"
  | "autoArchiveMerged" | "orphanCleanup" | "prMergeStrategy" | "afterMerge"
  | "autoFetchIntervalMinutes"
  | "autoDeleteOnPrClose"
> = {
  baseBranch: null,
  copyIgnoredFiles: null,
  copyUntrackedFiles: null,
  setupScript: null,
  runScript: null,
  archiveScript: null,
  terminalMetaHotkeys: null,
  worktreeStorage: null,
  promptOnCreate: null,
  deleteBranchOnRemove: null,
  autoArchiveMerged: null,
  orphanCleanup: null,
  prMergeStrategy: null,
  afterMerge: null,
  autoFetchIntervalMinutes: null,
  autoDeleteOnPrClose: null,
};

/** Repo-local config loaded from .tuic.json (team-shareable, snake_case from Rust) */
interface RepoLocalConfig {
  base_branch?: string;
  copy_ignored_files?: boolean;
  copy_untracked_files?: boolean;
  // Script fields intentionally omitted — unsafe without TOFU prompt.
  worktree_storage?: WorktreeStorage;
  delete_branch_on_remove?: boolean;
  auto_archive_merged?: boolean;
  orphan_cleanup?: OrphanCleanup;
  pr_merge_strategy?: MergeStrategy;
  after_merge?: WorktreeAfterMerge;
  auto_delete_on_pr_close?: AutoDeleteOnPrClose;
}

/** Repository settings store state */
interface RepoSettingsState {
  settings: Record<string, RepoSettings>;
  localConfigs: Record<string, RepoLocalConfig | null>;
  activeRepoPath: string | null;
}

const LEGACY_STORAGE_KEY = "tui-commander-repo-settings";

/** Persist settings to Rust backend (fire-and-forget) */
function saveSettings(settings: Record<string, RepoSettings>): void {
  invoke("save_repo_settings", { config: { repos: settings } }).catch((err) =>
    appLogger.error("config", "Failed to save repo settings", err),
  );
}

/** Create repository settings store */
function createRepoSettingsStore() {
  const [state, setState] = createStore<RepoSettingsState>({
    settings: {},
    localConfigs: {},
    activeRepoPath: null,
  });

  const actions = {
    /** Load settings from Rust backend; migrate from localStorage on first run */
    async hydrate(): Promise<void> {
      try {
        // One-time migration from localStorage
        const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
        if (legacy) {
          try {
            const parsed = JSON.parse(legacy);
            await invoke("save_repo_settings", { config: { repos: parsed } });
          } catch { /* ignore corrupt legacy data */ }
          localStorage.removeItem(LEGACY_STORAGE_KEY);
        }

        const loaded = await invoke<{ repos?: Record<string, RepoSettings> }>("load_repo_settings");
        if (loaded?.repos) {
          setState("settings", loaded.repos);
        }
      } catch (err) {
        appLogger.debug("config", "Failed to hydrate repo settings", err);
      }
    },

    /** Get raw settings for a repository (may contain nulls = inherited) */
    get(path: string): RepoSettings | undefined {
      return state.settings[path];
    },

    /** Get or create settings for a repository */
    getOrCreate(path: string, displayName: string): RepoSettings {
      if (state.settings[path]) {
        return state.settings[path];
      }

      const newSettings: RepoSettings = {
        path,
        displayName,
        color: "",
        ...OVERRIDABLE_NULL_DEFAULTS,
      };

      setState("settings", path, newSettings);
      saveSettings(state.settings);

      return newSettings;
    },

    /** Load .tuic.json for a repository and cache the result */
    async loadLocalConfig(path: string): Promise<void> {
      try {
        const config = await invoke<RepoLocalConfig | null>("load_repo_local_config", { repoPath: path });
        setState("localConfigs", path, config ?? null);
      } catch (err) {
        appLogger.debug("config", "Failed to load .tuic.json", { path, err });
        setState("localConfigs", path, null);
      }
    },

    /** Get fully resolved settings: per-repo > .tuic.json > global defaults */
    getEffective(path: string): EffectiveRepoSettings | undefined {
      const settings = state.settings[path];
      if (!settings) return undefined;

      const local = state.localConfigs[path];
      const defaults = repoDefaultsStore.state;
      return {
        path: settings.path,
        displayName: settings.displayName,
        color: settings.color,
        baseBranch: settings.baseBranch ?? local?.base_branch ?? defaults.baseBranch,
        copyIgnoredFiles: settings.copyIgnoredFiles ?? local?.copy_ignored_files ?? defaults.copyIgnoredFiles,
        copyUntrackedFiles: settings.copyUntrackedFiles ?? local?.copy_untracked_files ?? defaults.copyUntrackedFiles,
        // SECURITY: .tuic.json scripts are NOT merged here — a malicious repo could
        // inject arbitrary shell commands via committed .tuic.json. Scripts must come
        // from per-repo user settings or global defaults only. A future trust-on-first-use
        // (TOFU) prompt will re-enable .tuic.json script inheritance.
        setupScript: settings.setupScript ?? defaults.setupScript,
        runScript: settings.runScript ?? defaults.runScript,
        archiveScript: settings.archiveScript ?? defaults.archiveScript,
        terminalMetaHotkeys: settings.terminalMetaHotkeys ?? true,
        worktreeStorage: settings.worktreeStorage ?? local?.worktree_storage ?? defaults.worktreeStorage,
        promptOnCreate: settings.promptOnCreate ?? defaults.promptOnCreate,
        deleteBranchOnRemove: settings.deleteBranchOnRemove ?? local?.delete_branch_on_remove ?? defaults.deleteBranchOnRemove,
        autoArchiveMerged: settings.autoArchiveMerged ?? local?.auto_archive_merged ?? defaults.autoArchiveMerged,
        orphanCleanup: settings.orphanCleanup ?? local?.orphan_cleanup ?? defaults.orphanCleanup,
        prMergeStrategy: settings.prMergeStrategy ?? local?.pr_merge_strategy ?? defaults.prMergeStrategy,
        afterMerge: settings.afterMerge ?? local?.after_merge ?? defaults.afterMerge,
        autoFetchIntervalMinutes: settings.autoFetchIntervalMinutes ?? defaults.autoFetchIntervalMinutes,
        autoDeleteOnPrClose: settings.autoDeleteOnPrClose ?? local?.auto_delete_on_pr_close ?? defaults.autoDeleteOnPrClose,
      };
    },

    /** Update settings for a repository */
    update(path: string, updates: Partial<Omit<RepoSettings, "path">>): void {
      if (!state.settings[path]) return;

      setState("settings", path, { ...state.settings[path], ...updates });
      saveSettings(state.settings);
    },

    /** Remove settings for a repository */
    remove(path: string): void {
      const { [path]: _, ...rest } = state.settings;
      setState("settings", reconcile(rest));
      saveSettings(state.settings);

      if (state.activeRepoPath === path) {
        setState("activeRepoPath", null);
      }
    },

    /** Set active repository for settings panel */
    setActiveRepo(path: string | null): void {
      setState("activeRepoPath", path);
    },

    /** Get all configured repositories */
    getAll(): RepoSettings[] {
      return Object.values(state.settings);
    },

    /** Check if repository has custom settings (delegates to Rust backend) */
    async hasCustomSettings(path: string): Promise<boolean> {
      if (!state.settings[path]) return false;
      try {
        return await invoke<boolean>("check_has_custom_settings", { path });
      } catch (err) {
        appLogger.warn("config", "hasCustomSettings IPC failed", { path, err });
        return false;
      }
    },

    /** Reset per-repo overridable fields to null (inherits from global defaults) */
    reset(path: string): void {
      if (!state.settings[path]) return;

      setState("settings", path, {
        ...state.settings[path],
        ...OVERRIDABLE_NULL_DEFAULTS,
      });
      saveSettings(state.settings);
    },
  };

  return { state, ...actions };
}

export const repoSettingsStore = createRepoSettingsStore();
