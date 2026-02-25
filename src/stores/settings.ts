import { createStore } from "solid-js/store";
import { invoke } from "../invoke";
import { setLocale } from "../i18n";
import { appLogger } from "./appLogger";

// Legacy storage keys for one-time migration
const LEGACY_KEYS = {
  IDE: "tui-commander-default-ide",
  SESSION: "tui-commander-session",
} as const;

/** Rust AppConfig shape (subset needed for font/ide read/write) */
interface RustAppConfig {
  shell: string | null;
  font_family: string;
  font_size: number;
  theme: string;
  worktree_dir: string | null;
  mcp_server_enabled: boolean;
  ide: string;
  default_font_size: number;
  confirm_before_quit: boolean;
  confirm_before_closing_tab: boolean;
  max_tab_name_length: number;
  split_tab_mode: string;
  auto_show_pr_popover: boolean;
  prevent_sleep_when_busy: boolean;
  auto_update_enabled: boolean;
  language: string;
  update_channel: string;
  session_token_duration_secs: number;
  show_all_branches: boolean;
  disabled_agents: string[];
}

// Default values
const DEFAULTS = {
  ide: "vscode" as const,
  font: "JetBrains Mono" as const,
  fontSize: 13,
};

/** IDE options */
export type IdeType =
  | "vscode" | "cursor" | "zed" | "windsurf" | "neovim" | "xcode"
  | "ghostty" | "wezterm" | "alacritty" | "kitty" | "warp"
  | "sourcetree" | "github-desktop" | "fork" | "gitkraken" | "smerge"
  | "terminal" | "finder" | "editor";

/** Font family options (all bundled as woff2) */
export type FontType =
  | "JetBrains Mono"
  | "Fira Code"
  | "Hack"
  | "Cascadia Code"
  | "Iosevka"
  | "Source Code Pro"
  | "Inconsolata"
  | "IBM Plex Mono"
  | "Monaspace Neon"
  | "Commit Mono"
  | "Geist Mono";

/** IDE display names */
export const IDE_NAMES: Record<IdeType, string> = {
  vscode: "VS Code",
  cursor: "Cursor",
  zed: "Zed",
  windsurf: "Windsurf",
  neovim: "Neovim",
  xcode: "Xcode",
  ghostty: "Ghostty",
  wezterm: "WezTerm",
  alacritty: "Alacritty",
  kitty: "Kitty",
  warp: "Warp",
  sourcetree: "Sourcetree",
  "github-desktop": "GitHub Desktop",
  fork: "Fork",
  gitkraken: "GitKraken",
  smerge: "Sublime Merge",
  terminal: "Terminal",
  finder: "Finder",
  editor: "$EDITOR",
};

/** IDE icon SVG imports */
import vscodeSvg from "../assets/icons/vscode.svg";
import cursorSvg from "../assets/icons/cursor.svg";
import zedSvg from "../assets/icons/zed.svg";
import windsurfSvg from "../assets/icons/windsurf.svg";
import neovimSvg from "../assets/icons/neovim.svg";
import xcodeSvg from "../assets/icons/xcode.svg";
import ghosttySvg from "../assets/icons/ghostty.svg";
import weztermSvg from "../assets/icons/wezterm.svg";
import alacritySvg from "../assets/icons/alacritty.svg";
import kittySvg from "../assets/icons/kitty.svg";
import warpSvg from "../assets/icons/warp.svg";
import sourcetreeSvg from "../assets/icons/sourcetree.svg";
import githubDesktopSvg from "../assets/icons/github-desktop.svg";
import forkSvg from "../assets/icons/fork.svg";
import gitkrakenSvg from "../assets/icons/gitkraken.svg";
import smergeSvg from "../assets/icons/smerge.svg";
import terminalSvg from "../assets/icons/terminal.svg";
import finderSvg from "../assets/icons/finder.svg";
import editorSvg from "../assets/icons/editor.svg";

/** IDE icon paths (SVG) */
export const IDE_ICON_PATHS: Record<IdeType, string> = {
  vscode: vscodeSvg,
  cursor: cursorSvg,
  zed: zedSvg,
  windsurf: windsurfSvg,
  neovim: neovimSvg,
  xcode: xcodeSvg,
  ghostty: ghosttySvg,
  wezterm: weztermSvg,
  alacritty: alacritySvg,
  kitty: kittySvg,
  warp: warpSvg,
  sourcetree: sourcetreeSvg,
  "github-desktop": githubDesktopSvg,
  fork: forkSvg,
  gitkraken: gitkrakenSvg,
  smerge: smergeSvg,
  terminal: terminalSvg,
  finder: finderSvg,
  editor: editorSvg,
};

/** IDE icons (emoji fallbacks for text-only contexts) */
export const IDE_ICONS: Record<IdeType, string> = {
  vscode: "🔵",
  cursor: "🟣",
  zed: "⚡",
  windsurf: "🌊",
  neovim: "🟢",
  xcode: "🔨",
  ghostty: "👻",
  wezterm: "🟪",
  alacritty: "🔳",
  kitty: "🐱",
  warp: "🔷",
  sourcetree: "🌳",
  "github-desktop": "🐙",
  fork: "🔱",
  gitkraken: "🦑",
  smerge: "🔀",
  terminal: ">_",
  finder: "📁",
  editor: "$_",
};

/** IDE categories */
export const IDE_CATEGORIES: Record<string, IdeType[]> = {
  editors: ["vscode", "cursor", "zed", "windsurf", "neovim", "xcode", "editor"],
  terminals: ["ghostty", "wezterm", "alacritty", "kitty", "warp"],
  git: ["sourcetree", "github-desktop", "fork", "gitkraken", "smerge"],
  utilities: ["terminal", "finder"],
};

/** Font family CSS values (bundled font first, Nerd Font as optional override) */
export const FONT_FAMILIES: Record<FontType, string> = {
  "JetBrains Mono": '"JetBrainsMono Nerd Font", "JetBrains Mono", monospace',
  "Fira Code": '"FiraCode Nerd Font", "Fira Code", monospace',
  "Hack": '"Hack Nerd Font", "Hack", monospace',
  "Cascadia Code": '"CaskaydiaCove Nerd Font", "Cascadia Code", monospace',
  "Iosevka": '"Iosevka Nerd Font", "Iosevka", monospace',
  "Source Code Pro": '"SauceCodePro Nerd Font", "Source Code Pro", monospace',
  "Inconsolata": '"Inconsolata Nerd Font", "Inconsolata", monospace',
  "IBM Plex Mono": '"BlexMono Nerd Font", "IBM Plex Mono", monospace',
  "Monaspace Neon": '"Monaspace Neon", monospace',
  "Commit Mono": '"CommitMono Nerd Font", "Commit Mono", monospace',
  "Geist Mono": '"GeistMono Nerd Font", "Geist Mono", monospace',
};

/** Valid IDE values */
const VALID_IDES: readonly string[] = Object.keys(IDE_NAMES);

/** Valid font values */
const VALID_FONTS: readonly FontType[] = [
  "JetBrains Mono",
  "Fira Code",
  "Hack",
  "Cascadia Code",
  "Iosevka",
  "Source Code Pro",
  "Inconsolata",
  "IBM Plex Mono",
  "Monaspace Neon",
  "Commit Mono",
  "Geist Mono",
];

/** Validate and return IDE type or default */
function validateIde(value: string | null): IdeType {
  return value && VALID_IDES.includes(value) ? (value as IdeType) : DEFAULTS.ide;
}

/** Validate and return font type or default */
function validateFont(value: string | null): FontType {
  return value && VALID_FONTS.includes(value as FontType) ? (value as FontType) : DEFAULTS.font;
}


/** Split tab mode */
export type SplitTabMode = "separate" | "unified";

/** Update channel */
export type UpdateChannel = "stable" | "beta" | "nightly";

/** Settings store state */
interface SettingsStoreState {
  ide: IdeType;
  font: FontType;
  defaultFontSize: number;
  shell: string | null;
  theme: string;
  confirmBeforeQuit: boolean;
  confirmBeforeClosingTab: boolean;
  maxTabNameLength: number;
  splitTabMode: SplitTabMode;
  autoShowPrPopover: boolean;
  preventSleepWhenBusy: boolean;
  autoUpdateEnabled: boolean;
  language: string;
  updateChannel: UpdateChannel;
  showAllBranches: boolean;
  disabledAgents: string[];
}

/** Create the settings store */
function createSettingsStore() {
  const [state, setState] = createStore<SettingsStoreState>({
    ide: DEFAULTS.ide,
    font: DEFAULTS.font,
    defaultFontSize: DEFAULTS.fontSize,
    shell: null,
    theme: "commander",
    confirmBeforeQuit: true,
    confirmBeforeClosingTab: true,
    maxTabNameLength: 25,
    splitTabMode: "separate",
    autoShowPrPopover: true,
    preventSleepWhenBusy: false,
    autoUpdateEnabled: true,
    language: "en",
    updateChannel: "stable" as UpdateChannel,
    showAllBranches: false,
    disabledAgents: [],
  });

  const actions = {
    /** Load settings from Rust config; migrate from localStorage on first run */
    async hydrate(): Promise<void> {
      try {
        // One-time migration from localStorage
        const legacyIde = localStorage.getItem(LEGACY_KEYS.IDE);
        if (legacyIde) {
          try {
            const config = await invoke<RustAppConfig>("load_config");
            config.ide = legacyIde;
            await invoke("save_config", { config });
          } catch { /* ignore migration failure */ }
          localStorage.removeItem(LEGACY_KEYS.IDE);
        }
        // Clean up legacy session key
        localStorage.removeItem(LEGACY_KEYS.SESSION);

        const config = await invoke<RustAppConfig>("load_config");
        setState("font", validateFont(config.font_family));
        setState("ide", validateIde(config.ide));
        setState("defaultFontSize", config.default_font_size || DEFAULTS.fontSize);
        setState("shell", config.shell || null);
        setState("theme", config.theme || "vscode-dark");
        setState("confirmBeforeQuit", config.confirm_before_quit ?? true);
        setState("confirmBeforeClosingTab", config.confirm_before_closing_tab ?? true);
        setState("maxTabNameLength", config.max_tab_name_length || 25);
        setState("splitTabMode", config.split_tab_mode === "unified" ? "unified" : "separate");
        setState("autoShowPrPopover", config.auto_show_pr_popover ?? true);
        setState("preventSleepWhenBusy", config.prevent_sleep_when_busy ?? false);
        setState("autoUpdateEnabled", config.auto_update_enabled ?? true);
        setState("language", config.language || "en");
        setLocale(config.language || "en");
        const channel = config.update_channel;
        setState("updateChannel", (channel === "beta" || channel === "nightly") ? channel : "stable");
        setState("showAllBranches", config.show_all_branches ?? false);
        setState("disabledAgents", config.disabled_agents ?? []);
      } catch (err) {
        appLogger.error("config", "Failed to hydrate settings", err);
      }
    },

    /** Set IDE preference */
    async setIde(ide: IdeType): Promise<void> {
      const prevIde = state.ide;
      setState("ide", ide);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.ide = ide;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist IDE to config:", err);
        setState("ide", prevIde);
      }
    },

    /** Set font preference and persist to Rust config */
    async setFont(font: FontType): Promise<void> {
      const prevFont = state.font;
      setState("font", font);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.font_family = font;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist font to config:", err);
        setState("font", prevFont);
      }
    },

    /** Load font from Rust config (call on app startup) */
    async loadFontFromConfig(): Promise<void> {
      try {
        const config = await invoke<RustAppConfig>("load_config");
        const validated = validateFont(config.font_family);
        setState("font", validated);
      } catch (err) {
        console.error("Failed to load font from config:", err);
      }
    },

    /** Set default font size and persist to Rust config */
    async setDefaultFontSize(size: number): Promise<void> {
      const clamped = Math.max(8, Math.min(32, size));
      const prev = state.defaultFontSize;
      setState("defaultFontSize", clamped);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.default_font_size = clamped;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist defaultFontSize:", err);
        setState("defaultFontSize", prev);
      }
    },

    /** Set custom shell override (null = use system default) */
    async setShell(shell: string | null): Promise<void> {
      const value = shell?.trim() || null;
      const prevShell = state.shell;
      setState("shell", value);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.shell = value;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist shell to config:", err);
        setState("shell", prevShell);
      }
    },

    /** Set terminal theme */
    async setTheme(theme: string): Promise<void> {
      const prevTheme = state.theme;
      setState("theme", theme);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.theme = theme;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist theme to config:", err);
        setState("theme", prevTheme);
      }
    },

    /** Set confirm-before-quit preference */
    async setConfirmBeforeQuit(enabled: boolean): Promise<void> {
      const prevValue = state.confirmBeforeQuit;
      setState("confirmBeforeQuit", enabled);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.confirm_before_quit = enabled;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist confirmBeforeQuit:", err);
        setState("confirmBeforeQuit", prevValue);
      }
    },

    /** Set confirm-before-closing-tab preference */
    async setConfirmBeforeClosingTab(enabled: boolean): Promise<void> {
      const prevValue = state.confirmBeforeClosingTab;
      setState("confirmBeforeClosingTab", enabled);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.confirm_before_closing_tab = enabled;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist confirmBeforeClosingTab:", err);
        setState("confirmBeforeClosingTab", prevValue);
      }
    },

    /** Set split tab mode preference */
    async setSplitTabMode(mode: SplitTabMode): Promise<void> {
      const prevMode = state.splitTabMode;
      setState("splitTabMode", mode);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.split_tab_mode = mode;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist splitTabMode:", err);
        setState("splitTabMode", prevMode);
      }
    },

    /** Set auto-show PR popover preference */
    async setAutoShowPrPopover(enabled: boolean): Promise<void> {
      const prevValue = state.autoShowPrPopover;
      setState("autoShowPrPopover", enabled);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.auto_show_pr_popover = enabled;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist autoShowPrPopover:", err);
        setState("autoShowPrPopover", prevValue);
      }
    },

    /** Set prevent-sleep-when-busy preference */
    async setPreventSleepWhenBusy(enabled: boolean): Promise<void> {
      const prevValue = state.preventSleepWhenBusy;
      setState("preventSleepWhenBusy", enabled);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.prevent_sleep_when_busy = enabled;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist preventSleepWhenBusy:", err);
        setState("preventSleepWhenBusy", prevValue);
      }
    },

    /** Set auto-update-enabled preference */
    async setAutoUpdateEnabled(enabled: boolean): Promise<void> {
      const prevValue = state.autoUpdateEnabled;
      setState("autoUpdateEnabled", enabled);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.auto_update_enabled = enabled;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist autoUpdateEnabled:", err);
        setState("autoUpdateEnabled", prevValue);
      }
    },

    /** Set update channel preference */
    async setUpdateChannel(channel: UpdateChannel): Promise<void> {
      const prevChannel = state.updateChannel;
      setState("updateChannel", channel);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.update_channel = channel;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist updateChannel:", err);
        setState("updateChannel", prevChannel);
      }
    },

    /** Set UI language */
    async setLanguage(language: string): Promise<void> {
      const prevLang = state.language;
      setState("language", language);
      setLocale(language);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.language = language;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist language:", err);
        setState("language", prevLang);
        setLocale(prevLang);
      }
    },

    /** Set max tab name length and persist */
    async setMaxTabNameLength(length: number): Promise<void> {
      const clamped = Math.max(10, Math.min(60, length));
      const prev = state.maxTabNameLength;
      setState("maxTabNameLength", clamped);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.max_tab_name_length = clamped;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist maxTabNameLength:", err);
        setState("maxTabNameLength", prev);
      }
    },

    /** Set show-all-branches default */
    async setShowAllBranches(enabled: boolean): Promise<void> {
      const prevValue = state.showAllBranches;
      setState("showAllBranches", enabled);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.show_all_branches = enabled;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist showAllBranches:", err);
        setState("showAllBranches", prevValue);
      }
    },

    /** Toggle an agent's enabled/disabled state */
    async toggleAgent(agentType: string): Promise<void> {
      const prev = [...state.disabledAgents];
      const isDisabled = prev.includes(agentType);
      const next = isDisabled ? prev.filter((a) => a !== agentType) : [...prev, agentType];
      setState("disabledAgents", next);
      try {
        const config = await invoke<RustAppConfig>("load_config");
        config.disabled_agents = next;
        await invoke("save_config", { config });
      } catch (err) {
        console.error("Failed to persist disabledAgents:", err);
        setState("disabledAgents", prev);
      }
    },

    /** Check if an agent type is enabled */
    isAgentEnabled(agentType: string): boolean {
      return !state.disabledAgents.includes(agentType);
    },

    /** Get CSS font family string */
    getFontFamily(): string {
      return FONT_FAMILIES[state.font] || FONT_FAMILIES[DEFAULTS.font];
    },

    /** Get IDE display name */
    getIdeName(): string {
      return IDE_NAMES[state.ide] || IDE_NAMES[DEFAULTS.ide];
    },
  };

  return { state, ...actions };
}

export const settingsStore = createSettingsStore();
