import { batch } from "solid-js";
import { createStore } from "solid-js/store";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";

const LEGACY_SIDEBAR_VISIBLE_KEY = "tui-commander-sidebar-visible";
const LEGACY_SIDEBAR_WIDTH_KEY = "tui-commander-sidebar-width";

const SIDEBAR_MIN_WIDTH = 200;
const SIDEBAR_MAX_WIDTH = 500;
const SIDEBAR_DEFAULT_WIDTH = 300;

const MARKDOWN_PANEL_DEFAULT_WIDTH = 400;
const NOTES_PANEL_DEFAULT_WIDTH = 350;
const PLAN_PANEL_DEFAULT_WIDTH = 350;
const GIT_PANEL_DEFAULT_WIDTH = 380;
const SETTINGS_NAV_DEFAULT_WIDTH = 180;

/** Git panel tab names */
export type GitPanelTab = "changes" | "log" | "stashes" | "branches";

/** UI store state */
interface UIStoreState {
  // Sidebar visibility
  sidebarVisible: boolean;

  // Sidebar width
  sidebarWidth: number;

  // Panel visibility
  markdownPanelVisible: boolean;
  notesPanelVisible: boolean;
  fileBrowserPanelVisible: boolean;
  planPanelVisible: boolean;
  gitPanelVisible: boolean;

  // Requested active tab for the git panel (set by external actions like toggle-branches-tab)
  gitPanelRequestedTab: GitPanelTab | null;

  // Resizable panel widths (persisted)
  markdownPanelWidth: number;
  notesPanelWidth: number;
  planPanelWidth: number;
  gitPanelWidth: number;
  settingsNavWidth: number;

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
  const [state, setState] = createStore<UIStoreState>({
    sidebarVisible: true,
    sidebarWidth: SIDEBAR_DEFAULT_WIDTH,
    markdownPanelVisible: false,
    notesPanelVisible: false,
    fileBrowserPanelVisible: false,
    planPanelVisible: false,
    gitPanelVisible: false,
    gitPanelRequestedTab: null,
    markdownPanelWidth: MARKDOWN_PANEL_DEFAULT_WIDTH,
    notesPanelWidth: NOTES_PANEL_DEFAULT_WIDTH,
    planPanelWidth: PLAN_PANEL_DEFAULT_WIDTH,
    gitPanelWidth: GIT_PANEL_DEFAULT_WIDTH,
    settingsNavWidth: SETTINGS_NAV_DEFAULT_WIDTH,
    activeDropdown: null,
    isLoading: false,
    loadingMessage: "",
  });

  /** Persist all layout prefs to Rust backend (fire-and-forget) */
  function saveUIPrefs(): void {
    invoke("save_ui_prefs", {
      config: {
        sidebar_visible: state.sidebarVisible,
        sidebar_width: state.sidebarWidth,
        markdown_panel_visible: state.markdownPanelVisible,
        notes_panel_visible: state.notesPanelVisible,
        file_browser_panel_visible: state.fileBrowserPanelVisible,
        plan_panel_visible: state.planPanelVisible,
        git_panel_visible: state.gitPanelVisible,
        markdown_panel_width: state.markdownPanelWidth,
        notes_panel_width: state.notesPanelWidth,
        plan_panel_width: state.planPanelWidth,
        git_panel_width: state.gitPanelWidth,
        settings_nav_width: state.settingsNavWidth,
      },
    }).catch((err) => appLogger.debug("store", "Failed to save UI prefs", err));
  }

  /** Keys of the mutually exclusive right-side panels */
  type ExclusivePanel = "markdownPanelVisible" | "fileBrowserPanelVisible" | "planPanelVisible" | "gitPanelVisible";
  const exclusivePanels: ExclusivePanel[] = [
    "markdownPanelVisible",
    "fileBrowserPanelVisible",
    "planPanelVisible",
    "gitPanelVisible",
  ];

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
    /** Load UI prefs from Rust backend; migrate from localStorage on first run */
    async hydrate(): Promise<void> {
      try {
        // One-time migration from localStorage
        const legacyVisible = localStorage.getItem(LEGACY_SIDEBAR_VISIBLE_KEY);
        const legacyWidth = localStorage.getItem(LEGACY_SIDEBAR_WIDTH_KEY);
        if (legacyVisible !== null || legacyWidth !== null) {
          const visible = legacyVisible !== "false";
          const width = parseInt(legacyWidth || "", 10);
          const sidebarWidth = clampWidth(isNaN(width) ? SIDEBAR_DEFAULT_WIDTH : width);
          await invoke("save_ui_prefs", {
            config: { sidebar_visible: visible, sidebar_width: sidebarWidth },
          });
          localStorage.removeItem(LEGACY_SIDEBAR_VISIBLE_KEY);
          localStorage.removeItem(LEGACY_SIDEBAR_WIDTH_KEY);
        }

        const loaded = await invoke<{
          sidebar_visible?: boolean;
          sidebar_width?: number;
          markdown_panel_visible?: boolean;
          notes_panel_visible?: boolean;
          file_browser_panel_visible?: boolean;
          plan_panel_visible?: boolean;
          git_panel_visible?: boolean;
          markdown_panel_width?: number;
          notes_panel_width?: number;
          plan_panel_width?: number;
          git_panel_width?: number;
          settings_nav_width?: number;
        }>("load_ui_prefs");
        if (loaded) {
          if (loaded.sidebar_visible !== undefined) {
            setState("sidebarVisible", loaded.sidebar_visible);
          }
          if (loaded.sidebar_width !== undefined) {
            setState("sidebarWidth", clampWidth(loaded.sidebar_width));
          }
          if (loaded.markdown_panel_visible !== undefined) {
            setState("markdownPanelVisible", loaded.markdown_panel_visible);
          }
          if (loaded.notes_panel_visible !== undefined) {
            setState("notesPanelVisible", loaded.notes_panel_visible);
          }
          if (loaded.file_browser_panel_visible !== undefined) {
            setState("fileBrowserPanelVisible", loaded.file_browser_panel_visible);
          }
          if (loaded.plan_panel_visible !== undefined) {
            setState("planPanelVisible", loaded.plan_panel_visible);
          }
          if (loaded.git_panel_visible !== undefined) {
            setState("gitPanelVisible", loaded.git_panel_visible);
          }
          if (loaded.markdown_panel_width !== undefined) {
            setState("markdownPanelWidth", loaded.markdown_panel_width);
          }
          if (loaded.notes_panel_width !== undefined) {
            setState("notesPanelWidth", loaded.notes_panel_width);
          }
          if (loaded.plan_panel_width !== undefined) {
            setState("planPanelWidth", loaded.plan_panel_width);
          }
          if (loaded.git_panel_width !== undefined) {
            setState("gitPanelWidth", loaded.git_panel_width);
          }
          if (loaded.settings_nav_width !== undefined) {
            setState("settingsNavWidth", loaded.settings_nav_width);
          }
        }
      } catch (err) {
        appLogger.debug("store", "Failed to hydrate UI prefs", err);
      }
    },

    // Panel toggles — mutually exclusive
    toggleMarkdownPanel(): void {
      setExclusivePanel("markdownPanelVisible", !state.markdownPanelVisible);
    },

    setMarkdownPanelVisible(visible: boolean): void {
      setExclusivePanel("markdownPanelVisible", visible);
    },

    toggleNotesPanel(): void {
      setState("notesPanelVisible", (v) => !v);
      saveUIPrefs();
    },

    setNotesPanelVisible(visible: boolean): void {
      setState("notesPanelVisible", visible);
      saveUIPrefs();
    },

    toggleFileBrowserPanel(): void {
      setExclusivePanel("fileBrowserPanelVisible", !state.fileBrowserPanelVisible);
    },

    setFileBrowserPanelVisible(visible: boolean): void {
      setExclusivePanel("fileBrowserPanelVisible", visible);
    },

    togglePlanPanel(): void {
      setExclusivePanel("planPanelVisible", !state.planPanelVisible);
    },

    setPlanPanelVisible(visible: boolean): void {
      setExclusivePanel("planPanelVisible", visible);
    },

    toggleGitPanel(): void {
      setExclusivePanel("gitPanelVisible", !state.gitPanelVisible);
    },

    setGitPanelVisible(visible: boolean): void {
      setExclusivePanel("gitPanelVisible", visible);
    },

    /**
     * Open the git panel and switch to the given tab.
     * If the panel is already open on that tab, close it (toggle behaviour).
     */
    toggleGitPanelOnTab(tab: GitPanelTab): void {
      const alreadyOpenOnTab =
        state.gitPanelVisible && state.gitPanelRequestedTab === tab;
      if (alreadyOpenOnTab) {
        setExclusivePanel("gitPanelVisible", false);
      } else {
        setState("gitPanelRequestedTab", tab);
        setExclusivePanel("gitPanelVisible", true);
      }
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

    setSidebarVisible(visible: boolean): void {
      setState("sidebarVisible", visible);
      saveUIPrefs();
    },

    // Sidebar width
    setSidebarWidth(width: number): void {
      setState("sidebarWidth", clampWidth(width));
      saveUIPrefs();
    },

    // Panel widths
    setMarkdownPanelWidth(width: number): void {
      setState("markdownPanelWidth", width);
      saveUIPrefs();
    },

    setNotesPanelWidth(width: number): void {
      setState("notesPanelWidth", width);
      saveUIPrefs();
    },

    setPlanPanelWidth(width: number): void {
      setState("planPanelWidth", width);
      saveUIPrefs();
    },

    setGitPanelWidth(width: number): void {
      setState("gitPanelWidth", width);
      saveUIPrefs();
    },

    setSettingsNavWidth(width: number): void {
      setState("settingsNavWidth", width);
    },

    /** Persist current UI prefs to disk. Call after drag-end, not during drag. */
    persistUIPrefs(): void {
      saveUIPrefs();
    },

    /** Reset all panel and sidebar widths to defaults */
    resetLayout(): void {
      batch(() => {
        setState("sidebarWidth", SIDEBAR_DEFAULT_WIDTH);
        setState("markdownPanelWidth", MARKDOWN_PANEL_DEFAULT_WIDTH);
        setState("notesPanelWidth", NOTES_PANEL_DEFAULT_WIDTH);
        setState("planPanelWidth", PLAN_PANEL_DEFAULT_WIDTH);
        setState("gitPanelWidth", GIT_PANEL_DEFAULT_WIDTH);
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

  return { state, ...actions };
}

export const uiStore = createUIStore();
