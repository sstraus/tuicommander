import { type Accessor, createContext, useContext } from "solid-js";

/** `compact` is the classic one-line rows; `rich` spends spare room on detail lines. */
export type SidebarDensity = "compact" | "rich";

/** H uses rich rows and filters branches to live harness sessions. */
export type SidebarDensityMode = SidebarDensity | "harness";

const MODE_CYCLE: readonly SidebarDensityMode[] = ["rich", "compact", "harness"];

export const isSidebarDensityMode = (v: unknown): v is SidebarDensityMode =>
	MODE_CYCLE.includes(v as SidebarDensityMode);

/** Next mode of the toolbar toggle: rich, compact, harness, then rich again. */
export const nextSidebarDensityMode = (m: SidebarDensityMode): SidebarDensityMode =>
	MODE_CYCLE[(MODE_CYCLE.indexOf(m) + 1) % MODE_CYCLE.length];

export const sidebarDensity = (mode: SidebarDensityMode = "rich"): SidebarDensity =>
	mode === "compact" ? "compact" : "rich";

/** The density the enclosing sidebar resolved; rows read it instead of taking a prop per level. */
const COMPACT: Accessor<SidebarDensity> = () => "compact";
export const SidebarDensityContext = createContext<Accessor<SidebarDensity>>(COMPACT);
export const useSidebarDensity = (): Accessor<SidebarDensity> => useContext(SidebarDensityContext) ?? COMPACT;
