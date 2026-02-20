import { createStore } from "solid-js/store";
import { invoke } from "../invoke";

const LEGACY_SIDEBAR_VISIBLE_KEY = "tui-commander-sidebar-visible";
const LEGACY_SIDEBAR_WIDTH_KEY = "tui-commander-sidebar-width";

const SIDEBAR_MIN_WIDTH = 200;
const SIDEBAR_MAX_WIDTH = 500;
const SIDEBAR_DEFAULT_WIDTH = 300;

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

  // Diff panel state
  currentDiffRepo: string | null;

  // Dropdown visibility
  ideDropdownVisible: boolean;
  fontDropdownVisible: boolean;
  agentDropdownVisible: boolean;

  // Loading states
  isLoading: boolean;
  loadingMessage: string;

  // Plan file detected in terminal output
  planFilePath: string | null;
}

function clampWidth(v: number): number {
  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, v));
}

/** Persist sidebar prefs to Rust backend (fire-and-forget) */
function saveSidebarPrefs(visible: boolean, width: number): void {
  invoke("save_ui_prefs", {
    config: { sidebar_visible: visible, sidebar_width: width },
  }).catch((err) => console.debug("Failed to save UI prefs:", err));
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
    currentDiffRepo: null,
    ideDropdownVisible: false,
    fontDropdownVisible: false,
    agentDropdownVisible: false,
    isLoading: false,
    loadingMessage: "",
    planFilePath: null,
  });

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

        const loaded = await invoke<{ sidebar_visible?: boolean; sidebar_width?: number }>("load_ui_prefs");
        if (loaded) {
          if (loaded.sidebar_visible !== undefined) {
            setState("sidebarVisible", loaded.sidebar_visible);
          }
          if (loaded.sidebar_width !== undefined) {
            setState("sidebarWidth", clampWidth(loaded.sidebar_width));
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
      setState("ideDropdownVisible", (v) => !v);
      // Close others
      setState("fontDropdownVisible", false);
      setState("agentDropdownVisible", false);
    },

    toggleFontDropdown(): void {
      setState("fontDropdownVisible", (v) => !v);
      // Close others
      setState("ideDropdownVisible", false);
      setState("agentDropdownVisible", false);
    },

    toggleAgentDropdown(): void {
      setState("agentDropdownVisible", (v) => !v);
      // Close others
      setState("ideDropdownVisible", false);
      setState("fontDropdownVisible", false);
    },

    closeAllDropdowns(): void {
      setState("ideDropdownVisible", false);
      setState("fontDropdownVisible", false);
      setState("agentDropdownVisible", false);
    },

    // Sidebar visibility
    toggleSidebar(): void {
      const next = !state.sidebarVisible;
      setState("sidebarVisible", next);
      saveSidebarPrefs(next, state.sidebarWidth);
    },

    setSidebarVisible(visible: boolean): void {
      setState("sidebarVisible", visible);
      saveSidebarPrefs(visible, state.sidebarWidth);
    },

    // Sidebar width
    setSidebarWidth(width: number): void {
      const clamped = clampWidth(width);
      setState("sidebarWidth", clamped);
      saveSidebarPrefs(state.sidebarVisible, clamped);
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
