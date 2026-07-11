/**
 * File type colors for file browser icons.
 *
 * Modern pastel palette (Tailwind 400-level hues) tuned for the app's dark
 * chrome — the same tones VSCode-style icon themes use. Icons themselves stay
 * monochrome `currentColor` SVGs; only the inherited CSS color changes.
 * Dotfiles and lockfiles use the theme's muted token so they stay dimmed
 * under every theme.
 */

/** Muted gray-blue folder tone, in the spirit of VSCode's folder icons. */
const FOLDER_COLOR = "#8f98a7";

const MUTED = "var(--fg-muted)";

const COLORS = {
	blue: "#60a5fa",
	yellow: "#facc15",
	amber: "#f59e0b",
	rose: "#fb7185",
	orange: "#fb923c",
	indigo: "#818cf8",
	violet: "#a78bfa",
	sky: "#7dd3fc",
	cyan: "#22d3ee",
	green: "#86efac",
	emerald: "#4ade80",
	red: "#f87171",
	clay: "#d97757",
	gold: "#fbbf24",
	docker: "#38bdf8",
} as const;

const EXTENSION_COLORS: Record<string, string> = {
	// Languages
	ts: COLORS.blue,
	tsx: COLORS.blue,
	mts: COLORS.blue,
	cts: COLORS.blue,
	js: COLORS.yellow,
	jsx: COLORS.yellow,
	mjs: COLORS.yellow,
	cjs: COLORS.yellow,
	rs: COLORS.rose,
	go: COLORS.cyan,
	py: COLORS.sky,
	sh: COLORS.green,
	bash: COLORS.green,
	zsh: COLORS.green,
	fish: COLORS.green,
	// Web
	css: COLORS.indigo,
	scss: COLORS.indigo,
	less: COLORS.indigo,
	html: COLORS.orange,
	htm: COLORS.orange,
	vue: COLORS.orange,
	svelte: COLORS.orange,
	// Data / config
	json: COLORS.amber,
	jsonc: COLORS.amber,
	json5: COLORS.amber,
	yaml: COLORS.red,
	yml: COLORS.red,
	toml: COLORS.clay,
	// Docs
	md: COLORS.violet,
	mdx: COLORS.violet,
	// Images
	svg: COLORS.gold,
	png: COLORS.emerald,
	jpg: COLORS.emerald,
	jpeg: COLORS.emerald,
	gif: COLORS.emerald,
	webp: COLORS.emerald,
	ico: COLORS.emerald,
};

/** Lockfiles without a `.lock` extension. */
const LOCK_FILES = new Set(["package-lock.json", "pnpm-lock.yaml", "bun.lockb"]);

/**
 * CSS color for a file browser icon, or undefined to inherit `currentColor`
 * (the monochrome behavior). Well-known filenames (Dockerfile, Makefile) are
 * matched first, then dotfiles and lockfiles muted, then the extension map,
 * all case-insensitively.
 */
export function getFileIconColor(name: string, isDir: boolean): string | undefined {
	if (isDir) return FOLDER_COLOR;
	const lower = name.toLowerCase();
	if (lower === "dockerfile" || lower.startsWith("dockerfile.")) return COLORS.docker;
	if (lower === "makefile" || lower === "gnumakefile" || lower === "justfile") return MUTED;
	if (lower.startsWith(".")) return MUTED;
	if (lower.endsWith(".lock") || LOCK_FILES.has(lower)) return MUTED;
	const dot = lower.lastIndexOf(".");
	if (dot <= 0) return undefined;
	return EXTENSION_COLORS[lower.slice(dot + 1)];
}
