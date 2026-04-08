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
  font_weight: number;
  theme: string;
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
  disabled_agents: string[];
  intent_tab_title: boolean;
  suggest_followups: boolean;
  copy_on_select: boolean;
  bell_style: string;
  global_hotkey: string | null;
}

// Default values
const DEFAULTS = {
  ide: "vscode" as const,
  font: "JetBrains Mono" as const,
  fontSize: 13,
  fontWeight: 400,
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
export type UpdateChannel = "stable" | "nightly";

/** Settings store state */
interface SettingsStoreState {
  ide: IdeType;
  font: FontType;
  fontWeight: number;
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
  disabledAgents: string[];
  intentTabTitle: boolean;
  suggestFollowups: boolean;
  copyOnSelect: boolean;
  bellStyle: "none" | "visual" | "sound" | "both";
  globalHotkey: string | null;
}

const SAVE_DEBOUNCE_MS = 500;

/** Create the settings store */
function createSettingsStore() {
  const [state, setState] = createStore<SettingsStoreState>({
    ide: DEFAULTS.ide,
    font: DEFAULTS.font,
    fontWeight: DEFAULTS.fontWeight,
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
    disabledAgents: [],
    intentTabTitle: true,
    suggestFollowups: true,
    copyOnSelect: true,
    bellStyle: "visual",
    globalHotkey: null,
  });

  // Shadow copy of the last loaded config — preserves fields not tracked in SolidJS store
  // (e.g. session_token_duration_secs, mcp_server_enabled). Updated on hydrate.
  let baseConfig: RustAppConfig | null = null;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  /** Build a full RustAppConfig from current store state + base config fields */
  function buildConfig(): RustAppConfig {
    return {
      ...(baseConfig ?? {} as RustAppConfig),
      shell: state.shell,
      font_family: state.font,
      font_size: state.defaultFontSize,
      font_weight: state.fontWeight,
      theme: state.theme,
      ide: state.ide,
      default_font_size: state.defaultFontSize,
      confirm_before_quit: state.confirmBeforeQuit,
      confirm_before_closing_tab: state.confirmBeforeClosingTab,
      max_tab_name_length: state.maxTabNameLength,
      split_tab_mode: state.splitTabMode,
      auto_show_pr_popover: state.autoShowPrPopover,
      prevent_sleep_when_busy: state.preventSleepWhenBusy,
      auto_update_enabled: state.autoUpdateEnabled,
      language: state.language,
      update_channel: state.updateChannel,
      disabled_agents: [...state.disabledAgents],
      intent_tab_title: state.intentTabTitle,
      suggest_followups: state.suggestFollowups,
      copy_on_select: state.copyOnSelect,
      bell_style: state.bellStyle,
      global_hotkey: state.globalHotkey,
      session_token_duration_secs: baseConfig?.session_token_duration_secs ?? 86400,
      mcp_server_enabled: baseConfig?.mcp_server_enabled ?? true,
    };
  }

  /** Debounced save — coalesces rapid setting changes into a single IPC call */
  function save(): void {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      saveTimer = null;
      invoke("save_config", { config: buildConfig() }).catch((err: unknown) =>
        appLogger.error("config", "Failed to save config", err),
      );
    }, SAVE_DEBOUNCE_MS);
  }

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
        baseConfig = config;
        setState("font", validateFont(config.font_family));
        setState("fontWeight", config.font_weight || DEFAULTS.fontWeight);
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
        setState("updateChannel", channel === "nightly" ? channel : "stable");
        setState("disabledAgents", config.disabled_agents ?? []);
        setState("intentTabTitle", config.intent_tab_title ?? true);
        setState("copyOnSelect", config.copy_on_select ?? true);
        setState("bellStyle", (config.bell_style || "visual") as SettingsStoreState["bellStyle"]);
        setState("suggestFollowups", config.suggest_followups ?? true);
        setState("globalHotkey", config.global_hotkey ?? null);
      } catch (err) {
        appLogger.error("config", "Failed to hydrate settings", err);
      }
    },

    /** Set IDE preference */
    setIde(ide: IdeType): void {
      setState("ide", ide);
      save();
    },

    /** Set font preference */
    setFont(font: FontType): void {
      setState("font", font);
      save();
    },

    /** Set terminal font weight */
    setFontWeight(weight: number): void {
      setState("fontWeight", Math.max(100, Math.min(900, Math.round(weight / 100) * 100)));
      save();
    },

    /** Set default font size */
    setDefaultFontSize(size: number): void {
      setState("defaultFontSize", Math.max(8, Math.min(32, size)));
      save();
    },

    /** Set custom shell override (null = use system default) */
    setShell(shell: string | null): void {
      setState("shell", shell?.trim() || null);
      save();
    },

    /** Set terminal theme */
    setTheme(theme: string): void {
      setState("theme", theme);
      save();
    },

    /** Set confirm-before-quit preference */
    setConfirmBeforeQuit(enabled: boolean): void {
      setState("confirmBeforeQuit", enabled);
      save();
    },

    /** Set confirm-before-closing-tab preference */
    setConfirmBeforeClosingTab(enabled: boolean): void {
      setState("confirmBeforeClosingTab", enabled);
      save();
    },

    /** Set split tab mode preference */
    setSplitTabMode(mode: SplitTabMode): void {
      setState("splitTabMode", mode);
      save();
    },

    /** Set auto-show PR popover preference */
    setAutoShowPrPopover(enabled: boolean): void {
      setState("autoShowPrPopover", enabled);
      save();
    },

    /** Set prevent-sleep-when-busy preference */
    setPreventSleepWhenBusy(enabled: boolean): void {
      setState("preventSleepWhenBusy", enabled);
      save();
    },

    /** Set auto-update-enabled preference */
    setAutoUpdateEnabled(enabled: boolean): void {
      setState("autoUpdateEnabled", enabled);
      save();
    },

    /** Set update channel preference */
    setUpdateChannel(channel: UpdateChannel): void {
      setState("updateChannel", channel);
      save();
    },

    /** Set UI language */
    setLanguage(language: string): void {
      setState("language", language);
      setLocale(language);
      save();
    },

    /** Set max tab name length */
    setMaxTabNameLength(length: number): void {
      setState("maxTabNameLength", Math.max(10, Math.min(60, length)));
      save();
    },

    /** Toggle an agent's enabled/disabled state */
    toggleAgent(agentType: string): void {
      const isDisabled = state.disabledAgents.includes(agentType);
      setState("disabledAgents", isDisabled
        ? state.disabledAgents.filter((a) => a !== agentType)
        : [...state.disabledAgents, agentType]);
      save();
    },

    /** Check if an agent type is enabled */
    isAgentEnabled(agentType: string): boolean {
      return !state.disabledAgents.includes(agentType);
    },

    /** Set intent-as-tab-title preference */
    setIntentTabTitle(enabled: boolean): void {
      setState("intentTabTitle", enabled);
      save();
    },

    /** Set suggest-followups preference */
    setSuggestFollowups(enabled: boolean): void {
      setState("suggestFollowups", enabled);
      save();
    },

    /** Set copy-on-select preference */
    setCopyOnSelect(enabled: boolean): void {
      setState("copyOnSelect", enabled);
      save();
    },

    /** Set terminal bell style */
    setBellStyle(style: SettingsStoreState["bellStyle"]): void {
      setState("bellStyle", style);
      save();
    },

    /** Re-apply font from the last loaded config (no IPC — uses hydrate cache) */
    loadFontFromConfig(): void {
      if (baseConfig) {
        setState("font", validateFont(baseConfig.font_family));
      }
    },

    /** Set global OS-level hotkey (or clear with null) */
    async setGlobalHotkey(combo: string | null): Promise<void> {
      const prevValue = state.globalHotkey;
      setState("globalHotkey", combo);
      try {
        await invoke("set_global_hotkey", { combo });
      } catch (err) {
        appLogger.error("config", "Failed to set global hotkey", err);
        setState("globalHotkey", prevValue);
        throw err;
      }
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
