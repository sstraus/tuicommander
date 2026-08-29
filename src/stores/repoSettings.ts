import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";
import type {
	AutoDeleteOnPrClose,
	MergeStrategy,
	OrphanCleanup,
	WorktreeAfterMerge,
	WorktreeStorage,
} from "./repoDefaults";
import { repoDefaultsStore } from "./repoDefaults";
import { settingsStore } from "./settings";

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
	/** Gather every worktree of this repo into one consolidated screen (#e767).
	 *  Repo-specific, not inheritable — it describes how you want to look at THIS
	 *  repo, and a global default would consolidate repos you never asked about. */
	autoConsolidateWorktrees: boolean;
	/** null = inherit global default (true on macOS). When false: left-Option sends composition chars instead of meta sequences */
	terminalMetaHotkeys: boolean | null;
	/** null = inherit from repoDefaultsStore */
	worktreeStorage: WorktreeStorage | null;
	/** null = inherit from repoDefaultsStore */
	promptOnCreate: boolean | null;
	/** null = inherit from repoDefaultsStore */
	promptOnWorktreeSwitch: boolean | null;
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
	/** Allowlist of upstream MCP server names for this repo (null = all servers) */
	mcpUpstreams: string[] | null;
	/** null = inherit from settingsStore (global) */
	prHideDrafts: boolean | null;
	/** null = inherit from settingsStore (global) */
	prHideConflicting: boolean | null;
	/** null = inherit from settingsStore (global) */
	prHideCiFailing: boolean | null;
	/** Human-readable labels for branches/worktrees, keyed by branch name */
	branchLabels: Record<string, string>;
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
	promptOnWorktreeSwitch: boolean;
	deleteBranchOnRemove: boolean;
	autoArchiveMerged: boolean;
	orphanCleanup: OrphanCleanup;
	prMergeStrategy: MergeStrategy;
	afterMerge: WorktreeAfterMerge;
	autoFetchIntervalMinutes: number;
	autoDeleteOnPrClose: AutoDeleteOnPrClose;
	/** Resolved MCP upstream allowlist (null = all servers) */
	mcpUpstreams: string[] | null;
	prHideDrafts: boolean;
	prHideConflicting: boolean;
	prHideCiFailing: boolean;
	/** Human-readable labels for branches/worktrees, keyed by branch name */
	branchLabels: Record<string, string>;
}

/** Fields that can be overridden per-repo (all others are repo-specific) */
const OVERRIDABLE_NULL_DEFAULTS: Pick<
	RepoSettings,
	| "baseBranch"
	| "copyIgnoredFiles"
	| "copyUntrackedFiles"
	| "setupScript"
	| "runScript"
	| "archiveScript"
	| "terminalMetaHotkeys"
	| "worktreeStorage"
	| "promptOnCreate"
	| "promptOnWorktreeSwitch"
	| "deleteBranchOnRemove"
	| "autoArchiveMerged"
	| "orphanCleanup"
	| "prMergeStrategy"
	| "afterMerge"
	| "autoFetchIntervalMinutes"
	| "autoDeleteOnPrClose"
	| "mcpUpstreams"
	| "prHideDrafts"
	| "prHideConflicting"
	| "prHideCiFailing"
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
	promptOnWorktreeSwitch: null,
	deleteBranchOnRemove: null,
	autoArchiveMerged: null,
	orphanCleanup: null,
	prMergeStrategy: null,
	afterMerge: null,
	autoFetchIntervalMinutes: null,
	autoDeleteOnPrClose: null,
	mcpUpstreams: null,
	prHideDrafts: null,
	prHideConflicting: null,
	prHideCiFailing: null,
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
	mcp_upstreams?: string[];
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

/**
 * Per-field resolution of the three-tier chain (per-repo > .tuic.json > global).
 * One resolver per effective field, each reading only the signals its own field
 * needs — so a caller that wants `terminalMetaHotkeys` does not subscribe to the
 * ~50 signals the whole object touches. `getEffective` is built from this table,
 * so the two entry points can never disagree.
 */
type EffectiveResolvers = {
	[K in keyof EffectiveRepoSettings]: (
		settings: RepoSettings,
		local: () => RepoLocalConfig | null | undefined,
	) => EffectiveRepoSettings[K];
};

/** Create repository settings store */
function createRepoSettingsStore() {
	const [state, setState] = createStore<RepoSettingsState>({
		settings: {},
		localConfigs: {},
		activeRepoPath: null,
	});

	const resolvers: EffectiveResolvers = {
		path: (s) => s.path,
		displayName: (s) => s.displayName,
		color: (s) => s.color,
		baseBranch: (s, local) => s.baseBranch ?? local()?.base_branch ?? repoDefaultsStore.state.baseBranch,
		copyIgnoredFiles: (s, local) =>
			s.copyIgnoredFiles ?? local()?.copy_ignored_files ?? repoDefaultsStore.state.copyIgnoredFiles,
		copyUntrackedFiles: (s, local) =>
			s.copyUntrackedFiles ?? local()?.copy_untracked_files ?? repoDefaultsStore.state.copyUntrackedFiles,
		// SECURITY: .tuic.json scripts are NOT merged here — a malicious repo could
		// inject arbitrary shell commands via committed .tuic.json. Scripts must come
		// from per-repo user settings or global defaults only. A future trust-on-first-use
		// (TOFU) prompt will re-enable .tuic.json script inheritance.
		setupScript: (s) => s.setupScript ?? repoDefaultsStore.state.setupScript,
		runScript: (s) => s.runScript ?? repoDefaultsStore.state.runScript,
		archiveScript: (s) => s.archiveScript ?? repoDefaultsStore.state.archiveScript,
		terminalMetaHotkeys: (s) => s.terminalMetaHotkeys ?? true,
		worktreeStorage: (s, local) =>
			s.worktreeStorage ?? local()?.worktree_storage ?? repoDefaultsStore.state.worktreeStorage,
		promptOnCreate: (s) => s.promptOnCreate ?? repoDefaultsStore.state.promptOnCreate,
		promptOnWorktreeSwitch: (s) => s.promptOnWorktreeSwitch ?? repoDefaultsStore.state.promptOnWorktreeSwitch,
		deleteBranchOnRemove: (s, local) =>
			s.deleteBranchOnRemove ?? local()?.delete_branch_on_remove ?? repoDefaultsStore.state.deleteBranchOnRemove,
		autoArchiveMerged: (s, local) =>
			s.autoArchiveMerged ?? local()?.auto_archive_merged ?? repoDefaultsStore.state.autoArchiveMerged,
		orphanCleanup: (s, local) => s.orphanCleanup ?? local()?.orphan_cleanup ?? repoDefaultsStore.state.orphanCleanup,
		prMergeStrategy: (s, local) =>
			s.prMergeStrategy ?? local()?.pr_merge_strategy ?? repoDefaultsStore.state.prMergeStrategy,
		afterMerge: (s, local) => s.afterMerge ?? local()?.after_merge ?? repoDefaultsStore.state.afterMerge,
		autoFetchIntervalMinutes: (s) => s.autoFetchIntervalMinutes ?? repoDefaultsStore.state.autoFetchIntervalMinutes,
		autoDeleteOnPrClose: (s, local) =>
			s.autoDeleteOnPrClose ?? local()?.auto_delete_on_pr_close ?? repoDefaultsStore.state.autoDeleteOnPrClose,
		mcpUpstreams: (s, local) => s.mcpUpstreams ?? local()?.mcp_upstreams ?? null,
		prHideDrafts: (s) => s.prHideDrafts ?? settingsStore.state.prHideDrafts,
		prHideConflicting: (s) => s.prHideConflicting ?? settingsStore.state.prHideConflicting,
		prHideCiFailing: (s) => s.prHideCiFailing ?? settingsStore.state.prHideCiFailing,
		branchLabels: (s) => s.branchLabels ?? {},
	};

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
					} catch {
						/* ignore corrupt legacy data */
					}
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
				autoConsolidateWorktrees: false,
				branchLabels: {},
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

			const local = () => state.localConfigs[path];
			// Spelled out rather than looped so this stays one object literal with no
			// intermediate arrays. The resolution rules live only in `resolvers`, and
			// the literal must satisfy EffectiveRepoSettings, so neither can drift.
			return {
				path: resolvers.path(settings, local),
				displayName: resolvers.displayName(settings, local),
				color: resolvers.color(settings, local),
				baseBranch: resolvers.baseBranch(settings, local),
				copyIgnoredFiles: resolvers.copyIgnoredFiles(settings, local),
				copyUntrackedFiles: resolvers.copyUntrackedFiles(settings, local),
				setupScript: resolvers.setupScript(settings, local),
				runScript: resolvers.runScript(settings, local),
				archiveScript: resolvers.archiveScript(settings, local),
				terminalMetaHotkeys: resolvers.terminalMetaHotkeys(settings, local),
				worktreeStorage: resolvers.worktreeStorage(settings, local),
				promptOnCreate: resolvers.promptOnCreate(settings, local),
				promptOnWorktreeSwitch: resolvers.promptOnWorktreeSwitch(settings, local),
				deleteBranchOnRemove: resolvers.deleteBranchOnRemove(settings, local),
				autoArchiveMerged: resolvers.autoArchiveMerged(settings, local),
				orphanCleanup: resolvers.orphanCleanup(settings, local),
				prMergeStrategy: resolvers.prMergeStrategy(settings, local),
				afterMerge: resolvers.afterMerge(settings, local),
				autoFetchIntervalMinutes: resolvers.autoFetchIntervalMinutes(settings, local),
				autoDeleteOnPrClose: resolvers.autoDeleteOnPrClose(settings, local),
				mcpUpstreams: resolvers.mcpUpstreams(settings, local),
				prHideDrafts: resolvers.prHideDrafts(settings, local),
				prHideConflicting: resolvers.prHideConflicting(settings, local),
				prHideCiFailing: resolvers.prHideCiFailing(settings, local),
				branchLabels: resolvers.branchLabels(settings, local),
			};
		},

		/**
		 * Resolve a single effective field. Prefer this over `getEffective` in a
		 * reactive scope: it subscribes only to the signals that field's own
		 * inheritance chain reads, instead of all ~50 across the four stores.
		 */
		getEffectiveField<K extends keyof EffectiveRepoSettings>(
			path: string,
			field: K,
		): EffectiveRepoSettings[K] | undefined {
			const settings = state.settings[path];
			if (!settings) return undefined;
			return resolvers[field](settings, () => state.localConfigs[path]);
		},

		/** Update settings for a repository */
		update(path: string, updates: Partial<Omit<RepoSettings, "path">>): void {
			if (!state.settings[path]) return;

			setState("settings", path, { ...state.settings[path], ...updates });
			saveSettings(state.settings);
		},

		/** Set or clear a human-readable label for a branch/worktree. `null` removes it. */
		setLabel(repoPath: string, branchName: string, label: string | null): void {
			const current = state.settings[repoPath];
			if (!current) return;
			const next = { ...current.branchLabels };
			if (label?.trim()) {
				next[branchName] = label.trim();
			} else {
				delete next[branchName];
			}
			setState("settings", repoPath, "branchLabels", reconcile(next));
			invoke("set_branch_label", { repoPath, branchName, label: label?.trim() || null }).catch((err) =>
				appLogger.error("config", "Failed to set branch label", err),
			);
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
