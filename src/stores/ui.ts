import { createStore } from "solid-js/store";
import { invoke } from "../invoke";

const LEGACY_SIDEBAR_VISIBLE_KEY = "tui-commander-sidebar-visible";
const LEGACY_SIDEBAR_WIDTH_KEY = "tui-commander-sidebar-width";

const SIDEBAR_MIN_WIDTH = 200;
const SIDEBAR_MAX_WIDTH = 500;
const SIDEBAR_DEFAULT_WIDTH = 300;

const DIFF_PANEL_DEFAULT_WIDTH = 400;
const MARKDOWN_PANEL_DEFAULT_WIDTH = 400;
const NOTES_PANEL_DEFAULT_WIDTH = 350;
const SETTINGS_NAV_DEFAULT_WIDTH = 180;

/** UI store state */
interface UIStoreState {
  // Sidebar visibility
  sidebarVisible: boolean;

  // Sidebar width
  sidebarWidth: number;

  // Panel visibility
  diffPanelVisible: boolean;
  markdownPanelVisible: boolean;
  notesPanelVisible: boolean;
  fileBrowserPanelVisible: boolean;

  // Resizable panel widths (persisted)
  diffPanelWidth: number;
  markdownPanelWidth: number;
  notesPanelWidth: number;
  settingsNavWidth: number;

  // Diff panel state
  currentDiffRepo: string | null;

  // Active dropdown (mutually exclusive)
  activeDropdown: "ide" | "font" | "agent" | null;

  // Loading states
  isLoading: boolean;
  loadingMessage: string;

  // Plan file detected in terminal output
  planFilePath: string | null;
}

function clampWidth(v: number): number {
  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, v));
}

/** Create the UI store */
function createUIStore() {
  const [state, setState] = createStore<UIStoreState>({
    sidebarVisible: true,
    sidebarWidth: SIDEBAR_DEFAULT_WIDTH,
    diffPanelVisible: false,
    markdownPanelVisible: false,
    notesPanelVisible: false,
    fileBrowserPanelVisible: false,
    diffPanelWidth: DIFF_PANEL_DEFAULT_WIDTH,
    markdownPanelWidth: MARKDOWN_PANEL_DEFAULT_WIDTH,
    notesPanelWidth: NOTES_PANEL_DEFAULT_WIDTH,
    settingsNavWidth: SETTINGS_NAV_DEFAULT_WIDTH,
    currentDiffRepo: null,
    activeDropdown: null,
    isLoading: false,
    loadingMessage: "",
    planFilePath: null,
  });

  /** Persist all layout prefs to Rust backend (fire-and-forget) */
  function saveUIPrefs(): void {
    invoke("save_ui_prefs", {
      config: {
        sidebar_visible: state.sidebarVisible,
        sidebar_width: state.sidebarWidth,
        diff_panel_width: state.diffPanelWidth,
        markdown_panel_width: state.markdownPanelWidth,
        notes_panel_width: state.notesPanelWidth,
        settings_nav_width: state.settingsNavWidth,
      },
    }).catch((err) => console.debug("Failed to save UI prefs:", err));
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
          diff_panel_width?: number;
          markdown_panel_width?: number;
          notes_panel_width?: number;
          settings_nav_width?: number;
        }>("load_ui_prefs");
        if (loaded) {
          if (loaded.sidebar_visible !== undefined) {
            setState("sidebarVisible", loaded.sidebar_visible);
          }
          if (loaded.sidebar_width !== undefined) {
            setState("sidebarWidth", clampWidth(loaded.sidebar_width));
          }
          if (loaded.diff_panel_width !== undefined) {
            setState("diffPanelWidth", loaded.diff_panel_width);
          }
          if (loaded.markdown_panel_width !== undefined) {
            setState("markdownPanelWidth", loaded.markdown_panel_width);
          }
          if (loaded.notes_panel_width !== undefined) {
            setState("notesPanelWidth", loaded.notes_panel_width);
          }
          if (loaded.settings_nav_width !== undefined) {
            setState("settingsNavWidth", loaded.settings_nav_width);
          }
        }
      } catch (err) {
        console.debug("Failed to hydrate UI prefs:", err);
      }
    },

    // Panel toggles — FB, MD, Diff are mutually exclusive
    toggleDiffPanel(): void {
      const next = !state.diffPanelVisible;
      setState("diffPanelVisible", next);
      if (next) {
        setState("markdownPanelVisible", false);
        setState("fileBrowserPanelVisible", false);
      }
    },

    toggleMarkdownPanel(): void {
      const next = !state.markdownPanelVisible;
      setState("markdownPanelVisible", next);
      if (next) {
        setState("diffPanelVisible", false);
        setState("fileBrowserPanelVisible", false);
      }
    },

    setDiffPanelVisible(visible: boolean): void {
      setState("diffPanelVisible", visible);
      if (visible) {
        setState("markdownPanelVisible", false);
        setState("fileBrowserPanelVisible", false);
      }
    },

    setMarkdownPanelVisible(visible: boolean): void {
      setState("markdownPanelVisible", visible);
      if (visible) {
        setState("diffPanelVisible", false);
        setState("fileBrowserPanelVisible", false);
      }
    },

    toggleNotesPanel(): void {
      setState("notesPanelVisible", (v) => !v);
    },

    setNotesPanelVisible(visible: boolean): void {
      setState("notesPanelVisible", visible);
    },

    toggleFileBrowserPanel(): void {
      const next = !state.fileBrowserPanelVisible;
      setState("fileBrowserPanelVisible", next);
      if (next) {
        setState("diffPanelVisible", false);
        setState("markdownPanelVisible", false);
      }
    },

    setFileBrowserPanelVisible(visible: boolean): void {
      setState("fileBrowserPanelVisible", visible);
      if (visible) {
        setState("diffPanelVisible", false);
        setState("markdownPanelVisible", false);
      }
    },

    // Diff repo selection
    setCurrentDiffRepo(path: string | null): void {
      setState("currentDiffRepo", path);
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
    setDiffPanelWidth(width: number): void {
      setState("diffPanelWidth", width);
      saveUIPrefs();
    },

    setMarkdownPanelWidth(width: number): void {
      setState("markdownPanelWidth", width);
      saveUIPrefs();
    },

    setNotesPanelWidth(width: number): void {
      setState("notesPanelWidth", width);
      saveUIPrefs();
    },

    setSettingsNavWidth(width: number): void {
      setState("settingsNavWidth", width);
      saveUIPrefs();
    },

    // Loading state
    setLoading(loading: boolean, message?: string): void {
      setState("isLoading", loading);
      setState("loadingMessage", message || "");
    },

    // Plan file
    setPlanFilePath(path: string | null): void {
      setState("planFilePath", path);
    },

    clearPlanFile(): void {
      setState("planFilePath", null);
    },
  };

  return { state, ...actions };
}

export const uiStore = createUIStore();
