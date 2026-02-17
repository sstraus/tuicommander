import { createStore } from "solid-js/store";
import { invoke } from "../invoke";
import type { AgentType } from "../agents";

// Legacy storage keys for one-time migration
const LEGACY_KEYS = {
  IDE: "tui-commander-default-ide",
  AGENT: "tui-commander-agent",
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
}

// Default values
const DEFAULTS = {
  ide: "vscode" as const,
  font: "JetBrains Mono" as const,
  agent: "claude" as AgentType,
  fontSize: 12,
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

/** Valid agent values */
const VALID_AGENTS: readonly string[] = ["claude", "gemini", "opencode", "aider", "codex"];

/** Validate and return IDE type or default */
function validateIde(value: string | null): IdeType {
  return value && VALID_IDES.includes(value) ? (value as IdeType) : DEFAULTS.ide;
}

/** Validate and return font type or default */
function validateFont(value: string | null): FontType {
  return value && VALID_FONTS.includes(value as FontType) ? (value as FontType) : DEFAULTS.font;
}

/** Validate and return agent type or default */
function validateAgent(value: string | null): AgentType {
  return value && VALID_AGENTS.includes(value) ? (value as AgentType) : DEFAULTS.agent;
}

/** Split tab mode */
export type SplitTabMode = "separate" | "unified";

/** Settings store state */
interface SettingsStoreState {
  ide: IdeType;
  font: FontType;
  agent: AgentType;
  defaultFontSize: number;
  shell: string | null;
  theme: string;
  confirmBeforeQuit: boolean;
  confirmBeforeClosingTab: boolean;
  maxTabNameLength: number;
  splitTabMode: SplitTabMode;
}

/** Create the settings store */
function createSettingsStore() {
  const [state, setState] = createStore<SettingsStoreState>({
    ide: DEFAULTS.ide,
    font: DEFAULTS.font,
    agent: DEFAULTS.agent,
    defaultFontSize: DEFAULTS.fontSize,
    shell: null,
    theme: "vscode-dark",
    confirmBeforeQuit: true,
    confirmBeforeClosingTab: true,
    maxTabNameLength: 25,
    splitTabMode: "separate",
  });

  const actions = {
    /** Load settings from Rust config; migrate from localStorage on first run */
    async hydrate(): Promise<void> {
      try {
        // One-time migration from localStorage
        const legacyIde = localStorage.getItem(LEGACY_KEYS.IDE);
        const legacyAgent = localStorage.getItem(LEGACY_KEYS.AGENT);
        let migrated = false;

        if (legacyIde || legacyAgent) {
          try {
            const config = await invoke<RustAppConfig>("load_config");
            if (legacyIde) config.ide = legacyIde;
            await invoke("save_config", { config });
          } catch { /* ignore migration failure */ }
          if (legacyIde) localStorage.removeItem(LEGACY_KEYS.IDE);
          if (legacyAgent) localStorage.removeItem(LEGACY_KEYS.AGENT);
          migrated = true;
        }
        // Also clean up legacy session key
        localStorage.removeItem(LEGACY_KEYS.SESSION);

        const config = await invoke<RustAppConfig>("load_config");
        setState("font", validateFont(config.font_family));
        setState("ide", validateIde(config.ide));
        setState("defaultFontSize", config.default_font_size || DEFAULTS.fontSize);
        setState("shell", config.shell || null);
        setState("theme", config.theme || "tokyo-night");
        setState("confirmBeforeQuit", config.confirm_before_quit ?? true);
        setState("confirmBeforeClosingTab", config.confirm_before_closing_tab ?? true);
        setState("maxTabNameLength", config.max_tab_name_length || 25);
        setState("splitTabMode", config.split_tab_mode === "unified" ? "unified" : "separate");

        // Agent stored separately in agent-config
        if (!migrated && !legacyAgent) {
          const agentConfig = await invoke<{ primary_agent?: string }>("load_agent_config");
          if (agentConfig?.primary_agent) {
            setState("agent", validateAgent(agentConfig.primary_agent));
          }
        } else if (legacyAgent) {
          setState("agent", validateAgent(legacyAgent));
        }
      } catch (err) {
        console.error("Failed to hydrate settings:", err);
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

    /** Set agent preference */
    setAgent(agent: AgentType): void {
      setState("agent", agent);
      // Agent is persisted via agentFallback store's configure()
    },

    /** Set default font size */
    setDefaultFontSize(size: number): void {
      setState("defaultFontSize", Math.max(8, Math.min(32, size)));
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
