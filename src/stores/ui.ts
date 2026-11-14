import { batch } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { settingsCatalogue } from "../generated/settingsCatalogue";
import { invoke } from "../invoke";
import { reportSettingsConflicts, settingsRefresh } from "../services/settingsRefresh";
import type { TaskViewPreference } from "../types/taskPreferences";
import { createConfigDeltaWriter } from "../utils/configDeltaWriter";
import { isSidebarDensityMode, nextSidebarDensityMode, type SidebarDensityMode } from "../utils/sidebarDensity";
import { appLogger } from "./appLogger";

const LEGACY_SIDEBAR_VISIBLE_KEY = "tui-commander-sidebar-visible";
const LEGACY_SIDEBAR_WIDTH_KEY = "tui-commander-sidebar-width";
const EDITOR_WRAP_KEY = "tui-commander-editor-wrap";

function agentLaunchCounts(repoPath: string, branchName?: string): Record<string, number> {
	try {
		const saved = JSON.parse(localStorage.getItem(agentLaunchKey(repoPath, branchName)) ?? "{}");
		return Object.fromEntries(
			Object.entries(saved).filter((entry): entry is [string, number] => typeof entry[1] === "number" && entry[1] > 0),
		);
	} catch {
		return {};
	}
}

function agentLaunchKey(repoPath: string, branchName?: string): string {
	return `tui-commander-agent-launches:${JSON.stringify([repoPath, branchName ?? null])}`;
}

function loadEditorWrapPrefs(): { text: boolean; code: boolean } {
	try {
		const saved: unknown = JSON.parse(localStorage.getItem(EDITOR_WRAP_KEY) ?? "null");
		if (saved && typeof saved === "object") {
			const values = saved as Record<string, unknown>;
			return {
				text: typeof values.text === "boolean" ? values.text : true,
				code: typeof values.code === "boolean" ? values.code : false,
			};
		}
	} catch {
		// An unavailable or malformed local preference keeps the normal defaults.
	}
	return { text: true, code: false };
}

export const SIDEBAR_MAX_WIDTH = 500;
export const SIDEBAR_DEFAULT_WIDTH = settingsCatalogue.fields["preferences.sidebar_width"].default;
/** The default is also the floor: the sidebar never gets narrower than it opens. */
export const SIDEBAR_MIN_WIDTH = SIDEBAR_DEFAULT_WIDTH;

const SETTINGS_NAV_DEFAULT_WIDTH = settingsCatalogue.fields["preferences.settings_nav_width"].default;

/** Debounce window for persisting UI prefs, matching settingsStore. */
const SAVE_DEBOUNCE_MS = 500;

/** Git panel tab names */
export type GitPanelTab = "changes" | "log" | "stashes" | "branches";

/** Diff viewer display mode */
export type DiffViewMode = "split" | "unified" | "scroll";

/** UI store state */
interface UIStoreState {
	tasksViewPreferences: Record<string, TaskViewPreference>;
	tasksPreferencesRevision: number;
	// Sidebar visibility
	sidebarVisible: boolean;

	/** Focus mode: hides sidebar, tab bar and side panels to maximize the active
	 *  tab's content area. Toolbar and StatusBar stay visible. Session-only
	 *  (not persisted) — toggled via Cmd+Alt+Enter. */
	focusMode: boolean;

	/** Repo filter: when true the sidebar shows only repositories that have at
	 *  least one open terminal. Session-only (not persisted) — toggled from the
	 *  toolbar filter icon next to the sidebar collapse button. */
	repoFilterActiveOnly: boolean;

	/** H view: when true the sidebar keeps only branches with a live harness
	 *  session (and their repositories and groups). Derived from the saved mode. */
	repoFilterHarnessOnly: boolean;

	// Sidebar width
	sidebarWidth: number;

	/** Sidebar layout: rich, compact or harness-only with rich rows. Persisted. */
	sidebarDensityMode: SidebarDensityMode;

	// Panel visibility
	markdownPanelVisible: boolean;
	ideasPanelVisible: boolean;
	fileBrowserPanelVisible: boolean;
	gitPanelVisible: boolean;

	outlinePanelVisible: boolean;
	referencesPanelVisible: boolean;
	aiChatPanelVisible: boolean;
	/** Rendered width of the docked AI Chat panel; 0 when closed or detached. Not persisted. */
	aiChatPanelMeasuredWidth: number;
	detachedPanels: Record<string, string>;

	/** Collapsed state of the GitHub panel sections, keyed by section id
	 *  (`my-prs`, `prs`, `issues`). A missing key means the section has never
	 *  been toggled and keeps its own default. */
	githubSectionCollapsed: Record<string, boolean>;

	// Requested active tab for the git panel (set by external actions like toggle-branches-tab)
	gitPanelRequestedTab: GitPanelTab | null;

	// Resizable panel widths (persisted). The right-side panels are resized by
	// PanelResizeHandle, which writes panel.style.width inline and keeps no
	// store state — only the sidebar and the settings nav are tracked here.
	settingsNavWidth: number;

	// Settings "expert mode" (persisted). Off = an unmodified expert control is
	// hidden; on = every control is shown regardless of its default/modified state.
	settingsExpertMode: boolean;

	// Diff viewer mode (persisted)
	diffViewMode: DiffViewMode;

	// File browser view mode (persisted)
	fileBrowserViewMode: "flat" | "tree";
	editorWrapText: boolean;
	editorWrapCode: boolean;

	// External root (ephemeral) — when set, FileBrowserPanel browses this absolute
	// path instead of the active repo. Used by "Open Folder…" / "Open Path…".
	fileBrowserExternalRoot: string | null;

	// Content-search request nonce (ephemeral) — bumped by the
	// `toggle-file-browser-content-search` shortcut (Cmd+Shift+F). FileBrowserPanel
	// watches it and switches its local searchMode to "content" + focuses the input.
	fileBrowserContentSearchNonce: number;

	// Active dropdown (mutually exclusive)
	activeDropdown: "ide" | "font" | "agent" | null;

	// Loading states
	isLoading: boolean;
	loadingMessage: string;
}

function clampWidth(v: number): number {
	return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, v));
}

/** Create the UI store */
function createUIStore() {
	const uiPrefsWriter = createConfigDeltaWriter<Record<string, unknown>>("save_ui_prefs");
	const editorWrapPrefs = loadEditorWrapPrefs();
	const [state, setState] = createStore<UIStoreState>({
		tasksViewPreferences: {},
		tasksPreferencesRevision: 0,
		sidebarVisible: true,
		focusMode: false,
		repoFilterActiveOnly: false,
		get repoFilterHarnessOnly() {
			return this.sidebarDensityMode === "harness";
		},
		sidebarWidth: SIDEBAR_DEFAULT_WIDTH,
		sidebarDensityMode: "rich" as SidebarDensityMode,
		markdownPanelVisible: false,
		ideasPanelVisible: false,
		fileBrowserPanelVisible: false,
		gitPanelVisible: false,
		outlinePanelVisible: false,
		referencesPanelVisible: false,
		aiChatPanelVisible: false,
		aiChatPanelMeasuredWidth: 0,
		detachedPanels: {} as Record<string, string>,
		githubSectionCollapsed: {} as Record<string, boolean>,
		gitPanelRequestedTab: null,
		settingsNavWidth: SETTINGS_NAV_DEFAULT_WIDTH,
		settingsExpertMode: false,
		diffViewMode: "split" as DiffViewMode,
		fileBrowserViewMode: "tree" as "flat" | "tree",
		editorWrapText: editorWrapPrefs.text,
		editorWrapCode: editorWrapPrefs.code,
		fileBrowserExternalRoot: null,
		fileBrowserContentSearchNonce: 0,
		activeDropdown: null,
		isLoading: false,
		loadingMessage: "",
	});

	let saveTimer: ReturnType<typeof setTimeout> | null = null;

	/** Debounced persist — coalesces a burst of panel toggles into one write.
	 *  Each `save_ui_prefs` costs a config read, the global write lock and an
	 *  atomic write on the Rust side, so `setExclusivePanel` must not pay one per
	 *  mutation. Mirrors `settingsStore.save()` and `paneLayoutStore.scheduleSave()`. */
	function saveUIPrefs(): void {
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(() => {
			saveTimer = null;
			writeUIPrefs();
		}, SAVE_DEBOUNCE_MS);
	}

	/** Keys of the mutually exclusive right-side panels, paired with their
	 *  backend (snake_case) pref field name. Single source of truth for the
	 *  exclusivity logic (`setExclusivePanel`) AND for save/load (`writeUIPrefs`,
	 *  `hydrate`) — extend this list, not the functions that read it, when
	 *  adding a new exclusive panel (story 653-4cdf). */
	const exclusivePanelPrefs = [
		{ stateKey: "markdownPanelVisible", backendKey: "markdown_panel_visible" },
		{ stateKey: "fileBrowserPanelVisible", backendKey: "file_browser_panel_visible" },
		{ stateKey: "gitPanelVisible", backendKey: "git_panel_visible" },
		{ stateKey: "outlinePanelVisible", backendKey: "outline_panel_visible" },
		{ stateKey: "referencesPanelVisible", backendKey: "references_panel_visible" },
		{ stateKey: "aiChatPanelVisible", backendKey: "ai_chat_panel_visible" },
		{ stateKey: "ideasPanelVisible", backendKey: "notes_panel_visible" },
	] as const;

	/** Keys of the mutually exclusive right-side panels */
	type ExclusivePanel = (typeof exclusivePanelPrefs)[number]["stateKey"];
	const exclusivePanels: ExclusivePanel[] = exclusivePanelPrefs.map((p) => p.stateKey);

	type UIPrefs = {
		sidebar_visible?: boolean;
		sidebar_width?: number;
		sidebar_density?: string;
		settings_nav_width?: number;
		settings_expert_mode?: boolean;
		diff_view_mode?: string;
		file_browser_view_mode?: string;
		detached_panels?: Record<string, string>;
		github_section_collapsed?: Record<string, boolean>;
		tasks_view_preferences?: Record<string, string>;
	} & Partial<Record<(typeof exclusivePanelPrefs)[number]["backendKey"], boolean>>;

	/** Write the current prefs to the Rust backend (fire-and-forget) */
	function uiPrefsSnapshot() {
		const panelVisibility: Record<string, boolean> = {};
		for (const { stateKey, backendKey } of exclusivePanelPrefs) {
			panelVisibility[backendKey] = state[stateKey];
		}
		return {
			sidebar_visible: state.sidebarVisible,
			sidebar_width: state.sidebarWidth,
			sidebar_density: state.sidebarDensityMode,
			...panelVisibility,
			settings_nav_width: state.settingsNavWidth,
			settings_expert_mode: state.settingsExpertMode,
			diff_view_mode: state.diffViewMode,
			file_browser_view_mode: state.fileBrowserViewMode,
			detached_panels: state.detachedPanels,
			github_section_collapsed: state.githubSectionCollapsed,
			tasks_view_preferences: Object.fromEntries(
				Object.entries(state.tasksViewPreferences).map(([key, value]) => [key, JSON.stringify(value)]),
			),
		};
	}

	function writeUIPrefs(): void {
		uiPrefsWriter.save(uiPrefsSnapshot()).catch((err) => appLogger.debug("store", "Failed to save UI prefs", err));
	}

	function applyPrefs(loaded: UIPrefs | null): void {
		const taskPreferences: Record<string, TaskViewPreference> = {};
		for (const [key, value] of Object.entries(loaded?.tasks_view_preferences ?? {})) {
			try {
				const parsed: unknown = JSON.parse(value);
				if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
					taskPreferences[key] = parsed as TaskViewPreference;
				}
			} catch (err) {
				appLogger.debug("store", "Failed to decode Tasks view preference", err);
			}
		}
		setState("tasksViewPreferences", reconcile(taskPreferences));
		setState("tasksPreferencesRevision", (revision) => revision + 1);
		if (loaded) {
			if (loaded.sidebar_visible !== undefined) {
				setState("sidebarVisible", loaded.sidebar_visible);
			}
			if (loaded.sidebar_width !== undefined) {
				setState("sidebarWidth", clampWidth(loaded.sidebar_width));
			}
			if (isSidebarDensityMode(loaded.sidebar_density)) {
				setState("sidebarDensityMode", loaded.sidebar_density);
			}
			for (const { stateKey, backendKey } of exclusivePanelPrefs) {
				const value = loaded[backendKey];
				if (value !== undefined) {
					setState(stateKey, value);
				}
			}
			if (loaded.settings_nav_width !== undefined) {
				setState("settingsNavWidth", loaded.settings_nav_width);
			}
			if (loaded.settings_expert_mode !== undefined) {
				setState("settingsExpertMode", loaded.settings_expert_mode);
			}
			if (
				loaded.diff_view_mode === "split" ||
				loaded.diff_view_mode === "unified" ||
				loaded.diff_view_mode === "scroll"
			) {
				setState("diffViewMode", loaded.diff_view_mode);
			}
			if (loaded.file_browser_view_mode === "flat" || loaded.file_browser_view_mode === "tree") {
				setState("fileBrowserViewMode", loaded.file_browser_view_mode);
			}
			if (loaded.detached_panels && typeof loaded.detached_panels === "object") {
				setState("detachedPanels", loaded.detached_panels);
			}
			if (loaded.github_section_collapsed && typeof loaded.github_section_collapsed === "object") {
				setState("githubSectionCollapsed", loaded.github_section_collapsed);
			}
		}
	}

	/** Open one exclusive panel and close the others, or close all if `key` is already open (toggle). */
	function setExclusivePanel(key: ExclusivePanel, visible: boolean): void {
		batch(() => {
			setState(key, visible);
			if (visible) {
				for (const k of exclusivePanels) {
					if (k !== key) setState(k, false);
				}
			}
		});
		saveUIPrefs();
	}

	const actions = {
		startRemoteSync(): () => void {
			return settingsRefresh.register({
				section: "preferences",
				read: () => invoke<ReturnType<typeof uiPrefsSnapshot>>("load_ui_prefs"),
				apply: (remote) => {
					const merged = uiPrefsWriter.adopt(remote ?? {}, uiPrefsSnapshot());
					const pendingSave = saveTimer !== null;
					if (saveTimer) {
						clearTimeout(saveTimer);
						saveTimer = null;
					}
					applyPrefs(merged.value);
					if (pendingSave && !merged.conflicts.length) saveUIPrefs();
					reportSettingsConflicts(merged.conflicts);
				},
			});
		},
		setTaskViewPreference(key: string, patch: TaskViewPreference): void {
			setState("tasksViewPreferences", key, (previous) => ({ ...previous, ...patch }));
			saveUIPrefs();
		},
		/** Load UI prefs from Rust backend; migrate from localStorage on first run */
		async hydrate(): Promise<void> {
			try {
				// One-time migration from localStorage
				const legacyVisible = localStorage.getItem(LEGACY_SIDEBAR_VISIBLE_KEY);
				const legacyWidth = localStorage.getItem(LEGACY_SIDEBAR_WIDTH_KEY);
				if (legacyVisible !== null || legacyWidth !== null) {
					const migrationBase = (await invoke<Record<string, unknown>>("load_ui_prefs")) ?? {};
					uiPrefsWriter.loaded(migrationBase);
					const visible = legacyVisible !== "false";
					const width = parseInt(legacyWidth || "", 10);
					const sidebarWidth = clampWidth(Number.isNaN(width) ? SIDEBAR_DEFAULT_WIDTH : width);
					await uiPrefsWriter.save({ ...migrationBase, sidebar_visible: visible, sidebar_width: sidebarWidth });
					localStorage.removeItem(LEGACY_SIDEBAR_VISIBLE_KEY);
					localStorage.removeItem(LEGACY_SIDEBAR_WIDTH_KEY);
				}

				const loaded = await invoke<UIPrefs>("load_ui_prefs");
				uiPrefsWriter.loaded(loaded ?? {});
				// H depends on live harnesses, which are not restored at startup.
				// Only launch resets it; a runtime preferences sync keeps the user's H.
				applyPrefs(loaded?.sidebar_density === "harness" ? { ...loaded, sidebar_density: "rich" } : loaded);
			} catch (err) {
				appLogger.debug("store", "Failed to hydrate UI prefs", err);
			}
		},

		// Sidebar density
		cycleSidebarDensityMode(): void {
			setState("sidebarDensityMode", nextSidebarDensityMode);
			saveUIPrefs();
		},

		// Diff view mode
		setDiffViewMode(mode: DiffViewMode): void {
			setState("diffViewMode", mode);
			saveUIPrefs();
		},

		// File browser view mode
		setFileBrowserViewMode(mode: "flat" | "tree"): void {
			setState("fileBrowserViewMode", mode);
			saveUIPrefs();
		},

		setEditorWrap(kind: "text" | "code", enabled: boolean): void {
			// Wrapping is a per-client view preference; backend UI prefs require a Rust restart.
			setState(kind === "text" ? "editorWrapText" : "editorWrapCode", enabled);
			try {
				localStorage.setItem(
					EDITOR_WRAP_KEY,
					JSON.stringify({ text: state.editorWrapText, code: state.editorWrapCode }),
				);
			} catch (err) {
				appLogger.warn("store", "Failed to persist editor wrapping", err);
			}
		},

		// External root (ephemeral, not persisted) — lets the file browser escape
		// repo scoping for "Open Folder…" / "Open Path…".
		setFileBrowserExternalRoot(path: string | null): void {
			setState("fileBrowserExternalRoot", path);
		},

		// Open the file browser (if not already) and ask it to enter content-search
		// mode. Bumping the nonce lets the panel react even when it's already visible.
		requestFileBrowserContentSearch(): void {
			setExclusivePanel("fileBrowserPanelVisible", true);
			setState("fileBrowserContentSearchNonce", (n) => n + 1);
		},

		// Panel toggles — mutually exclusive
		toggleMarkdownPanel(): void {
			setExclusivePanel("markdownPanelVisible", !state.markdownPanelVisible);
		},

		setMarkdownPanelVisible(visible: boolean): void {
			setExclusivePanel("markdownPanelVisible", visible);
		},

		toggleIdeasPanel(): void {
			setExclusivePanel("ideasPanelVisible", !state.ideasPanelVisible);
		},

		setIdeasPanelVisible(visible: boolean): void {
			setExclusivePanel("ideasPanelVisible", visible);
		},

		toggleFileBrowserPanel(): void {
			setExclusivePanel("fileBrowserPanelVisible", !state.fileBrowserPanelVisible);
		},

		setFileBrowserPanelVisible(visible: boolean): void {
			setExclusivePanel("fileBrowserPanelVisible", visible);
		},

		toggleGitPanel(): void {
			setExclusivePanel("gitPanelVisible", !state.gitPanelVisible);
		},

		setGitPanelVisible(visible: boolean): void {
			setExclusivePanel("gitPanelVisible", visible);
		},

		/** Open the git panel on the given tab; never closes it. */
		openGitPanelOnTab(tab: GitPanelTab): void {
			setState("gitPanelRequestedTab", tab);
			setExclusivePanel("gitPanelVisible", true);
		},

		/**
		 * Open the git panel and switch to the given tab.
		 * If the panel is already open on that tab, close it (toggle behaviour).
		 */
		toggleGitPanelOnTab(tab: GitPanelTab): void {
			const alreadyOpenOnTab = state.gitPanelVisible && state.gitPanelRequestedTab === tab;
			if (alreadyOpenOnTab) {
				setExclusivePanel("gitPanelVisible", false);
			} else {
				setState("gitPanelRequestedTab", tab);
				setExclusivePanel("gitPanelVisible", true);
			}
		},

		toggleOutlinePanel(): void {
			setExclusivePanel("outlinePanelVisible", !state.outlinePanelVisible);
		},

		setOutlinePanelVisible(visible: boolean): void {
			setExclusivePanel("outlinePanelVisible", visible);
		},

		toggleReferencesPanel(): void {
			setExclusivePanel("referencesPanelVisible", !state.referencesPanelVisible);
		},

		setReferencesPanelVisible(visible: boolean): void {
			setExclusivePanel("referencesPanelVisible", visible);
		},

		toggleAiChatPanel(): void {
			setExclusivePanel("aiChatPanelVisible", !state.aiChatPanelVisible);
		},

		setAiChatPanelMeasuredWidth(width: number): void {
			setState("aiChatPanelMeasuredWidth", width);
		},

		setAiChatPanelVisible(visible: boolean): void {
			setExclusivePanel("aiChatPanelVisible", visible);
		},

		setDetached(panelId: string, windowLabel: string): void {
			setState("detachedPanels", panelId, windowLabel);
			saveUIPrefs();
		},

		clearDetached(panelId: string): void {
			const { [panelId]: _, ...rest } = state.detachedPanels;
			setState("detachedPanels", reconcile(rest));
			saveUIPrefs();
		},

		isDetached(panelId: string): boolean {
			return panelId in state.detachedPanels;
		},

		/** Collapsed flag for a GitHub panel section, or undefined when the user
		 *  has never toggled it — the caller then applies its own default. */
		getGithubSectionCollapsed(sectionId: string): boolean | undefined {
			return state.githubSectionCollapsed[sectionId];
		},

		setGithubSectionCollapsed(sectionId: string, collapsed: boolean): void {
			setState("githubSectionCollapsed", sectionId, collapsed);
			saveUIPrefs();
		},

		// Dropdown management
		toggleIdeDropdown(): void {
			setState("activeDropdown", (v) => (v === "ide" ? null : "ide"));
		},

		toggleFontDropdown(): void {
			setState("activeDropdown", (v) => (v === "font" ? null : "font"));
		},

		toggleAgentDropdown(): void {
			setState("activeDropdown", (v) => (v === "agent" ? null : "agent"));
		},

		closeAllDropdowns(): void {
			setState("activeDropdown", null);
		},

		// Sidebar visibility
		toggleSidebar(): void {
			setState("sidebarVisible", (v) => !v);
			saveUIPrefs();
		},

		// Focus mode (session-only, not persisted)
		toggleFocusMode(): void {
			setState("focusMode", (v) => !v);
		},

		// Repo filter (session-only, not persisted)
		toggleRepoFilter(): void {
			setState("repoFilterActiveOnly", (v) => !v);
		},

		setRepoFilterActiveOnly(active: boolean): void {
			setState("repoFilterActiveOnly", active);
		},

		setRepoFilterHarnessOnly(active: boolean): void {
			if (active || state.sidebarDensityMode === "harness") {
				setState("sidebarDensityMode", active ? "harness" : "rich");
				saveUIPrefs();
			}
		},

		setSidebarVisible(visible: boolean): void {
			setState("sidebarVisible", visible);
			saveUIPrefs();
		},

		// Sidebar width
		setSidebarWidth(width: number): void {
			setState("sidebarWidth", clampWidth(width));
			saveUIPrefs();
		},

		setSettingsNavWidth(width: number): void {
			setState("settingsNavWidth", width);
		},

		setSettingsExpertMode(enabled: boolean): void {
			setState("settingsExpertMode", enabled);
			saveUIPrefs();
		},

		/** Persist current UI prefs to disk. Call after drag-end, not during drag. */
		persistUIPrefs(): void {
			saveUIPrefs();
		},

		/** Reset all panel and sidebar widths to defaults */
		resetLayout(): void {
			batch(() => {
				setState("sidebarWidth", SIDEBAR_DEFAULT_WIDTH);
				setState("settingsNavWidth", SETTINGS_NAV_DEFAULT_WIDTH);
			});
			saveUIPrefs();
		},

		// Loading state
		setLoading(loading: boolean, message?: string): void {
			batch(() => {
				setState("isLoading", loading);
				setState("loadingMessage", message || "");
			});
		},
	};

	return {
		state,
		...actions,
		recordAgentLaunch(repoPath: string, branchName: string, key: string): void {
			for (const branch of [undefined, branchName]) {
				const counts = agentLaunchCounts(repoPath, branch);
				counts[key] = (counts[key] ?? 0) + 1;
				try {
					localStorage.setItem(agentLaunchKey(repoPath, branch), JSON.stringify(counts));
				} catch (err) {
					appLogger.warn("store", "Failed to save agent launch usage", err);
				}
			}
		},
		rankAgentLaunches<T extends { key: string }>(repoPath: string, branchName: string, entries: T[]): T[] {
			const branch = agentLaunchCounts(repoPath, branchName);
			const counts = Object.keys(branch).length ? branch : agentLaunchCounts(repoPath);
			return entries.toSorted((a, b) => (counts[b.key] ?? 0) - (counts[a.key] ?? 0));
		},
		/** Write a pending debounced save now — the app is going away. */
		flushSave(): void {
			if (!saveTimer) return;
			clearTimeout(saveTimer);
			saveTimer = null;
			writeUIPrefs();
		},
		_testCancelPendingSave(): void {
			if (saveTimer) {
				clearTimeout(saveTimer);
				saveTimer = null;
			}
		},
	};
}

export const uiStore = createUIStore();

// Debug registry — expose UI panel state for MCP introspection
import { registerDebugSnapshot } from "./debugRegistry";

registerDebugSnapshot("ui", () => {
	const s = uiStore.state;
	return {
		sidebarVisible: s.sidebarVisible,
		focusMode: s.focusMode,
		sidebarWidth: s.sidebarWidth,
		markdownPanelVisible: s.markdownPanelVisible,
		ideasPanelVisible: s.ideasPanelVisible,
		fileBrowserPanelVisible: s.fileBrowserPanelVisible,
		gitPanelVisible: s.gitPanelVisible,
		aiChatPanelVisible: s.aiChatPanelVisible,
		diffViewMode: s.diffViewMode,
		editorWrapText: s.editorWrapText,
		editorWrapCode: s.editorWrapCode,
		isLoading: s.isLoading,
	};
});
