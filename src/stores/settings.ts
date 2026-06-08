import { createStore } from "solid-js/store";
import { setLocale } from "../i18n";
import { invoke } from "../invoke";
import type { IssueFilterMode } from "../types";
import { appLogger } from "./appLogger";

// Legacy storage keys for one-time migration
const LEGACY_KEYS = {
	IDE: "tui-commander-default-ide",
	SESSION: "tui-commander-session",
} as const;

/** Rust AppConfig shape (subset needed for font/ide read/write) */
/** A user-defined launcher for the "Open in" menu (GH #71). */
export interface CustomLauncher {
	id: string;
	name: string;
	/** Executable: bare name (resolved on PATH) or absolute path. */
	executable: string;
	/** Args; each may contain {path}/{file}/{line}/{column} placeholders. */
	args: string[];
	enabled: boolean;
	/** Optional platform filter: "macos" | "windows" | "linux". undefined = all. */
	platform?: "macos" | "windows" | "linux";
}

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
	tab_ordering_mode: string;
	tab_cycling_all_types: boolean;
	auto_show_pr_popover: boolean;
	prevent_sleep_when_busy: boolean;
	auto_update_enabled: boolean;
	auto_update_plugins_enabled: boolean;
	language: string;
	update_channel: string;
	services: {
		auth: { session_token_duration_secs: number };
	};
	disabled_agents: string[];
	intent_tab_title: boolean;
	suggest_followups: boolean;
	copy_on_select: boolean;
	show_last_prompt: boolean;
	bell_style: string;
	global_hotkey: string | null;
	issue_filter?: string;
	pr_hide_drafts?: boolean;
	pr_hide_conflicting?: boolean;
	pr_hide_ci_failing?: boolean;
	experimental_features_enabled?: boolean;
	ai_chat_enabled?: boolean;
	ai_triage_enabled?: boolean;
	ai_watchers_enabled?: boolean;
	scrollback_reflow?: boolean;
	cursor_style?: string;
	terminal_renderer?: string;
	show_block_timestamps?: boolean;
	show_scrollbar_marks?: boolean;
	block_folding_enabled?: boolean;
	index_strategy?: string;
	standby_timeout_minutes?: number;
	custom_launchers?: CustomLauncher[];
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
	| "vscode"
	| "cursor"
	| "zed"
	| "windsurf"
	| "neovim"
	| "xcode"
	| "ghostty"
	| "wezterm"
	| "alacritty"
	| "kitty"
	| "warp"
	| "iterm2"
	| "sourcetree"
	| "github-desktop"
	| "fork"
	| "gitkraken"
	| "smerge"
	| "tower"
	| "intellij"
	| "pycharm"
	| "webstorm"
	| "goland"
	| "clion"
	| "phpstorm"
	| "rubymine"
	| "rider"
	| "datagrip"
	| "rustrover"
	| "android-studio"
	| "fleet"
	| "terminal"
	| "finder"
	| "editor";

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
	iterm2: "iTerm2",
	sourcetree: "Sourcetree",
	"github-desktop": "GitHub Desktop",
	fork: "Fork",
	gitkraken: "GitKraken",
	smerge: "Sublime Merge",
	tower: "Tower",
	intellij: "IntelliJ IDEA",
	pycharm: "PyCharm",
	webstorm: "WebStorm",
	goland: "GoLand",
	clion: "CLion",
	phpstorm: "PhpStorm",
	rubymine: "RubyMine",
	rider: "Rider",
	datagrip: "DataGrip",
	rustrover: "RustRover",
	"android-studio": "Android Studio",
	fleet: "Fleet",
	terminal: "Terminal",
	finder: "Finder",
	editor: "$EDITOR",
};

import alacritySvg from "../assets/icons/alacritty.svg";
import androidStudioSvg from "../assets/icons/android-studio.svg";
import clionSvg from "../assets/icons/clion.svg";
import cursorSvg from "../assets/icons/cursor.svg";
import datagripSvg from "../assets/icons/datagrip.svg";
import editorSvg from "../assets/icons/editor.svg";
import finderSvg from "../assets/icons/finder.svg";
import fleetSvg from "../assets/icons/fleet.svg";
import forkSvg from "../assets/icons/fork.svg";
import ghosttySvg from "../assets/icons/ghostty.svg";
import githubDesktopSvg from "../assets/icons/github-desktop.svg";
import gitkrakenSvg from "../assets/icons/gitkraken.svg";
import golandSvg from "../assets/icons/goland.svg";
import intellijSvg from "../assets/icons/intellij.svg";
import iterm2Svg from "../assets/icons/iterm2.svg";
import kittySvg from "../assets/icons/kitty.svg";
import neovimSvg from "../assets/icons/neovim.svg";
import phpstormSvg from "../assets/icons/phpstorm.svg";
import pycharmSvg from "../assets/icons/pycharm.svg";
import riderSvg from "../assets/icons/rider.svg";
import rubymineSvg from "../assets/icons/rubymine.svg";
import rustroverSvg from "../assets/icons/rustrover.svg";
import smergeSvg from "../assets/icons/smerge.svg";
import sourcetreeSvg from "../assets/icons/sourcetree.svg";
import terminalSvg from "../assets/icons/terminal.svg";
import towerSvg from "../assets/icons/tower.svg";
/** IDE icon SVG imports */
import vscodeSvg from "../assets/icons/vscode.svg";
import warpSvg from "../assets/icons/warp.svg";
import webstormSvg from "../assets/icons/webstorm.svg";
import weztermSvg from "../assets/icons/wezterm.svg";
import windsurfSvg from "../assets/icons/windsurf.svg";
import xcodeSvg from "../assets/icons/xcode.svg";
import zedSvg from "../assets/icons/zed.svg";

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
	iterm2: iterm2Svg,
	sourcetree: sourcetreeSvg,
	"github-desktop": githubDesktopSvg,
	fork: forkSvg,
	gitkraken: gitkrakenSvg,
	smerge: smergeSvg,
	tower: towerSvg,
	intellij: intellijSvg,
	pycharm: pycharmSvg,
	webstorm: webstormSvg,
	goland: golandSvg,
	clion: clionSvg,
	phpstorm: phpstormSvg,
	rubymine: rubymineSvg,
	rider: riderSvg,
	datagrip: datagripSvg,
	rustrover: rustroverSvg,
	"android-studio": androidStudioSvg,
	fleet: fleetSvg,
	terminal: terminalSvg,
	finder: finderSvg,
	editor: editorSvg,
};

/** IDE categories */
export const IDE_CATEGORIES: Record<string, IdeType[]> = {
	editors: ["vscode", "cursor", "zed", "windsurf", "neovim", "xcode", "editor"],
	jetbrains: [
		"intellij",
		"pycharm",
		"webstorm",
		"goland",
		"clion",
		"phpstorm",
		"rubymine",
		"rider",
		"datagrip",
		"rustrover",
		"android-studio",
		"fleet",
	],
	terminals: ["ghostty", "wezterm", "alacritty", "kitty", "warp", "iterm2"],
	git: ["sourcetree", "github-desktop", "fork", "gitkraken", "smerge", "tower"],
	utilities: ["terminal", "finder"],
};

/** Font family CSS values — system Nerd Font (if installed) → bundled font → bundled symbols → monospace */
export const FONT_FAMILIES: Record<FontType, string> = {
	"JetBrains Mono": '"JetBrainsMono Nerd Font", "JetBrains Mono", "Symbols Nerd Font Mono", monospace',
	"Fira Code": '"FiraCode Nerd Font", "Fira Code", "Symbols Nerd Font Mono", monospace',
	Hack: '"Hack Nerd Font", "Hack", "Symbols Nerd Font Mono", monospace',
	"Cascadia Code": '"CaskaydiaCove Nerd Font", "Cascadia Code", "Symbols Nerd Font Mono", monospace',
	Iosevka: '"Iosevka Nerd Font", "Iosevka", "Symbols Nerd Font Mono", monospace',
	"Source Code Pro": '"SauceCodePro Nerd Font", "Source Code Pro", "Symbols Nerd Font Mono", monospace',
	Inconsolata: '"Inconsolata Nerd Font", "Inconsolata", "Symbols Nerd Font Mono", monospace',
	"IBM Plex Mono": '"BlexMono Nerd Font", "IBM Plex Mono", "Symbols Nerd Font Mono", monospace',
	"Monaspace Neon": '"Monaspace Neon", "Symbols Nerd Font Mono", monospace',
	"Commit Mono": '"CommitMono Nerd Font", "Commit Mono", "Symbols Nerd Font Mono", monospace',
	"Geist Mono": '"GeistMono Nerd Font", "Geist Mono", "Symbols Nerd Font Mono", monospace',
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

/** Valid issue filter values */
const VALID_ISSUE_FILTERS: readonly IssueFilterMode[] = ["assigned", "created", "mentioned", "all", "disabled"];

/** Validate and return issue filter or default */
function validateIssueFilter(value: string | null): IssueFilterMode {
	return value && (VALID_ISSUE_FILTERS as readonly string[]).includes(value) ? (value as IssueFilterMode) : "assigned";
}

/** Valid terminal renderer values */
const VALID_RENDERERS: readonly TerminalRenderer[] = ["webgl", "canvas", "native"];

function validateTerminalRenderer(value: string | null): TerminalRenderer {
	return value && (VALID_RENDERERS as readonly string[]).includes(value) ? (value as TerminalRenderer) : "webgl";
}

/** Split tab mode */
export type SplitTabMode = "separate" | "unified";

/** Tab ordering mode */
export type TabOrderingMode = "grouped-by-type" | "terminals-first" | "free";

/** Terminal renderer backend */
export type TerminalRenderer = "webgl" | "canvas" | "native";

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
	tabOrderingMode: TabOrderingMode;
	tabCyclingAllTypes: boolean;
	autoShowPrPopover: boolean;
	preventSleepWhenBusy: boolean;
	autoUpdateEnabled: boolean;
	autoUpdatePluginsEnabled: boolean;
	language: string;
	updateChannel: UpdateChannel;
	disabledAgents: string[];
	intentTabTitle: boolean;
	suggestFollowups: boolean;
	copyOnSelect: boolean;
	showLastPrompt: boolean;
	bellStyle: "none" | "visual" | "sound" | "both";
	globalHotkey: string | null;
	issueFilter: IssueFilterMode;
	prHideDrafts: boolean;
	prHideConflicting: boolean;
	prHideCiFailing: boolean;
	experimentalFeaturesEnabled: boolean;
	aiChatEnabled: boolean;
	aiTriageEnabled: boolean;
	aiWatchersEnabled: boolean;
	scrollbackReflow: boolean;
	cursorStyle: "bar" | "block" | "underline";
	terminalRenderer: TerminalRenderer;
	showBlockTimestamps: boolean;
	showScrollbarMarks: boolean;
	blockFoldingEnabled: boolean;
	indexStrategy: "disabled" | "active_only" | "active_and_switch" | "all_sequential";
	standbyTimeoutMinutes: number;
	customLaunchers: CustomLauncher[];
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
		tabOrderingMode: "grouped-by-type",
		tabCyclingAllTypes: false,
		autoShowPrPopover: true,
		preventSleepWhenBusy: false,
		autoUpdateEnabled: true,
		autoUpdatePluginsEnabled: true,
		language: "en",
		updateChannel: "stable" as UpdateChannel,
		disabledAgents: [],
		intentTabTitle: true,
		suggestFollowups: true,
		copyOnSelect: true,
		showLastPrompt: true,
		bellStyle: "visual",
		globalHotkey: null,
		issueFilter: "assigned",
		prHideDrafts: false,
		prHideConflicting: false,
		prHideCiFailing: false,
		experimentalFeaturesEnabled: false,
		aiChatEnabled: false,
		aiTriageEnabled: false,
		aiWatchersEnabled: false,
		scrollbackReflow: false,
		cursorStyle: "bar" as SettingsStoreState["cursorStyle"],
		terminalRenderer: "webgl",
		showBlockTimestamps: true,
		showScrollbarMarks: true,
		blockFoldingEnabled: true,
		indexStrategy: "active_and_switch",
		standbyTimeoutMinutes: 5,
		customLaunchers: [],
	});

	// Shadow copy of the last loaded config — preserves fields not tracked in SolidJS store
	// (e.g. session_token_duration_secs, mcp_server_enabled). Updated on hydrate.
	let baseConfig: RustAppConfig | null = null;
	let saveTimer: ReturnType<typeof setTimeout> | null = null;

	/** Build a full RustAppConfig from current store state + base config fields */
	function buildConfig(): RustAppConfig {
		return {
			...(baseConfig ?? ({} as RustAppConfig)),
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
			tab_ordering_mode: state.tabOrderingMode,
			tab_cycling_all_types: state.tabCyclingAllTypes,
			auto_show_pr_popover: state.autoShowPrPopover,
			prevent_sleep_when_busy: state.preventSleepWhenBusy,
			auto_update_enabled: state.autoUpdateEnabled,
			auto_update_plugins_enabled: state.autoUpdatePluginsEnabled,
			language: state.language,
			update_channel: state.updateChannel,
			disabled_agents: [...state.disabledAgents],
			intent_tab_title: state.intentTabTitle,
			suggest_followups: state.suggestFollowups,
			copy_on_select: state.copyOnSelect,
			show_last_prompt: state.showLastPrompt,
			bell_style: state.bellStyle,
			global_hotkey: state.globalHotkey,
			issue_filter: state.issueFilter,
			pr_hide_drafts: state.prHideDrafts,
			pr_hide_conflicting: state.prHideConflicting,
			pr_hide_ci_failing: state.prHideCiFailing,
			experimental_features_enabled: state.experimentalFeaturesEnabled,
			ai_chat_enabled: state.aiChatEnabled,
			ai_triage_enabled: state.aiTriageEnabled,
			ai_watchers_enabled: state.aiWatchersEnabled,
			scrollback_reflow: state.scrollbackReflow,
			cursor_style: state.cursorStyle,
			terminal_renderer: state.terminalRenderer,
			show_block_timestamps: state.showBlockTimestamps,
			show_scrollbar_marks: state.showScrollbarMarks,
			block_folding_enabled: state.blockFoldingEnabled,
			index_strategy: state.indexStrategy,
			standby_timeout_minutes: state.standbyTimeoutMinutes,
			custom_launchers: [...state.customLaunchers],
			services: baseConfig?.services ?? { auth: { session_token_duration_secs: 86400 } },
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
					} catch {
						/* ignore migration failure */
					}
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
				const tom = config.tab_ordering_mode;
				setState("tabOrderingMode", tom === "terminals-first" || tom === "free" ? tom : "grouped-by-type");
				setState("tabCyclingAllTypes", config.tab_cycling_all_types ?? false);
				setState("autoShowPrPopover", config.auto_show_pr_popover ?? true);
				setState("preventSleepWhenBusy", config.prevent_sleep_when_busy ?? false);
				setState("autoUpdateEnabled", config.auto_update_enabled ?? true);
				setState("autoUpdatePluginsEnabled", config.auto_update_plugins_enabled ?? true);
				setState("language", config.language || "en");
				setLocale(config.language || "en");
				const channel = config.update_channel;
				setState("updateChannel", channel === "nightly" ? channel : "stable");
				setState("disabledAgents", config.disabled_agents ?? []);
				setState("intentTabTitle", config.intent_tab_title ?? true);
				setState("copyOnSelect", config.copy_on_select ?? true);
				setState("showLastPrompt", config.show_last_prompt ?? false);
				setState("bellStyle", (config.bell_style || "visual") as SettingsStoreState["bellStyle"]);
				setState("suggestFollowups", config.suggest_followups ?? true);
				setState("globalHotkey", config.global_hotkey ?? null);
				setState("issueFilter", validateIssueFilter(config.issue_filter || null));
				setState("prHideDrafts", config.pr_hide_drafts ?? false);
				setState("prHideConflicting", config.pr_hide_conflicting ?? false);
				setState("prHideCiFailing", config.pr_hide_ci_failing ?? false);
				setState("experimentalFeaturesEnabled", config.experimental_features_enabled ?? false);
				setState("aiChatEnabled", config.ai_chat_enabled ?? false);
				setState("aiTriageEnabled", config.ai_triage_enabled ?? false);
				setState("aiWatchersEnabled", config.ai_watchers_enabled ?? false);
				setState("scrollbackReflow", config.scrollback_reflow ?? false);
				const cs = config.cursor_style;
				setState("cursorStyle", cs === "block" || cs === "underline" ? cs : "bar");
				setState("terminalRenderer", validateTerminalRenderer(config.terminal_renderer || null));
				setState("showBlockTimestamps", config.show_block_timestamps ?? true);
				setState("showScrollbarMarks", config.show_scrollbar_marks ?? true);
				setState("blockFoldingEnabled", config.block_folding_enabled ?? true);
				setState(
					"indexStrategy",
					(config.index_strategy as SettingsStoreState["indexStrategy"]) ?? "active_and_switch",
				);
				setState("standbyTimeoutMinutes", config.standby_timeout_minutes ?? 5);
				setState("customLaunchers", config.custom_launchers ?? []);
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

		setTabOrderingMode(mode: TabOrderingMode): void {
			setState("tabOrderingMode", mode);
			save();
		},

		setTabCyclingAllTypes(enabled: boolean): void {
			setState("tabCyclingAllTypes", enabled);
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

		setStandbyTimeoutMinutes(minutes: number): void {
			setState("standbyTimeoutMinutes", Math.max(0, Math.min(60, minutes)));
			save();
		},

		setIndexStrategy(strategy: SettingsStoreState["indexStrategy"]): void {
			setState("indexStrategy", strategy);
			save();
		},

		/** Set auto-update-enabled preference */
		setAutoUpdateEnabled(enabled: boolean): void {
			setState("autoUpdateEnabled", enabled);
			save();
		},

		/** Set auto-update-plugins-enabled preference */
		setAutoUpdatePluginsEnabled(enabled: boolean): void {
			setState("autoUpdatePluginsEnabled", enabled);
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
			setState(
				"disabledAgents",
				isDisabled ? state.disabledAgents.filter((a) => a !== agentType) : [...state.disabledAgents, agentType],
			);
			save();
		},

		/** Check if an agent type is enabled */
		isAgentEnabled(agentType: string): boolean {
			return !state.disabledAgents.includes(agentType);
		},

		/** Replace the full list of custom launchers (add/edit/remove all go through here) */
		setCustomLaunchers(launchers: CustomLauncher[]): void {
			setState("customLaunchers", launchers);
			save();
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

		setShowLastPrompt(enabled: boolean): void {
			setState("showLastPrompt", enabled);
			save();
		},

		/** Set issue filter mode */
		setIssueFilter(filter: IssueFilterMode): void {
			setState("issueFilter", filter);
			save();
		},

		setPrHideDrafts(v: boolean): void {
			setState("prHideDrafts", v);
			save();
		},

		setPrHideConflicting(v: boolean): void {
			setState("prHideConflicting", v);
			save();
		},

		setPrHideCiFailing(v: boolean): void {
			setState("prHideCiFailing", v);
			save();
		},

		/** Set terminal bell style */
		setBellStyle(style: SettingsStoreState["bellStyle"]): void {
			setState("bellStyle", style);
			save();
		},

		setExperimentalFeaturesEnabled(enabled: boolean): void {
			setState("experimentalFeaturesEnabled", enabled);
			save();
		},

		setAiChatEnabled(enabled: boolean): void {
			setState("aiChatEnabled", enabled);
			save();
		},

		setAiTriageEnabled(enabled: boolean): void {
			setState("aiTriageEnabled", enabled);
			save();
		},

		setAiWatchersEnabled(enabled: boolean): void {
			setState("aiWatchersEnabled", enabled);
			save();
		},

		setScrollbackReflow(enabled: boolean): void {
			setState("scrollbackReflow", enabled);
			save();
		},

		setCursorStyle(style: SettingsStoreState["cursorStyle"]): void {
			setState("cursorStyle", style);
			save();
		},

		setTerminalRenderer(renderer: TerminalRenderer): void {
			setState("terminalRenderer", renderer);
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

		isAiChatEnabled(): boolean {
			return state.experimentalFeaturesEnabled && state.aiChatEnabled;
		},

		isAiTriageEnabled(): boolean {
			return state.experimentalFeaturesEnabled && state.aiTriageEnabled;
		},

		isAiWatchersEnabled(): boolean {
			return state.experimentalFeaturesEnabled && state.aiWatchersEnabled;
		},
	};

	return {
		state,
		...actions,
		_testCancelPendingSave(): void {
			if (saveTimer) {
				clearTimeout(saveTimer);
				saveTimer = null;
			}
		},
	};
}

export const settingsStore = createSettingsStore();

// Debug registry — expose app settings for MCP introspection
import { registerDebugSnapshot } from "./debugRegistry";

registerDebugSnapshot("settings", () => {
	const s = settingsStore.state;
	return {
		ide: s.ide,
		font: s.font,
		fontWeight: s.fontWeight,
		defaultFontSize: s.defaultFontSize,
		shell: s.shell,
		theme: s.theme,
		language: s.language,
		splitTabMode: s.splitTabMode,
		tabOrderingMode: s.tabOrderingMode,
		tabCyclingAllTypes: s.tabCyclingAllTypes,
		bellStyle: s.bellStyle,
		updateChannel: s.updateChannel,
		intentTabTitle: s.intentTabTitle,
		suggestFollowups: s.suggestFollowups,
		copyOnSelect: s.copyOnSelect,
		autoUpdateEnabled: s.autoUpdateEnabled,
		preventSleepWhenBusy: s.preventSleepWhenBusy,
		issueFilter: s.issueFilter,
		terminalRenderer: s.terminalRenderer,
	};
});
