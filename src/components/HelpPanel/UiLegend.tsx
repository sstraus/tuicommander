import { type Component, For } from "solid-js";
import { PrStateBadge } from "../Sidebar/PrStateBadge";
import { BranchIcon, type BranchIconProps, UnmergedMarker } from "../Sidebar/RepoSection";
import s from "./UiLegend.module.css";

// ---------------------------------------------------------------------------
// Legend data
// ---------------------------------------------------------------------------

interface LegendEntry {
	color: string;
	label: string;
	description: string;
	pulsing?: boolean;
}

const TERMINAL_DOT_LEGEND: LegendEntry[] = [
	{ color: "var(--fg-muted)", label: "No session", description: "Terminal never ran or was reset" },
	{ color: "var(--accent)", label: "Busy", description: "Producing output", pulsing: true },
	{ color: "var(--success)", label: "Idle", description: "Agent waiting, no recent output" },
	{ color: "var(--unseen)", label: "Unseen", description: "Went idle while not viewed" },
	{ color: "var(--attention)", label: "Question", description: "Agent needs input", pulsing: true },
	{ color: "var(--error)", label: "Error", description: "API error or agent stuck", pulsing: true },
];

interface TabTypeEntry {
	color: string;
	label: string;
	description: string;
}

const TAB_TYPE_LEGEND: TabTypeEntry[] = [
	{ color: "rgb(var(--tab-diff-rgb))", label: "Diff", description: "Git diff viewer" },
	{ color: "rgb(var(--tab-edit-rgb))", label: "Editor", description: "Code editor" },
	{ color: "rgb(var(--tab-md-rgb))", label: "Markdown", description: "Markdown viewer" },
	{ color: "rgb(var(--tab-panel-rgb))", label: "Panel", description: "Dashboard / plugin panel" },
	{ color: "rgb(var(--tab-remote-rgb))", label: "PTY", description: "Remote session (HTTP/MCP)" },
];

const PANEL_COLOR_LEGEND: TabTypeEntry[] = [
	{ color: "rgb(var(--tab-diff-rgb))", label: "Diff Panel", description: "Git diff summary" },
	{ color: "rgb(var(--tab-md-rgb))", label: "Markdown Panel", description: "Markdown browser" },
	{ color: "rgb(var(--tab-edit-rgb))", label: "File Browser", description: "File explorer" },
	{ color: "rgb(var(--tab-panel-rgb))", label: "Panel", description: "Dashboard / plugin panel" },
];

interface SymbolEntry {
	symbol: string;
	label: string;
	description: string;
	color?: string;
}

/** Sidebar row icons. Each entry renders the real `BranchIcon` with the props
 *  that select its shape, so the legend can never drift from the sidebar. */
export interface BranchIconEntry {
	icon: BranchIconProps;
	label: string;
	description: string;
}

export const SIDEBAR_SYMBOL_LEGEND: BranchIconEntry[] = [
	{
		icon: { isMainBranch: true, isMainWorktree: true, branchHasTerminals: true },
		label: "Main branch",
		description: "Primary branch (main/master)",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: true, branchHasTerminals: true },
		label: "Feature branch",
		description: "Main worktree switched to another branch",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, branchHasTerminals: true },
		label: "Worktree",
		description: "Linked git worktree",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, isMerged: true, branchHasTerminals: true },
		label: "Merged",
		description: "Branch merged into the default branch",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, isShell: true, branchHasTerminals: true },
		label: "Shell",
		description: "Folder without a git repository",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, hasQuestion: true, branchHasTerminals: true },
		label: "Awaiting input",
		description: "A terminal needs input",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, hasError: true, branchHasTerminals: true },
		label: "Error",
		description: "API error or agent stuck",
	},
	{
		icon: { isMainBranch: false, isMainWorktree: false, branchHasTerminals: false },
		label: "Idle",
		description: "No open terminal on this row",
	},
];

/** Rendered with the real sidebar component, so the legend cannot drift from
 *  what a branch row shows — color, shape and pulse included. */
export interface PrLegendEntry {
	label: string;
	description: string;
	badge: Partial<Parameters<typeof PrStateBadge>[0]>;
}

export const PR_BADGE_LEGEND: PrLegendEntry[] = [
	{ label: "Open", description: "Open PR", badge: { state: "open" } },
	{
		label: "Ready",
		description: "Approved and mergeable",
		badge: { state: "open", mergeable: "MERGEABLE", reviewDecision: "APPROVED" },
	},
	{ label: "Draft", description: "PR is a draft", badge: { state: "open", isDraft: true } },
	{
		label: "Conflicts",
		description: "Merge conflicts (pulsing diamond)",
		badge: { state: "open", conflictState: "conflicting" },
	},
	{
		label: "Checking",
		description: "GitHub is recomputing mergeability (pulsing)",
		badge: { state: "open", conflictState: "checking" },
	},
	{ label: "CI Failed", description: "CI checks failed", badge: { state: "open", ciFailed: 1 } },
	{
		label: "Changes Req.",
		description: "Changes requested",
		badge: { state: "open", reviewDecision: "CHANGES_REQUESTED" },
	},
	{
		label: "Comments",
		description: "Unresolved review threads (bot and human)",
		badge: { state: "open", unresolvedThreads: 1 },
	},
	{ label: "Review", description: "Awaiting review", badge: { state: "open", reviewDecision: "REVIEW_REQUIRED" } },
	{ label: "CI Running", description: "CI in progress (pulsing)", badge: { state: "open", ciPending: 1 } },
	{ label: "Merged", description: "PR merged", badge: { state: "merged" } },
	{ label: "Closed", description: "PR closed without merging", badge: { state: "closed" } },
];

const TOOLBAR_COUNT_LEGEND: SymbolEntry[] = [
	{
		symbol: "↑N",
		label: "Ahead",
		description: "Selected branch has N commits not pushed to its upstream. Absent without an upstream",
	},
	{ symbol: "↓N", label: "Behind", description: "Selected branch is N commits behind its upstream" },
];

const STATS_LEGEND: SymbolEntry[] = [
	{ symbol: "+N", label: "Additions", description: "Lines added vs main", color: "var(--success)" },
	{ symbol: "-N", label: "Deletions", description: "Lines removed vs main", color: "var(--diff-del)" },
];

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const UiLegend: Component = () => {
	return (
		<div class={s.legend}>
			{/* Terminal dot states */}
			<div class={s.group}>
				<label class={s.groupLabel}>Terminal Status Dots</label>
				<div class={s.grid}>
					<For each={TERMINAL_DOT_LEGEND}>
						{(entry) => (
							<div class={s.row}>
								<span class={entry.pulsing ? s.dotPulsing : s.dot} style={{ background: entry.color }} />
								<span class={s.label}>{entry.label}</span>
								<span class={s.desc}>{entry.description}</span>
							</div>
						)}
					</For>
				</div>
			</div>

			{/* Tab type colors */}
			<div class={s.group}>
				<label class={s.groupLabel}>Tab Types</label>
				<div class={s.grid}>
					<For each={TAB_TYPE_LEGEND}>
						{(entry) => (
							<div class={s.row}>
								<span class={s.colorBar} style={{ background: entry.color }} />
								<span class={s.label}>{entry.label}</span>
								<span class={s.desc}>{entry.description}</span>
							</div>
						)}
					</For>
				</div>
			</div>

			{/* Panel colors */}
			<div class={s.group}>
				<label class={s.groupLabel}>Panels</label>
				<div class={s.grid}>
					<For each={PANEL_COLOR_LEGEND}>
						{(entry) => (
							<div class={s.row}>
								<span class={s.colorBar} style={{ background: entry.color }} />
								<span class={s.label}>{entry.label}</span>
								<span class={s.desc}>{entry.description}</span>
							</div>
						)}
					</For>
				</div>
			</div>

			{/* Sidebar branch icons */}
			<div class={s.group}>
				<label class={s.groupLabel}>Sidebar Symbols</label>
				<div class={s.grid}>
					<For each={SIDEBAR_SYMBOL_LEGEND}>
						{(entry) => (
							<div class={s.row}>
								<span class={s.symbol}>
									<BranchIcon {...entry.icon} />
								</span>
								<span class={s.label}>{entry.label}</span>
								<span class={s.desc}>{entry.description}</span>
							</div>
						)}
					</For>
				</div>
			</div>

			{/* Unmerged marker */}
			<div class={s.group}>
				<label class={s.groupLabel}>Branch Markers</label>
				<div class={s.grid}>
					<div class={s.row}>
						<span class={s.symbol}>
							<UnmergedMarker />
						</span>
						<span class={s.label}>Unmerged</span>
						<span class={s.desc}>Commits not merged into the default branch. Not a dirty worktree</span>
					</div>
				</div>
			</div>

			{/* PR badges */}
			<div class={s.group}>
				<label class={s.groupLabel}>PR Status Badges</label>
				<div class={s.grid}>
					<For each={PR_BADGE_LEGEND}>
						{(entry) => (
							<div class={s.row}>
								<PrStateBadge compact prNumber={42} {...entry.badge} />
								<span class={s.label}>{entry.label}</span>
								<span class={s.desc}>{entry.description}</span>
							</div>
						)}
					</For>
				</div>
			</div>

			{/* Toolbar ahead/behind */}
			<div class={s.group}>
				<label class={s.groupLabel}>Toolbar Branch Counts</label>
				<div class={s.grid}>
					<For each={TOOLBAR_COUNT_LEGEND}>
						{(entry) => (
							<div class={s.row}>
								<span class={s.symbol}>{entry.symbol}</span>
								<span class={s.label}>{entry.label}</span>
								<span class={s.desc}>{entry.description}</span>
							</div>
						)}
					</For>
				</div>
			</div>

			{/* Stats */}
			<div class={s.group}>
				<label class={s.groupLabel}>Diff Stats</label>
				<div class={s.grid}>
					<For each={STATS_LEGEND}>
						{(entry) => (
							<div class={s.row}>
								<span class={s.symbol} style={{ color: entry.color }}>
									{entry.symbol}
								</span>
								<span class={s.label}>{entry.label}</span>
								<span class={s.desc}>{entry.description}</span>
							</div>
						)}
					</For>
				</div>
			</div>
		</div>
	);
};
