import { type Accessor, createContext, createSignal, onCleanup, useContext } from "solid-js";

/** `compact` is the classic one-line rows; `rich` spends spare room on detail lines. */
export type SidebarDensity = "compact" | "rich";

/** The user's choice: `auto` derives the density, the others force it. */
export type SidebarDensityMode = "auto" | SidebarDensity;

const MODE_CYCLE: readonly SidebarDensityMode[] = ["auto", "compact", "rich"];

export const isSidebarDensityMode = (v: unknown): v is SidebarDensityMode =>
	MODE_CYCLE.includes(v as SidebarDensityMode);

/** Next mode of the toolbar toggle: auto, compact, rich, then auto again. */
export const nextSidebarDensityMode = (m: SidebarDensityMode): SidebarDensityMode =>
	MODE_CYCLE[(MODE_CYCLE.indexOf(m) + 1) % MODE_CYCLE.length];

/**
 * Measured on the desktop build in rich mode (window 900px high): a branch row
 * with its detail line is 50-52px (67px when it carries a PR title), an agent
 * row 45px, a repo header with its meta line 45px. 52px is the budget per row.
 */
export const RICH_ROW_PX = 52;

/**
 * Viewport height the sidebar list does not get: toolbar, git quick actions and
 * footer. Measured: a 900px window leaves a 804px list.
 */
export const SIDEBAR_CHROME_PX = 96;

/** Rich rows that fit a viewport of this height without scrolling. */
export const roomyMaxRows = (viewportPx: number): number =>
	Math.max(0, Math.floor((viewportPx - SIDEBAR_CHROME_PX) / RICH_ROW_PX));

/** The viewport a 768px tablet gives: (768 - 96) / 52 rich rows. */
export const ROOMY_MAX_ROWS = roomyMaxRows(768);

export interface DensityRepoShape {
	collapsed: boolean;
	expanded: boolean;
	detachedWorktrees?: readonly unknown[];
	workspaces: Record<string, { terminals: readonly unknown[]; tabsCollapsed?: boolean }>;
}

/**
 * Rows the sidebar would render: one header per repo, one row per branch when the
 * repo is open, and one row per terminal tab under a branch whose tab list is shown
 * (`tabTreeEnabled` on and the branch not collapsed). Every open repo also renders
 * the sidebar plugin panels, `pluginRowsPerRepo` rows in all.
 */
export function countSidebarRows(
	repos: readonly DensityRepoShape[],
	tabTreeEnabled: boolean,
	pluginRowsPerRepo = 0,
): number {
	return repos.reduce((n, r) => {
		if (!r.expanded || r.collapsed) return n + 1;
		const branches = Object.values(r.workspaces);
		const tabs = tabTreeEnabled ? branches.reduce((t, w) => t + (w.tabsCollapsed ? 0 : w.terminals.length), 0) : 0;
		return n + 1 + branches.length + (r.detachedWorktrees?.length ?? 0) + tabs + pluginRowsPerRepo;
	}, 0);
}

/**
 * A finger always gets the rich layout, however many repos there are.
 * Rich navigation uses compact-sized rows; working agents retain intent details.
 * Otherwise a short list is shown rich and a long one compact. A forced mode wins.
 */
export function sidebarDensity(
	rowCount: number,
	coarsePointer: boolean,
	mode: SidebarDensityMode = "auto",
	maxRows: number = ROOMY_MAX_ROWS,
): SidebarDensity {
	if (mode !== "auto") return mode;
	if (coarsePointer) return "rich";
	return rowCount <= maxRows ? "rich" : "compact";
}

const COARSE_QUERY = "(pointer: coarse)";

/** Live `(pointer: coarse)` — true when the primary input is a finger. */
export function createCoarsePointer(): Accessor<boolean> {
	const mql = typeof window !== "undefined" ? window.matchMedia?.(COARSE_QUERY) : undefined;
	const [coarse, setCoarse] = createSignal(mql?.matches ?? false);
	if (mql) {
		const onChange = (e: MediaQueryListEvent) => setCoarse(e.matches);
		mql.addEventListener?.("change", onChange);
		onCleanup(() => mql.removeEventListener?.("change", onChange));
	}
	return coarse;
}

/** Live `window.innerHeight`, so the auto budget follows a resized window. */
export function createViewportHeight(): Accessor<number> {
	if (typeof window === "undefined") return () => 768;
	const [height, setHeight] = createSignal(window.innerHeight);
	const onResize = () => setHeight(window.innerHeight);
	window.addEventListener("resize", onResize);
	onCleanup(() => window.removeEventListener("resize", onResize));
	return height;
}

/** The density the enclosing sidebar resolved; rows read it instead of taking a prop per level. */
const COMPACT: Accessor<SidebarDensity> = () => "compact";
export const SidebarDensityContext = createContext<Accessor<SidebarDensity>>(COMPACT);
export const useSidebarDensity = (): Accessor<SidebarDensity> => useContext(SidebarDensityContext) ?? COMPACT;
