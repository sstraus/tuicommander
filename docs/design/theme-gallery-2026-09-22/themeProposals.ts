/**
 * Proposed theme edits, shown by the theme gallery next to the shipped theme.
 *
 * Each entry is a partial override merged over the shipped JSON: `appChrome`
 * keys over `appChrome`, every other key over the top-level ANSI/terminal
 * colors. The shipped files in src-tauri/src/themes/ stay untouched until a
 * proposal is accepted and copied into them. Vite HMR repaints the gallery
 * when this file changes.
 */
export interface ThemeProposal {
	/** Why the theme changes, shown above the proposed card. */
	note: string;
	/** Proposed for deletion from the bundled set. */
	remove?: boolean;
	appChrome?: Record<string, string>;
	colors?: Record<string, string>;
}

export const THEME_PROPOSALS: Record<string, ThemeProposal> = {
	clean: {
		note: "Visible borders: panel edges were lost against the surface.",
		appChrome: { border: "#2e2e2e" },
	},
	commander: {
		note: "Chrome matches the terminal background, clearer layers, a brighter accent so the repo name and links read on grey.",
		appChrome: {
			background: "#1e1e1e",
			surface: "#262626",
			surfaceElevated: "#2e2e2e",
			highlight: "#3a3a3a",
			border: "#383838",
			accent: "#3b8eea",
			accentHover: "#5aa0f0",
		},
	},
	darksun: {
		note: "More depth between main area, sidebar and selection.",
		appChrome: {
			surface: "#28272c",
			surfaceElevated: "#323136",
			highlight: "#413f46",
			border: "#3d3c43",
		},
	},
	"deep-black": {
		note: "Duplicate of Ink (neutral black). Keep only if a pure-black OLED theme is wanted.",
		remove: true,
	},
	"delicate-one": {
		note: "Fourth near-identical neutral grey, and the weakest one: dim accent, red and terminal colors.",
		remove: true,
	},
	"minimal-kiwi": {
		note: "Success no longer shares the accent green; clearer layers.",
		appChrome: {
			surface: "#1a1f17",
			surfaceElevated: "#232a1f",
			highlight: "#303828",
			border: "#2f3627",
			success: "#5cc98f",
		},
	},
	monokai: {
		note: "Darker sidebar (Monokai Pro style) to separate it from the editor.",
		appChrome: { surface: "#1e1f1c" },
	},
	"solarized-dark": {
		note: "Border no longer identical to the elevated surface. Low contrast is kept: it is the theme's identity.",
		appChrome: { border: "#125163" },
	},
	"tokyo-night": {
		note: "Tokyo Night's own sidebar and selection colors: selected and hovered rows were invisible.",
		appChrome: {
			surface: "#16161e",
			surfaceElevated: "#292e42",
			highlight: "#283457",
			border: "#2f334d",
		},
	},
	"vscode-dark": {
		note: "Merged into Commander, which now has the same greys with a brighter accent.",
		remove: true,
	},
	"vscode-light": {
		note: "Sidebar, hover and selection separate from the white editor; readable terminal yellow/cyan and success green.",
		appChrome: {
			surface: "#f3f3f3",
			surfaceElevated: "#e8e8e8",
			highlight: "#dcdcdc",
			border: "#d4d4d4",
			success: "#1a7f37",
		},
		colors: {
			yellow: "#7a6f00",
			brightYellow: "#7a6f00",
			cyan: "#0078a0",
			brightCyan: "#0078a0",
		},
	},
};
