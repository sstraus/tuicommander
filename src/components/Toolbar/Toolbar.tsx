import { type Component, createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import releaseNotes from "../../assets/release-notes.json";
import { useGitHub } from "../../hooks/useGitHub";
import { t } from "../../i18n";
import type { ActivityItem } from "../../plugins/types";
import { activityStore } from "../../stores/activityStore";
import { commandPaletteStore } from "../../stores/commandPalette";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { pluginStore } from "../../stores/pluginStore";
import type { PrNotification } from "../../stores/prNotifications";
import { type PrNotificationType, prNotificationsStore } from "../../stores/prNotifications";
import { progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { uiStore } from "../../stores/ui";
import { updaterStore } from "../../stores/updater";
import { isTauri } from "../../transport";
import { getRepoConnection } from "../../transportRuntime";
import { cx } from "../../utils";
import { keyFor } from "../../utils/hotkey";
import { navigateToTerminal } from "../../utils/navigateToTerminal";
import { getRepoTextColor } from "../../utils/repoColor";
import type { SidebarDensityMode } from "../../utils/sidebarDensity";
import { IdeLauncher } from "../IdeLauncher";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import { SmartPromptsDropdown } from "../SmartPromptsDropdown/SmartPromptsDropdown";
import { Button } from "../ui";
import { StoriesButton } from "./StoriesButton";
import s from "./Toolbar.module.css";

function relativeAge(timestamp: number): string {
	const seconds = Math.floor((Date.now() - timestamp) / 1000);
	if (seconds < 60) return "just now";
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) return `${minutes}m ago`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours}h ago`;
	return `${Math.floor(hours / 24)}d ago`;
}

const DENSITY_TITLES: Record<SidebarDensityMode, () => string> = {
	auto: () => t("toolbar.densityAuto", "Sidebar layout: auto (rich for a short list or a finger) — click for compact"),
	compact: () => t("toolbar.densityCompact", "Sidebar layout: compact — click for rich"),
	rich: () => t("toolbar.densityRich", "Sidebar layout: rich (details on every row) — click for auto"),
};

/** Back/forward arrows over the terminal activation history. */
const HISTORY_BUTTONS = [
	{
		direction: "back",
		action: "history-back",
		icon: "M10 3 5 8l5 5",
		go: (name: string) => t("toolbar.historyBack", "Back to {name}", { name }),
		none: () => t("toolbar.historyBackNone", "No earlier terminal"),
	},
	{
		direction: "forward",
		action: "history-forward",
		icon: "m6 3 5 5-5 5",
		go: (name: string) => t("toolbar.historyForward", "Forward to {name}", { name }),
		none: () => t("toolbar.historyForwardNone", "No later terminal"),
	},
] as const;

/** Row glyph: five tight lines for compact, a title with a detail line for rich. */
const DENSITY_LABELS: Record<SidebarDensityMode, () => string> = {
	auto: () => t("toolbar.densityAutoShort", "A"),
	compact: () => t("toolbar.densityCompactShort", "C"),
	rich: () => t("toolbar.densityRichShort", "R"),
};

const DENSITY_ICON_PATHS: Record<SidebarDensityMode, string> = {
	auto: "M2 3.5h12M2 8h12M2 12.5h12",
	compact: "M2 2.5h12M2 5.5h12M2 8.5h12M2 11.5h12M2 14.5h12",
	rich: "M2 3h12M2 5.5h7M2 10.5h12M2 13h7",
};

const NOTIFICATION_LABELS: Record<PrNotificationType, { label: string; icon: string; cls: string }> = {
	merged: { label: "Merged", icon: "\u2714", cls: s.notifMerged },
	closed: { label: "Closed", icon: "\u2716", cls: s.notifClosed },
	blocked: { label: "Conflicts", icon: "\u26A0", cls: s.notifBlocked },
	ci_failed: { label: "CI Failed", icon: "\u2716", cls: s.notifCiFailed },
	ci_recovered: { label: "CI Passed", icon: "\u2714", cls: s.notifReady },
	changes_requested: { label: "Changes Req.", icon: "\u270E", cls: s.notifChanges },
	ready: { label: "Ready", icon: "\u2713", cls: s.notifReady },
	review_started: { label: "Reviewing", icon: "▶", cls: s.notifReady },
};

// ---------------------------------------------------------------------------
// Last-item shortcut helpers
// ---------------------------------------------------------------------------

type LastItemSource =
	| { kind: "activity"; item: ActivityItem }
	| { kind: "pr"; notif: PrNotification }
	| { kind: "update"; version: string };

/** Find the most recently created item across all notification sources. */
function getLastItemAcrossStores(activeRepoPath: string | null): LastItemSource | null {
	const activityItem = activityStore.getLastItem(activeRepoPath ?? undefined);
	const prNotifs = prNotificationsStore.getActive();
	const prLast =
		prNotifs.length > 0 ? prNotifs.reduce((latest, n) => (n.createdAt >= latest.createdAt ? n : latest)) : null;

	// Update available is always "newest" — it's the most important notification
	const upd = updaterStore.state;
	if (upd.available && upd.version) {
		return { kind: "update", version: upd.version };
	}

	if (!activityItem && !prLast) return null;
	if (!prLast) return { kind: "activity", item: activityItem! };
	if (!activityItem) return { kind: "pr", notif: prLast };
	return activityItem.createdAt >= prLast.createdAt
		? { kind: "activity", item: activityItem }
		: { kind: "pr", notif: prLast };
}

export type LastItemSeverity = "error" | "success" | "warn" | "info";

/** Map a PR notification type to a pill severity (red/green/amber/neutral). */
export function prTypeSeverity(type: PrNotificationType): LastItemSeverity {
	switch (type) {
		case "ci_failed":
			return "error";
		case "blocked":
		case "changes_requested":
			return "warn";
		case "merged":
		case "ci_recovered":
		case "ready":
			return "success";
		default:
			return "info";
	}
}

/** Severity that colors the last-item pill, by source kind. */
export function lastItemSeverity(src: LastItemSource): LastItemSeverity {
	switch (src.kind) {
		case "update":
			return "info";
		case "pr":
			return prTypeSeverity(src.notif.type);
		default:
			return src.item.severity ?? "info";
	}
}

/** Stable identity for a last-item source, used to remember it was "seen" (so
 *  clicking the pill clears it without dismissing the underlying notification). */
export function lastItemKey(src: LastItemSource): string {
	switch (src.kind) {
		case "update":
			return `update:${src.version}`;
		case "pr":
			return `pr:${src.notif.id}`;
		default:
			return `activity:${src.item.id}`;
	}
}

export interface ToolbarProps {
	/** Confirms and restarts into an installed update while sessions are still busy. */
	onRestartNow?: () => void;
	repoPath?: string;
	runCommand?: string;
	onBranchClick?: () => void;
	onRun?: (shiftKey: boolean) => void;
	onReviewPr?: (repoPath: string, branchName: string, command: string) => void;
	/** Opens Settings on the Smart Prompts tab. */
	onOpenSettings?: () => void;
	onShowWhatsNew?: (version: string) => void;
}

export const Toolbar: Component<ToolbarProps> = (props) => {
	const [showNotifPopover, setShowNotifPopover] = createSignal(false);
	const [prDetailTarget, setPrDetailTarget] = createSignal<{ repoPath: string; branch: string } | null>(null);
	let notifRef: HTMLDivElement | undefined;

	// Close popover on outside click
	createEffect(() => {
		if (!showNotifPopover()) return;
		const handler = (e: MouseEvent) => {
			if (notifRef && !notifRef.contains(e.target as Node)) {
				setShowNotifPopover(false);
			}
		};
		document.addEventListener("mousedown", handler);
		onCleanup(() => document.removeEventListener("mousedown", handler));
	});

	// Tick signal to refresh relative ages in the popover (every 30s while open)
	const [ageTick, setAgeTick] = createSignal(0);
	createEffect(() => {
		if (!showNotifPopover()) return;
		const interval = setInterval(() => setAgeTick((n) => n + 1), 30_000);
		onCleanup(() => clearInterval(interval));
	});

	const activeNotifs = createMemo(() => prNotificationsStore.getActive());
	const activitySections = createMemo(() => activityStore.getSections().filter((s) => !s.panelOnly));
	const hasUpdate = () => updaterStore.state.available && !!updaterStore.state.version;
	/** Count only activity items visible in the popover (filtered by active repo) */
	const visibleActivityCount = createMemo(() => {
		const repoPath = repositoriesStore.state.activeRepoPath ?? undefined;
		return activityStore.countActiveInSections(
			activitySections().map((s) => s.id),
			repoPath,
		);
	});
	const totalBadgeCount = () =>
		activeNotifs().length + visibleActivityCount() + progressStore.unreadCount + (hasUpdate() ? 1 : 0);
	// Keys of last-items the user already "saw" by clicking the pill. Ephemeral
	// (per session): hides the pill without dismissing the underlying notification,
	// which stays in the bell dropdown until dismissed there.
	const [seenKeys, setSeenKeys] = createSignal<Set<string>>(new Set());
	const lastItem = createMemo(() => {
		const src = getLastItemAcrossStores(repositoriesStore.state.activeRepoPath);
		if (!src) return null;
		return seenKeys().has(lastItemKey(src)) ? null : src;
	});

	const activeBranch = () => {
		const activeRepoPath = repositoriesStore.state.activeRepoPath;
		if (!activeRepoPath) return null;
		const repo = repositoriesStore.state.repositories[activeRepoPath];
		if (!repo?.activeWorkspaceId) return null;
		return repo.workspaces[repo.activeWorkspaceId] || null;
	};

	const activeBranchName = () => activeBranch()?.branchName || null;

	const activeRepoName = () => {
		const activeRepoPath = repositoriesStore.state.activeRepoPath;
		if (!activeRepoPath) return null;
		const repo = repositoriesStore.state.repositories[activeRepoPath];
		return repo?.displayName || null;
	};

	/** Color inheritance: repo color > group color > default */
	const activeRepoColor = () => {
		const activeRepoPath = repositoriesStore.state.activeRepoPath;
		if (!activeRepoPath) return undefined;
		return getRepoTextColor(activeRepoPath);
	};

	const launchPath = () => activeBranch()?.worktreePath || props.repoPath;
	// Ahead/behind belongs to the selected branch's checkout, not the repo root
	const github = useGitHub(launchPath);

	const aheadBehind = () => {
		const gs = github.status();
		if (!gs) return null;
		if (gs.ahead > 0 && gs.behind > 0) return ` ↑${gs.ahead} ↓${gs.behind}`;
		if (gs.ahead > 0) return ` ↑${gs.ahead}`;
		if (gs.behind > 0) return ` ↓${gs.behind}`;
		return null;
	};

	const focusedFilePath = (): string | undefined => {
		const editTab = editorTabsStore.getActive();
		if (editTab) {
			return `${editTab.fsRoot}/${editTab.filePath}`;
		}
		const mdTab = mdTabsStore.getActive();
		if (mdTab?.type === "file") {
			return `${mdTab.repoPath}/${mdTab.filePath}`;
		}
		return undefined;
	};

	/** Open an activity item: virtual content tab or direct action */
	const openActivityItem = (item: ActivityItem) => {
		if (item.contentUri) {
			mdTabsStore.addVirtual(item.title, item.contentUri, item.repoPath);
		} else if (item.onClick) {
			item.onClick();
		}
	};

	/** The pill is a preview of the newest bell notification — clicking it opens the
	 *  bell dropdown (with full context + per-item actions) and marks this item seen,
	 *  so the pill clears. Unifies pill and bell instead of being a second action. */
	const handleLastItemClick = () => {
		const src = lastItem();
		if (!src) return;
		setSeenKeys((prev) => new Set(prev).add(lastItemKey(src)));
		setShowNotifPopover(true);
	};

	return (
		<div id="toolbar" class={s.toolbar} data-tauri-drag-region>
			<div class={s.left} data-tauri-drag-region>
				<span class={s.leftSpacer} data-tauri-drag-region />
				{/* Embossed app name —  */}
				<svg
					class={`${s.appName} ${s.appNameFull}`}
					data-tauri-drag-region
					viewBox="0 0 110 16"
					width="110"
					height="16"
					aria-label="TUICommander"
				>
					<defs>
						<linearGradient id="toolbar-name-grad" x1="0" y1="0" x2="110" y2="0" gradientUnits="userSpaceOnUse">
							<stop offset="0%" class={s.nameGradStart} />
							<stop offset="32%" class={s.nameGradMid} />
							<stop offset="100%" class={s.nameGradEnd} />
						</linearGradient>
					</defs>
					<text
						x="0"
						y="12"
						class={s.nameShadow}
						font-size="11"
						font-weight="700"
						letter-spacing="0.09em"
						font-family="system-ui,-apple-system,sans-serif"
						dx="1"
						dy="1"
					>
						TUICommander
					</text>
					<text
						x="0"
						y="12"
						class={s.nameHighlight}
						font-size="11"
						font-weight="700"
						letter-spacing="0.09em"
						font-family="system-ui,-apple-system,sans-serif"
						dx="-0.5"
						dy="-0.5"
					>
						TUICommander
					</text>
					<text
						x="0"
						y="12"
						fill="url(#toolbar-name-grad)"
						font-size="11"
						font-weight="700"
						letter-spacing="0.09em"
						font-family="system-ui,-apple-system,sans-serif"
					>
						TUICommander
					</text>
				</svg>
				<span class={s.leftSpacer} data-tauri-drag-region />
				<Show when={uiStore.state.sidebarVisible}>
					<Button
						size="compact"
						variant="ghost"
						class={cx(s.filterToggle, s.densityToggle)}
						classList={{
							[s.filterToggleActive]: uiStore.state.sidebarDensityMode !== "auto",
						}}
						onClick={() => uiStore.cycleSidebarDensityMode()}
						data-testid="sidebar-density-toggle"
						data-mode={uiStore.state.sidebarDensityMode}
						title={DENSITY_TITLES[uiStore.state.sidebarDensityMode]()}
						data-tooltip={DENSITY_TITLES[uiStore.state.sidebarDensityMode]()}
						data-tooltip-pos="bottom"
					>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
							<path
								d={DENSITY_ICON_PATHS[uiStore.state.sidebarDensityMode]}
								stroke="currentColor"
								stroke-width="1.3"
								stroke-linecap="round"
							/>
						</svg>
						{/* The mode is read off the button: title and tooltip do not show on touch. */}
						<span class={s.densityLabel} data-testid="sidebar-density-label">
							{DENSITY_LABELS[uiStore.state.sidebarDensityMode]()}
						</span>
					</Button>
					<Button
						size="compact"
						variant="ghost"
						class={s.filterToggle}
						classList={{ [s.filterToggleActive]: uiStore.state.repoFilterActiveOnly }}
						onClick={() => uiStore.toggleRepoFilter()}
						title={
							uiStore.state.repoFilterActiveOnly
								? t("toolbar.filterActiveOn", "Showing active repos only — click to show all")
								: t("toolbar.filterActiveOff", "Show only repos with open terminals")
						}
					>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
							<path
								d="M1.5 2.5h13l-5 6v5l-3 1.5v-6.5l-5-6Z"
								stroke="currentColor"
								stroke-width="1.3"
								stroke-linejoin="round"
							/>
						</svg>
					</Button>
				</Show>
				<Button
					size="compact"
					variant="ghost"
					class={s.sidebarToggle}
					onClick={() => uiStore.toggleSidebar()}
					title={
						uiStore.state.sidebarVisible
							? `${t("toolbar.hideSidebar", "Hide Sidebar")} (${keyFor("toggle-sidebar")})`
							: `${t("toolbar.showSidebar", "Show Sidebar")} (${keyFor("toggle-sidebar")})`
					}
				>
					{/* Panel-toggle icon: rounded panel + left-sidebar divider + chevron.
					    Chevron points left to collapse (sidebar visible) or right to expand. */}
					<svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
						<rect x="2" y="3" width="12" height="10" rx="1.6" stroke="currentColor" stroke-width="1.3" />
						<path d="M6.5 3v10" stroke="currentColor" stroke-width="1.3" />
						<path
							d={uiStore.state.sidebarVisible ? "M11 6 9 8l2 2" : "M9 6l2 2-2 2"}
							stroke="currentColor"
							stroke-width="1.3"
							stroke-linecap="round"
							stroke-linejoin="round"
						/>
					</svg>
				</Button>
			</div>

			<div class={s.center} data-tauri-drag-region>
				<For each={HISTORY_BUTTONS}>
					{(btn) => {
						const target = () => terminalsStore.getHistoryTarget(btn.direction);
						const title = () => {
							const id = target();
							if (!id) return btn.none();
							const name = terminalsStore.get(id)?.name ?? "";
							return `${btn.go(name)} (${keyFor(btn.action)})`;
						};
						return (
							<Button
								size="compact"
								variant="ghost"
								class={s.filterToggle}
								disabled={!target()}
								onClick={() => {
									const id = terminalsStore.stepHistory(btn.direction);
									if (id) navigateToTerminal(id);
								}}
								title={title()}
								data-tooltip={title()}
								data-tooltip-pos="bottom"
								data-testid={`history-${btn.direction}`}
							>
								<svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
									<path
										d={btn.icon}
										stroke="currentColor"
										stroke-width="1.5"
										stroke-linecap="round"
										stroke-linejoin="round"
									/>
								</svg>
							</Button>
						);
					}}
				</For>
				<Show when={activeBranchName()}>
					<Button
						size="compact"
						variant="ghost"
						class={s.branch}
						onClick={(e) => {
							e.stopPropagation();
							props.onBranchClick?.();
						}}
						title={t("toolbar.renameBranch", "Rename branch")}
					>
						<svg class={s.branchIcon} viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
							<path d="M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.493 2.493 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 1 1.5 0 .75.75 0 0 1-1.5 0z" />
						</svg>
						<Show when={activeRepoName()}>
							<span class={s.repoName} style={activeRepoColor() ? { color: activeRepoColor() } : undefined}>
								{activeRepoName()}
							</span>
							<span class={s.branchSeparator}>/</span>
						</Show>
						<span class={s.branchName}>{activeBranchName()}</span>
						<Show when={aheadBehind()}>{(ab) => <span class={s.aheadBehind}>{ab()}</span>}</Show>
					</Button>
				</Show>
			</div>

			<div class={s.right}>
				<Show when={props.repoPath}>
					<StoriesButton repoPath={props.repoPath as string} />
				</Show>
				{/* Notification group: smart prompts + last-item pill + bell.
				    The pill is a preview of the bell's newest notification, so the two
				    stay adjacent. */}
				<div class={s.notifGroup} ref={notifRef}>
					<SmartPromptsDropdown triggerSize="compact" repoPath={props.repoPath} onOpenSettings={props.onOpenSettings} />

					{/* Last-item shortcut — only when there are items */}
					<Show when={lastItem()}>
						{(src) => {
							const activitySrc = () =>
								src().kind === "activity" ? (src() as { kind: "activity"; item: ActivityItem }) : null;
							const prSrc = () => (src().kind === "pr" ? (src() as { kind: "pr"; notif: PrNotification }) : null);
							const updateSrc = () => (src().kind === "update" ? (src() as { kind: "update"; version: string }) : null);
							const sevClass = {
								error: s.lastItemError,
								success: s.lastItemSuccess,
								warn: s.lastItemWarn,
								info: undefined,
							}[lastItemSeverity(src())];
							return (
								<Button
									size="compact"
									variant="ghost"
									class={cx(s.lastItemBtn, sevClass)}
									onClick={handleLastItemClick}
									title={(() => {
										const v = src();
										if (v.kind === "update") return `Update to v${v.version}`;
										if (v.kind === "activity") return v.item.title;
										return v.notif.branch;
									})()}
								>
									<Show when={updateSrc()} keyed>
										{(us) => (
											<>
												<svg class={s.lastItemIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
													<path d="M8 16A8 8 0 1 1 8 0a8 8 0 0 1 0 16zm.93-9.412-1 4.705c-.07.34.029.533.304.533.194 0 .487-.07.686-.246l-.088.416c-.287.346-.92.598-1.465.598-.703 0-1.002-.422-.808-1.319l.738-3.468c.064-.293.006-.399-.287-.399l-.008-.078.012-.058h1.916zm.01-2.54a.96.96 0 1 0 0-1.92.96.96 0 0 0 0 1.92z" />
												</svg>
												<span class={s.lastItemTitle}>Update v{us.version}</span>
											</>
										)}
									</Show>
									<Show when={activitySrc()} keyed>
										{(as) => (
											<>
												<span class={s.lastItemIcon} innerHTML={as.item.icon} />
												<span class={s.lastItemTitle}>{as.item.title}</span>
											</>
										)}
									</Show>
									<Show when={prSrc()} keyed>
										{(ps) => (
											<>
												<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
													<path d="M1.5 3.25a2.25 2.25 0 1 1 3 2.122v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.25 2.25 0 0 1 1.5 3.25Zm5.677-.177L9.573.677A.25.25 0 0 1 10 .854V2.5h1A2.5 2.5 0 0 1 13.5 5v5.628a2.251 2.251 0 1 1-1.5 0V5a1 1 0 0 0-1-1h-1v1.646a.25.25 0 0 1-.427.177L7.177 3.427a.25.25 0 0 1 0-.354Z" />
												</svg>
												<span class={s.lastItemTitle}>{ps.notif.branch}</span>
											</>
										)}
									</Show>
								</Button>
							);
						}}
					</Show>

					{/* Bell — always visible */}
					<Button
						size="compact"
						variant="ghost"
						class={s.bell}
						onClick={() => setShowNotifPopover(!showNotifPopover())}
						title={`${totalBadgeCount()} ${t("toolbar.notifications", "notification(s)")}`}
					>
						<svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
							<path d="M12 22c1.1 0 2-.9 2-2h-4c0 1.1.9 2 2 2zm6-6v-5c0-3.07-1.63-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.64 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z" />
						</svg>
						<Show when={totalBadgeCount() > 0}>
							<span class={s.notifCount}>{totalBadgeCount()}</span>
						</Show>
					</Button>
					<Show when={showNotifPopover()}>
						<div class={s.popover}>
							<div class={s.sectionHeader}>
								<span class={s.sectionLabel}>PROGRESS</span>
								<Show when={progressStore.unreadCount > 0}>
									<Button size="compact" variant="ghost" class={s.dismissAll} onClick={() => progressStore.markRead()}>
										{t("toolbar.markAsRead", "Mark as read")}
									</Button>
								</Show>
							</div>
							<div
								class={s.activityItem}
								onClick={(event) => {
									event.stopPropagation();
									setShowNotifPopover(false);
									progressStore.open(repositoriesStore.state.activeRepoPath);
								}}
							>
								<span class={s.activityItemIcon}>
									<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
										<path d="M2 13.5h12v1H1.5a.5.5 0 0 1-.5-.5V1h1v12.5zM4 10l2.5-3 2 2L13 3.5l1 1-5.5 7-2-2L5 11z" />
									</svg>
								</span>
								<div class={s.activityItemBody}>
									<span class={s.activityItemTitle}>Terminal Progress</span>
									<span class={s.activityItemSubtitle}>
										<Show when={progressStore.unreadCount > 0} fallback="Open project journal">
											{progressStore.unreadCount} new update{progressStore.unreadCount === 1 ? "" : "s"}
										</Show>
									</span>
								</div>
							</div>
							{/* App update section */}
							<Show when={hasUpdate()}>
								<div class={s.notifHeader}>
									<span class={s.notifTitle}>{t("toolbar.appUpdate", "APP UPDATE")}</span>
									<Button size="compact" variant="ghost" class={s.dismissAll} onClick={() => updaterStore.dismiss()}>
										{t("toolbar.dismiss", "Dismiss")}
									</Button>
								</div>
								<div
									class={s.notifItem}
									onClick={(e) => {
										e.stopPropagation();
										setShowNotifPopover(false);
										if (updaterStore.state.restartPending) props.onRestartNow?.();
										else updaterStore.downloadAndInstall();
									}}
								>
									<span class={s.notifIcon}>
										<svg viewBox="0 0 16 16" width="14" height="14" fill="var(--success)">
											<path d="M8 16A8 8 0 1 1 8 0a8 8 0 0 1 0 16zm1-11H7v4H5l3 3 3-3H9V5z" />
										</svg>
									</span>
									<div class={s.notifDetails}>
										<span class={s.notifPr}>
											{`v${updaterStore.state.version} ${t("toolbar.available", "available")}`}
										</span>
										<span class={s.notifBranch}>
											{updaterStore.state.restartPending
												? t(
														"toolbar.restartPending",
														"Restarts when agents are idle ({count} busy) — click to restart now",
														{
															count: String(updaterStore.busyCount()),
														},
													)
												: updaterStore.state.downloading
													? `${t("statusBar.updating", "Updating")} ${updaterStore.state.progress}%`
													: t("toolbar.clickToUpdate", "Click to update")}
										</span>
										<Show
											when={
												updaterStore.state.version &&
												(releaseNotes as Record<string, unknown>)[updaterStore.state.version.replace(/[-+].*$/, "")] &&
												props.onShowWhatsNew
											}
										>
											<span
												class={s.notifBranch}
												style={{ color: "var(--accent)", cursor: "pointer" }}
												onClick={(e) => {
													e.stopPropagation();
													const v = updaterStore.state.version?.replace(/[-+].*$/, "");
													if (v) props.onShowWhatsNew?.(v);
												}}
											>
												What's new?
											</span>
										</Show>
									</div>
								</div>
							</Show>

							{/* Plugin activity sections (shown above PR updates) */}
							<For each={activitySections()}>
								{(section) => {
									const sectionItems = () =>
										activityStore.getForSection(section.id, repositoriesStore.state.activeRepoPath ?? undefined);
									const sectionPluginId = () => section.pluginId;
									const sectionPlugin = () =>
										sectionPluginId() ? pluginStore.getPlugin(sectionPluginId()!) : undefined;
									const isExternal = () => {
										const p = sectionPlugin();
										return p && !p.builtIn;
									};
									return (
										<Show when={sectionItems().length > 0}>
											<div class={s.sectionHeader}>
												<span class={s.sectionLabel}>{section.label}</span>
												<div class={s.sectionActions}>
													<Show when={isExternal() && sectionPlugin()}>
														<Button
															size="compact"
															variant="ghost"
															class={cx(s.pluginToggle, sectionPlugin()!.paused && s.pluginToggleOff)}
															onClick={(e) => {
																e.stopPropagation();
																const p = sectionPlugin()!;
																pluginStore.setPaused(p.id, !p.paused);
															}}
															title={sectionPlugin()!.paused ? "Resume plugin" : "Pause plugin"}
														>
															{sectionPlugin()!.paused ? (
																<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
																	<path d="M4 2l10 6-10 6z" />
																</svg>
															) : (
																<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
																	<path d="M3 2h3v12H3zm7 0h3v12h-3z" />
																</svg>
															)}
														</Button>
													</Show>
													<Show when={section.canDismissAll}>
														<Button
															size="compact"
															variant="ghost"
															class={s.activityDismissAll}
															onClick={() => activityStore.dismissSection(section.id)}
														>
															{t("toolbar.dismissAll", "Dismiss All")}
														</Button>
													</Show>
												</div>
											</div>
											<For each={sectionItems()}>
												{(item) => (
													<div
														class={s.activityItem}
														onClick={(e) => {
															e.stopPropagation();
															setShowNotifPopover(false);
															openActivityItem(item);
														}}
													>
														<span class={s.activityItemIcon} innerHTML={item.icon} />
														<div class={s.activityItemBody}>
															<span class={s.activityItemTitle}>{item.title}</span>
															<span class={s.activityItemSubtitle}>
																{ageTick() >= 0 && relativeAge(item.createdAt)}
																{item.subtitle ? ` · ${item.subtitle}` : ""}
															</span>
														</div>
														<Show when={item.dismissible}>
															<Button
																size="compact"
																variant="ghost"
																class={s.activityItemDismiss}
																onClick={(e) => {
																	e.stopPropagation();
																	activityStore.dismissItem(item.id);
																}}
															>
																&times;
															</Button>
														</Show>
													</div>
												)}
											</For>
										</Show>
									);
								}}
							</For>

							{/* PR Updates section */}
							<Show when={activeNotifs().length > 0}>
								<div class={s.notifHeader}>
									<span class={s.notifTitle}>{t("toolbar.prUpdates", "PR UPDATES")}</span>
									<Button
										size="compact"
										variant="ghost"
										class={s.dismissAll}
										onClick={() => {
											prNotificationsStore.dismissAll();
											setShowNotifPopover(false);
										}}
									>
										{t("toolbar.dismissAll", "Dismiss All")}
									</Button>
								</div>
								<For each={activeNotifs()}>
									{(notif) => {
										const info = NOTIFICATION_LABELS[notif.type];
										return (
											<div
												class={cx(s.notifItem, info.cls)}
												onClick={(e) => {
													e.stopPropagation();
													setShowNotifPopover(false);
													requestAnimationFrame(() => {
														if (getRepoConnection(notif.repoPath) !== notif.connectionId) return;
														setPrDetailTarget({ repoPath: notif.repoPath, branch: notif.branch });
													});
												}}
											>
												<span class={s.notifIcon}>{info.icon}</span>
												<div class={s.notifDetails}>
													<span
														class={s.notifRepo}
														style={(() => {
															const color = getRepoTextColor(notif.repoPath);
															return color ? { color } : undefined;
														})()}
													>
														{repositoriesStore.get(notif.repoPath)?.displayName ?? notif.repoPath.split(/[\\/]/).pop()}
													</span>
													<span class={s.notifPr}>
														#{notif.prNumber} {info.label}
													</span>
													<span class={s.notifBranch} title={notif.title}>
														{notif.branch}
													</span>
												</div>
												<Button
													size="compact"
													variant="ghost"
													class={s.notifClose}
													onClick={(e) => {
														e.stopPropagation();
														prNotificationsStore.dismiss(notif.id);
													}}
												>
													&times;
												</Button>
											</div>
										);
									}}
								</For>
							</Show>

							{/* Empty state */}
							<Show when={totalBadgeCount() === 0}>
								<div class={s.emptyState}>{t("toolbar.noNotifications", "No notifications")}</div>
							</Show>
						</div>
					</Show>
				</div>

				{/* IdeLauncher launches local external editors — impossible from a
				    browser, where it renders nothing. In that slot show a Command
				    Palette button instead, since browser-desktop has no native menu
				    and keyboard shortcuts may be swallowed by the browser. */}
				<Show
					when={isTauri()}
					fallback={
						<Button
							size="compact"
							variant="ghost"
							type="button"
							class={s.watcherBtn}
							onClick={() => commandPaletteStore.toggle()}
							title={t("toolbar.commandPalette", "Command palette ({key})", {
								key: keyFor("command-palette"),
							})}
							aria-label="Command palette"
							aria-controls="command-palette"
							aria-expanded={commandPaletteStore.state.isOpen}
						>
							<svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
								<path d="M15.5 14h-.79l-.28-.27a6.471 6.471 0 0 0 1.48-5.34c-.47-2.78-2.79-5-5.59-5.34a6.505 6.505 0 0 0-7.27 7.27c.34 2.8 2.56 5.12 5.34 5.59a6.471 6.471 0 0 0 5.34-1.48l.27.28v.79l4.25 4.25c.41.41 1.08.41 1.49 0 .41-.41.41-1.08 0-1.49L15.5 14zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z" />
							</svg>
						</Button>
					}
				>
					<IdeLauncher
						repoPath={launchPath()}
						focusedFilePath={focusedFilePath()}
						cwd={terminalsStore.getActive()?.cwd ?? undefined}
						cursorLine={editorTabsStore.getActive()?.cursorLine}
						cursorCol={editorTabsStore.getActive()?.cursorCol}
						runCommand={props.runCommand}
						onRun={props.onRun}
					/>
				</Show>
			</div>

			{/* PR detail popover triggered from notification click */}
			<Show when={prDetailTarget()}>
				{(target) => (
					<PrDetailPopover
						repoPath={target().repoPath}
						branch={target().branch}
						anchor="top"
						onClose={() => setPrDetailTarget(null)}
						onReview={props.onReviewPr}
					/>
				)}
			</Show>
		</div>
	);
};

export default Toolbar;
