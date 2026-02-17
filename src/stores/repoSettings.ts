import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";

/** Per-repository settings */
export interface RepoSettings {
  path: string;
  displayName: string;
  baseBranch: string;
  copyIgnoredFiles: boolean;
  copyUntrackedFiles: boolean;
  setupScript: string;
  runScript: string;
}

/** Default repository settings — used only for creating new entries */
const DEFAULT_REPO_SETTINGS: Omit<RepoSettings, "path" | "displayName"> = {
  baseBranch: "automatic",
  copyIgnoredFiles: false,
  copyUntrackedFiles: false,
  setupScript: "",
  runScript: "",
};

/** Repository settings store state */
interface RepoSettingsState {
  settings: Record<string, RepoSettings>;
  activeRepoPath: string | null;
}

const LEGACY_STORAGE_KEY = "tui-commander-repo-settings";

/** Persist settings to Rust backend (fire-and-forget) */
function saveSettings(settings: Record<string, RepoSettings>): void {
  invoke("save_repo_settings", { config: { repos: settings } }).catch((err) =>
    console.debug("Failed to save repo settings:", err),
  );
}

/** Create repository settings store */
function createRepoSettingsStore() {
  const [state, setState] = createStore<RepoSettingsState>({
    settings: {},
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
        console.debug("Failed to hydrate repo settings:", err);
      }
    },

    /** Get settings for a repository */
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
        ...DEFAULT_REPO_SETTINGS,
      };

      setState("settings", path, newSettings);
      saveSettings(state.settings);

      return newSettings;
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
      } catch {
        return false;
      }
    },

    /** Reset repository settings to defaults */
    reset(path: string): void {
      if (!state.settings[path]) return;

      setState("settings", path, {
        ...state.settings[path],
        ...DEFAULT_REPO_SETTINGS,
      });
      saveSettings(state.settings);
    },
  };

  return { state, ...actions };
}

export const repoSettingsStore = createRepoSettingsStore();
