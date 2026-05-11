import { invoke, listen } from "./invoke";
import { appLogger } from "./stores/appLogger";
import { FONT_FAMILIES, type FontType, settingsStore } from "./stores/settings";

/** Terminal color theme (mirrors xterm.js ITheme keys used by CanvasTerminal) */
export interface TerminalTheme {
	background?: string;
	foreground?: string;
	cursor?: string;
	cursorAccent?: string;
	selectionBackground?: string;
	selectionForeground?: string;
	selectionInactiveBackground?: string;
	black?: string;
	red?: string;
	green?: string;
	yellow?: string;
	blue?: string;
	magenta?: string;
	cyan?: string;
	white?: string;
	brightBlack?: string;
	brightRed?: string;
	brightGreen?: string;
	brightYellow?: string;
	brightBlue?: string;
	brightMagenta?: string;
	brightCyan?: string;
	brightWhite?: string;
}

/** App-wide color scheme applied to the UI chrome (sidebar, tabs, toolbar, etc.) */
export interface IAppTheme {
	bgPrimary: string;
	bgSecondary: string;
	bgTertiary: string;
	bgHighlight: string;
	fgPrimary: string;
	fgSecondary: string;
	fgMuted: string;
	accent: string;
	accentHover: string;
	border: string;
	success: string;
	warning: string;
	error: string;
	textOnAccent: string;
	textOnError: string;
	textOnSuccess: string;
}

// ---------------------------------------------------------------------------
// Rust ThemeEntry shape (snake_case from serde)
// ---------------------------------------------------------------------------

interface RustTerminalColors {
	background: string;
	foreground: string;
	cursor: string;
	cursor_accent: string | null;
	selection_background: string | null;
	ansi: string[];
}

interface RustAppChromeColors {
	bg_primary: string;
	bg_secondary: string;
	bg_tertiary: string;
	bg_highlight: string;
	fg_primary: string;
	fg_secondary: string;
	fg_muted: string;
	accent: string;
	accent_hover: string;
	border: string;
	success: string;
	warning: string;
	error: string;
	text_on_accent: string;
	text_on_error: string;
	text_on_success: string;
}

interface RustThemeEntry {
	key: string;
	name: string;
	terminal: RustTerminalColors;
	app_chrome: RustAppChromeColors;
}

// ---------------------------------------------------------------------------
// Theme store (loaded from Rust at startup, updated on hot-reload)
// ---------------------------------------------------------------------------

interface LoadedTheme {
	key: string;
	name: string;
	terminal: TerminalTheme;
	appChrome: IAppTheme;
}

const ANSI_NAMES: readonly (keyof TerminalTheme)[] = [
	"black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
	"brightBlack", "brightRed", "brightGreen", "brightYellow",
	"brightBlue", "brightMagenta", "brightCyan", "brightWhite",
];

function mapRustEntry(entry: RustThemeEntry): LoadedTheme {
	const t = entry.terminal;
	const terminal: TerminalTheme = {
		background: t.background,
		foreground: t.foreground,
		cursor: t.cursor,
		cursorAccent: t.cursor_accent ?? undefined,
		selectionBackground: t.selection_background ?? undefined,
	};
	for (let i = 0; i < ANSI_NAMES.length && i < t.ansi.length; i++) {
		terminal[ANSI_NAMES[i]!] = t.ansi[i];
	}

	const ac = entry.app_chrome;
	const appChrome: IAppTheme = {
		bgPrimary: ac.bg_primary,
		bgSecondary: ac.bg_secondary,
		bgTertiary: ac.bg_tertiary,
		bgHighlight: ac.bg_highlight,
		fgPrimary: ac.fg_primary,
		fgSecondary: ac.fg_secondary,
		fgMuted: ac.fg_muted,
		accent: ac.accent,
		accentHover: ac.accent_hover,
		border: ac.border,
		success: ac.success,
		warning: ac.warning,
		error: ac.error,
		textOnAccent: ac.text_on_accent,
		textOnError: ac.text_on_error,
		textOnSuccess: ac.text_on_success,
	};

	return { key: entry.key, name: entry.name, terminal, appChrome };
}

let themes: Map<string, LoadedTheme> = new Map();
let loaded = false;

const FALLBACK_TERMINAL: TerminalTheme = {
	background: "#1e1e1e",
	foreground: "#cccccc",
	cursor: "#cccccc",
};

const FALLBACK_APP: IAppTheme = {
	bgPrimary: "#1e1e1e",
	bgSecondary: "#252526",
	bgTertiary: "#2d2d30",
	bgHighlight: "#37373d",
	fgPrimary: "#cccccc",
	fgSecondary: "#a0a0a0",
	fgMuted: "#9aa1a9",
	accent: "#59a8dd",
	accentHover: "#7abde5",
	border: "#3e3e42",
	success: "#4ade80",
	warning: "#dcdcaa",
	error: "#ef4444",
	textOnAccent: "#000000",
	textOnError: "#000000",
	textOnSuccess: "#000000",
};

/** Load themes from Rust backend. Must be called before applyAppTheme(). */
export async function loadThemes(): Promise<void> {
	try {
		const entries = await invoke<RustThemeEntry[]>("list_themes");
		themes = new Map(entries.map((e) => [e.key, mapRustEntry(e)]));
		loaded = true;
	} catch (e) {
		appLogger.warn("app", "Failed to load themes from backend", { error: e });
	}
}

/** Whether themes have been loaded from the backend. */
export function themesLoaded(): boolean {
	return loaded;
}

let unlistenThemeChanges: (() => void) | undefined;

/** Listen for hot-reload events from the themes/ directory watcher. Idempotent — safe to call multiple times. */
export async function listenForThemeChanges(): Promise<void> {
	unlistenThemeChanges?.();
	unlistenThemeChanges = await listen("themes-changed", () => {
		loadThemes()
			.then(() => applyAppTheme(settingsStore.state.theme))
			.catch((e) => appLogger.warn("app", "Theme hot-reload failed", { error: String(e) }));
	});
}

/** Get a terminal theme by key, falling back to vscode-dark */
export function getTerminalTheme(key: string): TerminalTheme {
	return themes.get(key)?.terminal ?? themes.get("vscode-dark")?.terminal ?? FALLBACK_TERMINAL;
}

/** Get an app theme by key, falling back to vscode-dark */
export function getAppTheme(key: string): IAppTheme {
	return themes.get(key)?.appChrome ?? themes.get("vscode-dark")?.appChrome ?? FALLBACK_APP;
}

/** Get display names for all loaded themes. */
export function getThemeNames(): Record<string, string> {
	const result: Record<string, string> = {};
	for (const [key, t] of themes) {
		result[key] = t.name;
	}
	return result;
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/** Parse a hex color (#rrggbb) to [r, g, b] in 0–255 */
function hexToRgb(hex: string): [number, number, number] {
	const h = hex.replace("#", "");
	return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
}

/** WCAG 2.x relative luminance (0 = black, 1 = white) */
function relativeLuminance(hex: string): number {
	const [r, g, b] = hexToRgb(hex).map((c) => {
		const s = c / 255;
		return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio between two hex colors (range 1–21) */
export function contrastRatio(hex1: string, hex2: string): number {
	const l1 = relativeLuminance(hex1);
	const l2 = relativeLuminance(hex2);
	const lighter = Math.max(l1, l2);
	const darker = Math.min(l1, l2);
	return (lighter + 0.05) / (darker + 0.05);
}

/** Convert camelCase property name to a CSS custom property (e.g. bgPrimary -> --bg-primary) */
function camelToKebab(str: string): string {
	return `--${str.replace(/[A-Z]/g, (m) => `-${m.toLowerCase()}`)}`;
}

/** Apply the selected monospace font to the entire UI via --font-mono CSS variable. */
export function applyFontFamily(font: FontType): void {
	const family = FONT_FAMILIES[font] || FONT_FAMILIES["JetBrains Mono"];
	const id = "tuic-font-override";
	let tag = document.getElementById(id) as HTMLStyleElement | null;
	if (!tag) {
		tag = document.createElement("style");
		tag.id = id;
		document.head.appendChild(tag);
	}
	tag.textContent = `* { --font-mono: ${family} !important; }`;
}

const ANSI_KEYS: readonly (keyof TerminalTheme)[] = [
	"black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
	"brightBlack", "brightRed", "brightGreen", "brightYellow",
	"brightBlue", "brightMagenta", "brightCyan", "brightWhite",
];

/** Apply an app theme by setting CSS custom properties on the document root */
export function applyAppTheme(key: string): void {
	const appTheme = themes.get(key);
	if (!appTheme) {
		appLogger.warn("app", `Unknown theme "${key}", falling back to vscode-dark`);
	}
	const theme = getAppTheme(key);
	const root = document.documentElement.style;
	for (const [prop, value] of Object.entries(theme)) {
		root.setProperty(camelToKebab(prop), value);
	}
	const termTheme = getTerminalTheme(key);
	const ansiRgb: [number, number, number][] = [];
	for (const k of ANSI_KEYS) {
		const val = termTheme[k];
		if (typeof val === "string") {
			root.setProperty(`--ansi-${camelToKebab(k).slice(2)}`, val);
			ansiRgb.push(hexToRgb(val));
		} else {
			ansiRgb.push([0, 0, 0]);
		}
	}
	if (ansiRgb.length === 16) {
		invoke("set_ansi_colors", { colors: ansiRgb }).catch((e: unknown) => {
			appLogger.warn("app", "Failed to sync ANSI colors to backend", { error: e });
		});
	}
}
