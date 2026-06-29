import { type Component, createEffect, createSignal, Match, Show, Switch } from "solid-js";
import { type DiffStatus, diffTabsStore } from "../../stores/diffTabs";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import p from "../shared/panel.module.css";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { PanelWindowControls } from "../ui/PanelWindowControls";
import { BlameTab } from "./BlameTab";
import { BranchesTab } from "./BranchesTab";
import { ChangesTab } from "./ChangesTab";
import s from "./GitPanel.module.css";
import { HistoryTab } from "./HistoryTab";
import { LogTab } from "./LogTab";
import { StashesTab } from "./StashesTab";

export type OpenDiffFn = (
	repoPath: string,
	filePath: string,
	status: DiffStatus,
	scope?: string,
	untracked?: boolean,
) => void;

type GitTab = "changes" | "log" | "stashes" | "branches";

const TABS: { id: GitTab; label: string }[] = [
	{ id: "changes", label: "Changes" },
	{ id: "log", label: "Log" },
	{ id: "stashes", label: "Stashes" },
	{ id: "branches", label: "Branches" },
];

export interface GitPanelProps {
	visible: boolean;
	repoPath: string | null;
	/** Effective filesystem root (worktree path when on a linked worktree) */
	fsRoot?: string | null;
	onClose: () => void;
	/** When set, switches to the given tab (used by external shortcuts like toggle-branches-tab) */
	requestedTab?: GitTab | null;
	/** "inline" (default) = embedded in main window; "detached" = separate panel window */
	mode?: "inline" | "detached";
	/** Override for diff-tab opening. When omitted, calls diffTabsStore.add() directly. */
	onOpenDiff?: OpenDiffFn;
}

export const GitPanel: Component<GitPanelProps> = (props) => {
	const [activeTab, setActiveTab] = createSignal<GitTab>("changes");
	const [selectedFile, setSelectedFile] = createSignal<string | null>(null);
	const [historyExpanded, setHistoryExpanded] = createSignal(false);
	const [blameExpanded, setBlameExpanded] = createSignal(false);
	const gitPath = () => (props.fsRoot || props.repoPath) as string | null;
	const mode = () => props.mode ?? "inline";
	const openDiff: OpenDiffFn = (...args) => (props.onOpenDiff ?? diffTabsStore.add.bind(diffTabsStore))(...args);

	// Switch to the requested tab when an external action (e.g. keyboard shortcut) specifies one
	createEffect(() => {
		const tab = props.requestedTab;
		if (tab) setActiveTab(tab);
	});

	/** Split path into basename for compact display */
	function basename(path: string): string {
		const i = path.lastIndexOf("/");
		return i === -1 ? path : path.slice(i + 1);
	}

	return (
		<div
			id="git-panel"
			class={cx(s.panel, mode() === "detached" && s.detached, !props.visible && s.hidden)}
			tabIndex={-1}
		>
			<Show when={mode() === "inline"}>
				<PanelResizeHandle panelId="git-panel" />
			</Show>
			<div class={p.header}>
				<div
					class={s.tabs}
					// The strip hides its scrollbar (see GitPanel.module.css); map vertical
					// wheel/trackpad to horizontal scroll so the overflowing tabs stay reachable.
					onWheel={(e) => {
						const el = e.currentTarget;
						if (el.scrollWidth <= el.clientWidth) return;
						const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
						if (delta === 0) return;
						el.scrollLeft += delta;
						e.preventDefault();
					}}
				>
					{TABS.map((tab) => (
						<button class={cx(s.tab, activeTab() === tab.id && s.tabActive)} onClick={() => setActiveTab(tab.id)}>
							{tab.label}
						</button>
					))}
				</div>
				<PanelWindowControls panelId="git" mode={mode()} onInlineClose={props.onClose} />
			</div>
			{/* Main tab content — <Switch>/<Match> unmounts inactive tabs, preventing hidden
         tabs from reacting to revision bumps. Additionally, repoPath is null when the
         panel is hidden (props.visible=false), so even the active tab won't fetch. */}
			<div class={s.tabContent}>
				<Switch>
					<Match when={activeTab() === "changes"}>
						<ChangesTab
							repoPath={props.visible ? gitPath() : null}
							storeRepoPath={props.visible ? props.repoPath : null}
							onFileSelect={setSelectedFile}
							onOpenDiff={openDiff}
						/>
					</Match>
					<Match when={activeTab() === "log"}>
						<LogTab repoPath={props.visible ? gitPath() : null} onOpenDiff={openDiff} />
					</Match>
					<Match when={activeTab() === "stashes"}>
						<StashesTab repoPath={props.visible ? gitPath() : null} />
					</Match>
					<Match when={activeTab() === "branches"}>
						<BranchesTab repoPath={props.visible ? gitPath() : null} />
					</Match>
				</Switch>
			</div>
			{/* Sub-panels: History & Blame — only visible in Changes tab */}
			<Show when={activeTab() === "changes"}>
				<div class={s.subPanels}>
					<div
						class={s.subPanelHeader}
						role="button"
						tabIndex={0}
						onClick={() => setHistoryExpanded((v) => !v)}
						onKeyDown={onClickKeyDown(() => setHistoryExpanded((v) => !v))}
					>
						<span class={cx(s.subChevron, !historyExpanded() && s.subChevronCollapsed)}>&#x25BC;</span>
						<span class={s.subPanelLabel}>History</span>
						<Show when={selectedFile()}>
							<span class={s.subPanelFile}>{basename(selectedFile()!)}</span>
						</Show>
					</div>
					<Show when={historyExpanded()}>
						<div class={s.subPanelBody}>
							<HistoryTab repoPath={props.visible ? gitPath() : null} filePath={selectedFile()} onOpenDiff={openDiff} />
						</div>
					</Show>
					<div
						class={s.subPanelHeader}
						role="button"
						tabIndex={0}
						onClick={() => setBlameExpanded((v) => !v)}
						onKeyDown={onClickKeyDown(() => setBlameExpanded((v) => !v))}
					>
						<span class={cx(s.subChevron, !blameExpanded() && s.subChevronCollapsed)}>&#x25BC;</span>
						<span class={s.subPanelLabel}>Blame</span>
						<Show when={selectedFile()}>
							<span class={s.subPanelFile}>{basename(selectedFile()!)}</span>
						</Show>
					</div>
					<Show when={blameExpanded()}>
						<div class={s.subPanelBody}>
							<BlameTab repoPath={props.visible ? gitPath() : null} filePath={selectedFile()} />
						</div>
					</Show>
				</div>
			</Show>
		</div>
	);
};

export default GitPanel;
