import {
	batch,
	type Component,
	createEffect,
	createMemo,
	createSignal,
	For,
	Match,
	on,
	onCleanup,
	onMount,
	Show,
	Switch,
} from "solid-js";
import { openPathsAsTabs } from "../../hooks/useFileDrop";
import { initMouseDrag } from "../../hooks/useMouseDrag";
import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import { designModeBrowsers, designModeStore } from "../../stores/designMode";
import { diffTabsStore } from "../../stores/diffTabs";
import { findPaneGroupAtPoint } from "../../stores/dragDrop";
import { editorTabsStore } from "../../stores/editorTabs";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { mdTabsStore, type PluginPanelTab } from "../../stores/mdTabs";
import { paneLayoutStore } from "../../stores/paneLayout";
import { currentBranchKey, repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { tabOrderingStore } from "../../stores/tabManager";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import { cx } from "../../utils";
import { copyPathToClipboard, writeClipboard } from "../../utils/clipboard";
import { keyFor } from "../../utils/hotkey";
import { markdownDocumentPanelId } from "../../utils/markdownDocumentPanelId";
import { handleOpenUrl, openLocalPath } from "../../utils/openUrl";
// Named for its first caller; it is a plain `file://` → path parser and returns
// null for every other scheme, which is exactly the guard needed here.
import { parseOsc7Url } from "../../utils/osc7";
import { orderTerminalsByPaneLayout } from "../../utils/paneTabOrder";
import { computeLeafRects } from "../../utils/paneTreeGeometry";
import { isAbsolutePath, joinPath } from "../../utils/pathUtils";
import { isPerfDebug } from "../../utils/perfDebug";
import { fileContextSmartMenuItem } from "../../utils/promptContext";
import { ptyCaptureStore } from "../../utils/ptyCapture";
import { resumeTerminal, type SuspendOutcome, suspendRefusal, suspendTerminal } from "../../utils/suspendTerminal";
import type { ContextMenuItem } from "../ContextMenu/ContextMenu";
import { ContextMenu, createContextMenu } from "../ContextMenu/ContextMenu";
import { createAgentLaunchMenu } from "../ContextMenu/createAgentLaunchMenu";
import { Button } from "../ui/Button";
import s from "./TabBar.module.css";
import { DiffTabView, EditorTabView, MarkdownTabView, TerminalTabView } from "./TabViews";

export interface TabBarProps {
	quickSwitcherActive?: boolean;
	tabHintsActive?: boolean;
	onTabSelect: (id: string) => void;
	onTabClose: (id: string, skipConfirm?: boolean) => void;
	onCloseOthers: (id: string) => void;
	onCloseToRight: (id: string) => void;
	onNewTab: () => void;
	/** Agents a long press on the + button offers; each opens a new tab running that agent. */
	getNewAgentMenuItems?: () => ContextMenuItem[];
	onReorder?: (fromIndex: number, toIndex: number) => void;
	onDetachTab?: (id: string) => void;
	onReattachTab?: (id: string) => void;
	onFocusDetachedTab?: (id: string) => void;
	onMoveToWorktree?: (terminalId: string, worktreePath: string) => void;
	getWorktreeTargets?: (terminalId: string) => Array<{ branchName: string; path: string }>;
}

export const TabBar: Component<TabBarProps> = (props) => {
	onMount(() => onCleanup(designModeStore.subscribe()));
	const designModeForTab = (id: string) => designModeStore.forSession(terminalsStore.get(id)?.sessionId);
	const [dragOverId, setDragOverId] = createSignal<string | null>(null);
	const [dragOverSide, setDragOverSide] = createSignal<"left" | "right" | null>(null);
	const [draggingId, setDraggingId] = createSignal<string | null>(null);
	const [dragInvalid, setDragInvalid] = createSignal(false);
	const [editingId, setEditingId] = createSignal<string | null>(null);
	const smartPrompts = useSmartPrompts();

	/** Absolute on-disk path of a file-backed tab.
	 *
	 *  The tab stores keep `filePath` RELATIVE to the tab's filesystem root, which
	 *  is held next to it (`fsRoot`, falling back to `repoPath`); a file opened
	 *  from outside any repo is already absolute. Copy Path and the file-context
	 *  Smart Prompts both need the joined path — handing them the bare `filePath`
	 *  yields `src/foo.ts`, which resolves against whatever the consumer's cwd
	 *  happens to be. */
	const tabAbsPath = (root: string | undefined, filePath: string | undefined): string | undefined => {
		if (!filePath) return undefined;
		if (isAbsolutePath(filePath)) return filePath;
		return root ? joinPath(root, filePath) : filePath;
	};

	/** Append a Smart Prompts submenu to a tab context menu if any file-context
	 *  prompts exist. Mutates `items` in-place. */
	const pushFileContextItem = (items: ContextMenuItem[], filePath: string | undefined) => {
		if (!filePath) return;
		const repoRoot = repositoriesStore.getActive()?.path ?? null;
		const item = fileContextSmartMenuItem({ absPath: filePath, repoRoot, isDir: false }, smartPrompts, {
			separator: true,
		});
		if (item) items.push(item);
	};

	// Context menu for tabs
	const tabMenu = createContextMenu();
	const [contextTabId, setContextTabId] = createSignal<string | null>(null);

	// Context menu for overflow tabs (right-click on scroll arrows)
	const overflowMenu = createContextMenu();

	// Shared memo for pane position mini-map (avoids N redundant tree walks)
	const paneRects = createMemo(() => {
		const root = paneLayoutStore.getRoot();
		return root ? computeLeafRects(root) : [];
	});
	// Long press or right click on + lists the agents (see createAgentLaunchMenu).
	const newAgentMenu = createAgentLaunchMenu(() => props.getNewAgentMenuItems?.() ?? []);

	const handleNewTabClick = () => {
		if (newAgentMenu.consumeClick()) return;
		props.onNewTab();
	};

	const reportOutcome = async (pending: Promise<SuspendOutcome>) => {
		const outcome = await pending;
		if (!outcome.ok) toastsStore.add(t("tabBar.suspendRefused", "Tab not changed"), outcome.reason, "warn");
	};

	const getTabContextMenuItems = (): ContextMenuItem[] => {
		const id = contextTabId();
		if (!id) return [];

		if (id.startsWith("diff-")) {
			const tab = diffTabsStore.get(id);
			const ids = visibleDiffIds();
			const idx = ids.indexOf(id);
			const isPinned = tab?.pinned ?? false;
			const absPath = tabAbsPath(tab?.repoPath, tab?.filePath);
			const diffItems: ContextMenuItem[] = [
				{
					label: t("tabBar.copyPath", "Copy Path"),
					action: () => {
						if (absPath) copyPathToClipboard(absPath);
					},
				},
				{
					label: isPinned ? t("tabBar.unpinTab", "Unpin Tab") : t("tabBar.pinTab", "Pin Tab"),
					action: () => diffTabsStore.setPinned(id, !isPinned),
				},
				{ label: "", separator: true, action: () => {} },
				{
					label: t("tabBar.closeTab", "Close Tab"),
					action: () => {
						diffTabsStore.remove(id);
						props.onTabClose(id);
					},
				},
				{
					label: t("tabBar.closeOthers", "Close Other Tabs"),
					action: () => props.onCloseOthers(id),
					disabled: ids.length <= 1,
				},
				{
					label: t("tabBar.closeRight", "Close Tabs to the Right"),
					action: () => props.onCloseToRight(id),
					disabled: idx >= ids.length - 1,
				},
			];
			pushFileContextItem(diffItems, absPath);
			return diffItems;
		}

		if (id.startsWith("md-")) {
			const tab = mdTabsStore.get(id);
			const ids = visibleMdIds();
			const idx = ids.indexOf(id);
			const isPinned = tab?.pinned ?? false;
			const tabUrl = tab?.type === "plugin-panel" ? (tab as PluginPanelTab).url : undefined;
			// Three md-tab shapes are backed by a real file and all three should offer
			// Copy Path. The `file://` panel is the one that reads as memory-only and
			// is not: `ui action=tab url=file://…` keeps the url on the tab while
			// PluginPanel reads the file over IPC and renders it as `srcdoc`, because
			// the iframe sandbox blocks `file://` directly. The document on screen has
			// no src, so only the url names where it came from.
			const absPath =
				tab?.type === "file" || tab?.type === "html-preview"
					? tabAbsPath(tab.fsRoot || tab.repoPath, tab.filePath)
					: (parseOsc7Url(tabUrl ?? "") ?? undefined);
			const mdItems: ContextMenuItem[] = [
				...(absPath
					? [
							{
								label: t("tabBar.copyPath", "Copy Path"),
								action: () => copyPathToClipboard(absPath),
							},
						]
					: []),
				// A `file://` panel goes to the OS default app for the file (a browser,
				// for the HTML these tabs carry). `handleOpenUrl` rejects every scheme
				// but http/https/mailto, so routing one through it left the menu item
				// doing nothing but writing "Blocked URL with disallowed scheme" to
				// the log.
				...(tabUrl
					? [
							{
								label: t("tabBar.openInBrowser", "Open in Browser"),
								action: () =>
									tabUrl.startsWith("file://") && absPath ? openLocalPath(absPath) : handleOpenUrl(tabUrl),
							},
						]
					: []),
				{
					label: isPinned ? t("tabBar.unpinTab", "Unpin Tab") : t("tabBar.pinTab", "Pin Tab"),
					action: () => mdTabsStore.setPinned(id, !isPinned),
				},
				{ label: t("tabBar.print", "Print…"), action: () => window.print() },
				...(tab?.type === "file"
					? [
							uiStore.isDetached(markdownDocumentPanelId(id))
								? {
										label: t("tabBar.reattachTab", "Reattach to Main Window"),
										action: () => props.onReattachTab?.(id),
									}
								: {
										label: t("tabBar.detachToWindow", "Detach to Window"),
										action: () => props.onDetachTab?.(id),
									},
						]
					: []),
				{ label: "", separator: true, action: () => {} },
				{
					label: t("tabBar.closeTab", "Close Tab"),
					action: () => {
						mdTabsStore.remove(id);
						props.onTabClose(id);
					},
				},
				{
					label: t("tabBar.closeOthers", "Close Other Tabs"),
					action: () => props.onCloseOthers(id),
					disabled: ids.length <= 1,
				},
				{
					label: t("tabBar.closeRight", "Close Tabs to the Right"),
					action: () => props.onCloseToRight(id),
					disabled: idx >= ids.length - 1,
				},
			];
			pushFileContextItem(mdItems, absPath);
			return mdItems;
		}

		if (id.startsWith("edit-")) {
			const tab = editorTabsStore.get(id);
			const ids = visibleEditIds();
			const idx = ids.indexOf(id);
			const isPinned = tab?.pinned ?? false;
			const absPath = tabAbsPath(tab?.fsRoot, tab?.filePath);
			const editItems: ContextMenuItem[] = [
				{
					label: t("tabBar.copyPath", "Copy Path"),
					action: () => {
						if (absPath) copyPathToClipboard(absPath);
					},
				},
				{
					label: isPinned ? t("tabBar.unpinTab", "Unpin Tab") : t("tabBar.pinTab", "Pin Tab"),
					action: () => editorTabsStore.setPinned(id, !isPinned),
				},
				{ label: "", separator: true, action: () => {} },
				{ label: t("tabBar.closeTab", "Close Tab"), action: () => props.onTabClose(id) },
				{
					label: t("tabBar.closeOthers", "Close Other Tabs"),
					action: () => props.onCloseOthers(id),
					disabled: ids.length <= 1,
				},
				{
					label: t("tabBar.closeRight", "Close Tabs to the Right"),
					action: () => props.onCloseToRight(id),
					disabled: idx >= ids.length - 1,
				},
			];
			pushFileContextItem(editItems, absPath);
			return editItems;
		}

		// Terminal tab
		const ids = activeTerminals();
		const idx = ids.indexOf(id);
		const term = terminalsStore.get(id);
		const hasSession = !!term?.sessionId;
		const exited = term?.shellState === "exited";
		const detached = terminalsStore.isDetached(id);
		const worktreeTargets = props.getWorktreeTargets?.(id) ?? [];
		const items: ContextMenuItem[] = [
			{
				label: exited ? t("tabBar.removeTab", "Remove Tab") : t("tabBar.closeTab", "Close Tab"),
				shortcut: keyFor("close-terminal"),
				action: () => props.onTabClose(id),
			},
			{
				label: t("tabBar.closeOthers", "Close Other Tabs"),
				action: () => props.onCloseOthers(id),
				disabled: ids.length <= 1,
			},
			{
				label: t("tabBar.closeRight", "Close Tabs to the Right"),
				action: () => props.onCloseToRight(id),
				disabled: idx >= ids.length - 1,
			},
			{ label: "", separator: true, action: () => {} },
			{ label: t("tabBar.renameTab", "Rename Tab"), action: () => setEditingId(id) },
			detached
				? {
						label: t("tabBar.reattachTab", "Reattach to Main Window"),
						action: () => props.onReattachTab?.(id),
						disabled: exited,
					}
				: {
						label: t("tabBar.detachToWindow", "Detach to Window"),
						action: () => props.onDetachTab?.(id),
						disabled: !hasSession || exited,
					},
			term?.suspended
				? {
						label: t("tabBar.resumeTab", "Resume Tab"),
						action: () => void reportOutcome(resumeTerminal(id)),
					}
				: {
						label: t("tabBar.suspendTab", "Suspend Tab"),
						action: () => void reportOutcome(suspendTerminal(id)),
						disabled: !term || suspendRefusal(term) !== null,
					},
		];
		if (worktreeTargets.length > 0) {
			items.push(
				{ label: "", separator: true, action: () => {} },
				{
					label: t("tabBar.moveToWorktree", "Move to Worktree"),
					action: () => {},
					children: worktreeTargets.map((wt) => ({
						label: wt.branchName,
						action: () => props.onMoveToWorktree?.(id, wt.path),
					})),
				},
			);
		}
		// The alias is the address another agent is told to notify, so it has to be
		// readable and copyable from the tab that owns it — otherwise it exists only
		// inside an MCP payload.
		if (term?.alias) {
			const alias = term.alias;
			items.push(
				{ label: "", separator: true, action: () => {} },
				{
					label: t("tabBar.alias", "Alias: {alias}", { alias }),
					action: () => {
						writeClipboard(alias).catch((err) => appLogger.error("app", "Failed to copy alias", err));
					},
				},
			);
		}
		// Global workspace promote/unpromote
		const isPromoted = globalWorkspaceStore.isPromoted(id);
		items.push(
			{ label: "", separator: true, action: () => {} },
			{
				label: isPromoted
					? t("tabBar.removeFromWorkspace", "Remove from Global Workspace")
					: t("tabBar.promoteToWorkspace", "Add to Global Workspace"),
				action: () => globalWorkspaceStore.togglePromote(id),
			},
		);
		// Raw PTY capture — the evidence a state-detection bug needs has to be
		// recorded BEFORE the reproduction, so it belongs one click from the tab
		// that misbehaves. Gated on the same debug flag as the rest of the
		// instrumentation: on in dev, wakeable in a release build.
		const sessionId = term?.sessionId;
		if (term?.agentType && sessionId && !exited) {
			const isArmed = designModeStore.isArmed(sessionId);
			items.push(
				{ label: "", separator: true, action: () => {} },
				{
					label: isArmed
						? t("tabBar.stopDesignMode", "Stop Design Mode")
						: t("tabBar.startDesignMode", "Start Design Mode"),
					action: () => {
						if (isArmed) designModeStore.toggle(sessionId);
					},
					...(!isArmed
						? {
								children: designModeBrowsers.map((browser) => ({
									label: browser.label,
									action: () => designModeStore.toggle(sessionId, browser.id),
								})),
							}
						: {}),
				},
			);
		}
		if (isPerfDebug() && sessionId) {
			const recording = ptyCaptureStore.isRecording(sessionId);
			items.push(
				{ label: "", separator: true, action: () => {} },
				{
					label: recording
						? t("tabBar.stopCapture", "Stop Capture Session")
						: t("tabBar.captureSession", "Capture Session"),
					action: () => void ptyCaptureStore.toggle(sessionId),
				},
			);
		}
		// Plugin-registered tab actions
		const tabActions = contextMenuActionsStore.getContextActions("tab");
		if (tabActions.length > 0) {
			const ctx = { target: "tab" as const, tabId: id, sessionId: terminalsStore.get(id)?.sessionId ?? undefined };
			for (const a of tabActions) {
				items.push({
					label: a.label,
					action: () => a.action(ctx),
					disabled: a.disabled?.(ctx),
					separator: tabActions.indexOf(a) === 0,
				});
			}
		}
		return items;
	};

	const openTabContextMenu = (e: MouseEvent, id: string) => {
		e.preventDefault();
		e.stopPropagation();
		setContextTabId(id);
		// The tap is one global switch that curl and other windows can also flip,
		// so read it back rather than trusting the last value this window wrote.
		if (isPerfDebug()) void ptyCaptureStore.refresh();
		tabMenu.open(e);
	};

	// Get terminals for active branch only, ordered by pane layout when split.
	// When global workspace is active, show only promoted terminals instead.
	let prevTerminalIds: string[] = [];
	const activeTerminals = () => {
		let ids: string[];
		if (globalWorkspaceStore.isActive()) {
			ids = globalWorkspaceStore.getPromotedIds();
		} else {
			const activeRepoPath = repositoriesStore.state.activeRepoPath;
			if (!activeRepoPath) {
				ids = terminalsStore.getIds();
			} else {
				const repo = repositoriesStore.state.repositories[activeRepoPath];
				if (!repo?.activeWorkspaceId) {
					ids = terminalsStore.getIds();
				} else {
					const branch = repo.workspaces[repo.activeWorkspaceId];
					ids = branch?.terminals || [];
				}
			}
		}

		// During branch switch, hold previous tab list to prevent flash of empty tabs
		if (ids.length === 0 && repositoriesStore.state.branchSwitching) {
			return prevTerminalIds;
		}

		prevTerminalIds = ids;

		// Diagnostic: detect when TabBar shows 0 terminals but the store has some
		// that belong to the current branch (ignore terminals from other repos/branches)
		if (ids.length === 0 && !repositoriesStore.state.branchSwitching) {
			const activeRepoPath = repositoriesStore.state.activeRepoPath;
			const repo = activeRepoPath ? repositoriesStore.state.repositories[activeRepoPath] : null;
			const activeBranch = repo?.activeWorkspaceId;
			const branchTerminals = activeBranch ? repo!.workspaces[activeBranch]?.terminals : null;
			if (branchTerminals && branchTerminals.length > 0) {
				appLogger.warn("app", "TabBar: activeTerminals empty but branch has terminals", {
					branchTerminals,
					activeRepoPath,
					activeBranch,
					activeId: terminalsStore.state.activeId,
				});
			}
		}

		// In split mode, reorder tabs to match the spatial pane layout (DFS order)
		return orderTerminalsByPaneLayout(ids);
	};

	// Branch key for filtering non-terminal tabs. A legitimate use of focus: this
	// asks "what should be on screen for the repo the user is looking at", not
	// "which repo owns this tab".
	const activeBranchKey = () => {
		const repoPath = repositoriesStore.state.activeRepoPath;
		// A repo may be active before workspace discovery completes. Keep its
		// repo-scoped, branchless tabs visible when they stop being active.
		return currentBranchKey() ?? (repoPath ? `${repoPath}|` : null);
	};

	const visibleDiffIds = () => diffTabsStore.getVisibleIds(activeBranchKey());
	const visibleMdIds = () => mdTabsStore.getVisibleIds(activeBranchKey());
	const visibleEditIds = () => editorTabsStore.getVisibleIds(activeBranchKey());

	const isGroupedMode = () => settingsStore.state.tabOrderingMode === "grouped-by-type";
	const isFreeMode = () => settingsStore.state.tabOrderingMode === "free";

	const mergedNonTerminalIds = () => {
		if (isGroupedMode()) return [];
		if (isFreeMode()) {
			const allIds = new Set([...activeTerminals(), ...visibleDiffIds(), ...visibleMdIds(), ...visibleEditIds()]);
			return tabOrderingStore.getOrdered(allIds);
		}
		const allIds = new Set([...visibleDiffIds(), ...visibleMdIds(), ...visibleEditIds()]);
		return tabOrderingStore.getOrdered(allIds);
	};

	// Deactivate non-terminal tabs that become invisible after a BRANCH SWITCH.
	// Gated on activeBranchKey (with defer) so it fires only on an actual branch
	// change — NOT on every tab open. A plain createEffect here re-ran whenever a
	// tab was added (activeId change) and nuked a just-opened diff/md/editor whose
	// branchKey didn't yet match the active branch (e.g. opening a diff from a
	// worktree's Git panel), so the click appeared to do nothing.
	createEffect(
		on(
			activeBranchKey,
			() => {
				const diffActive = diffTabsStore.state.activeId;
				if (diffActive && !diffTabsStore.getVisibleIds(activeBranchKey(), false).includes(diffActive)) {
					diffTabsStore.setActive(null);
				}
				const mdActive = mdTabsStore.state.activeId;
				if (mdActive && !mdTabsStore.getVisibleIds(activeBranchKey(), false).includes(mdActive)) {
					mdTabsStore.setActive(null);
				}
				const editActive = editorTabsStore.state.activeId;
				if (editActive && !editorTabsStore.getVisibleIds(activeBranchKey(), false).includes(editActive)) {
					editorTabsStore.setActive(null);
				}
			},
			{ defer: true },
		),
	);

	// Unpinning a cross-repo MCP tab restores its opening-repo scope immediately.
	let previousMdPin: { id: string | null; pinned: boolean | undefined } | null = null;
	createEffect(() => {
		const id = mdTabsStore.state.activeId;
		const pinned = id ? mdTabsStore.get(id)?.pinned : undefined;
		if (
			id &&
			previousMdPin?.id === id &&
			previousMdPin.pinned &&
			!pinned &&
			!mdTabsStore.getVisibleIds(activeBranchKey(), false).includes(id)
		) {
			mdTabsStore.setActive(null);
		}
		previousMdPin = { id, pinned };
	});
	let previousEditPin: { id: string | null; pinned: boolean | undefined } | null = null;
	createEffect(() => {
		const id = editorTabsStore.state.activeId;
		const pinned = id ? editorTabsStore.get(id)?.pinned : undefined;
		if (
			id &&
			previousEditPin?.id === id &&
			previousEditPin.pinned &&
			!pinned &&
			!editorTabsStore.getVisibleIds(activeBranchKey(), false).includes(id)
		) {
			editorTabsStore.setActive(null);
		}
		previousEditPin = { id, pinned };
	});

	const tabTypeOf = (tabId: string): "terminal" | "markdown" | "diff" | "editor" | null => {
		if (activeTerminals().includes(tabId)) return "terminal";
		if (visibleMdIds().includes(tabId)) return "markdown";
		if (visibleDiffIds().includes(tabId)) return "diff";
		if (visibleEditIds().includes(tabId)) return "editor";
		return null;
	};

	// Mouse-based drag — replaces HTML5 DnD which conflicts with Tauri dragDropEnabled=true
	const handleMouseDrag = (
		e: PointerEvent,
		id: string,
		_tabType: "terminal" | "markdown" | "diff" | "editor" = "terminal",
	) => {
		initMouseDrag(e, e.currentTarget as HTMLElement, {
			onStart: () => setDraggingId(id),
			onMove: (x, y) => {
				const el = document.elementFromPoint(x, y);
				const tabEl = el?.closest("[data-tab-id]") as HTMLElement | null;
				if (tabEl && tabEl.dataset.tabId !== id) {
					const targetId = tabEl.dataset.tabId!;
					const rect = tabEl.getBoundingClientRect();
					setDragOverId(targetId);
					setDragOverSide(x < rect.left + rect.width / 2 ? "left" : "right");
					if (isGroupedMode()) {
						setDragInvalid(tabTypeOf(id) !== tabTypeOf(targetId));
					} else if (isFreeMode()) {
						setDragInvalid(false);
					} else {
						const srcType = tabTypeOf(id);
						const tgtType = tabTypeOf(targetId);
						setDragInvalid((srcType === "terminal") !== (tgtType === "terminal"));
					}
				} else {
					setDragOverId(null);
					setDragOverSide(null);
					setDragInvalid(false);
				}
			},
			onDrop: (x, y) => {
				const sourceId = id;
				// 1. Tab reorder (highest priority — dragOverId from onMove is reliable)
				const overId = dragOverId();
				const overSide = dragOverSide();
				if (overId && overId !== sourceId) {
					const termIds = activeTerminals();
					const fromIndex = termIds.indexOf(sourceId);
					const toIndex = termIds.indexOf(overId);
					if (fromIndex !== -1 && toIndex !== -1 && !isFreeMode()) {
						let adjustedTo = toIndex;
						if (overSide === "left" && fromIndex < toIndex) adjustedTo--;
						else if (overSide === "right" && fromIndex > toIndex) adjustedTo++;
						const clampedTo = Math.max(0, Math.min(adjustedTo, termIds.length - 1));
						if (fromIndex !== clampedTo) {
							props.onReorder?.(fromIndex, clampedTo);
						}
					} else if (isGroupedMode()) {
						if (visibleMdIds().includes(sourceId) && visibleMdIds().includes(overId)) {
							const side = overSide === "left" ? "before" : "after";
							mdTabsStore.reorderByIds(sourceId, overId, side);
						} else if (visibleDiffIds().includes(sourceId) && visibleDiffIds().includes(overId)) {
							const side = overSide === "left" ? "before" : "after";
							diffTabsStore.reorderByIds(sourceId, overId, side);
						} else if (visibleEditIds().includes(sourceId) && visibleEditIds().includes(overId)) {
							const side = overSide === "left" ? "before" : "after";
							editorTabsStore.reorderByIds(sourceId, overId, side);
						}
					} else {
						const side = overSide === "left" ? "before" : "after";
						tabOrderingStore.reorder(sourceId, overId, side);
						if (fromIndex !== -1 && toIndex !== -1) {
							let adjustedTo = toIndex;
							if (overSide === "left" && fromIndex < toIndex) adjustedTo--;
							else if (overSide === "right" && fromIndex > toIndex) adjustedTo++;
							const clampedTo = Math.max(0, Math.min(adjustedTo, termIds.length - 1));
							if (fromIndex !== clampedTo) {
								props.onReorder?.(fromIndex, clampedTo);
							}
						}
					}
					resetDragState();
					return;
				}
				// 2. Cross-pane move / assign orphan (split mode, dropped on pane area)
				if (paneLayoutStore.isSplit()) {
					const targetGroup = findPaneGroupAtPoint(x, y);
					if (targetGroup) {
						const fromGroup = paneLayoutStore.getGroupForTab(sourceId);
						if (fromGroup && fromGroup !== targetGroup) {
							paneLayoutStore.moveTab(fromGroup, targetGroup, sourceId);
							const srcGroup = paneLayoutStore.state.groups[fromGroup];
							if (srcGroup && srcGroup.tabs.length === 0) {
								paneLayoutStore.reset();
							} else {
								paneLayoutStore.setActiveGroup(targetGroup);
							}
							resetDragState();
							return;
						}
						if (!fromGroup) {
							const type = terminalsStore.get(sourceId)
								? ("terminal" as const)
								: diffTabsStore.get(sourceId)
									? ("diff" as const)
									: editorTabsStore.get(sourceId)
										? ("editor" as const)
										: ("markdown" as const);
							paneLayoutStore.addTab(targetGroup, { id: sourceId, type });
							paneLayoutStore.setActiveGroup(targetGroup);
							resetDragState();
							return;
						}
					}
				}
				// 3. Detach from split (dropped outside panes AND not on a tab)
				if (paneLayoutStore.isSplit()) {
					const fromGroup = paneLayoutStore.getGroupForTab(sourceId);
					if (fromGroup) {
						paneLayoutStore.removeTab(fromGroup, sourceId);
						const groups = Object.values(paneLayoutStore.state.groups);
						if (groups.some((g) => g.tabs.length === 0)) {
							paneLayoutStore.reset();
						}
						resetDragState();
						return;
					}
				}
				resetDragState();
			},
			onCancel: () => resetDragState(),
		});
	};

	const resetDragState = () => {
		setDraggingId(null);
		setDragOverId(null);
		setDragOverSide(null);
		setDragInvalid(false);
	};

	const commitRename = (id: string, input: HTMLInputElement) => {
		const newName = input.value.trim();
		if (newName) {
			terminalsStore.update(id, { name: newName, nameIsCustom: true });
		}
		setEditingId(null);
	};

	// Scroll state for arrows and fade gradients
	let tabsRef: HTMLDivElement | undefined;
	const [canScrollLeft, setCanScrollLeft] = createSignal(false);
	const [canScrollRight, setCanScrollRight] = createSignal(false);

	const updateScrollState = () => {
		const el = tabsRef;
		if (!el) return;
		const threshold = 8;
		const hasOverflow = el.scrollWidth > el.clientWidth + threshold;
		batch(() => {
			setCanScrollLeft(hasOverflow && el.scrollLeft > threshold);
			setCanScrollRight(hasOverflow && el.scrollLeft + el.clientWidth < el.scrollWidth - threshold);
		});
	};

	const scrollBy = (delta: number) => {
		tabsRef?.scrollBy({ left: delta, behavior: "smooth" });
	};

	onMount(() => {
		updateScrollState();
		const ro = new ResizeObserver(updateScrollState);
		if (tabsRef) ro.observe(tabsRef);
		onCleanup(() => ro.disconnect());
	});

	// Scroll active tab into view when selection changes
	createEffect(() => {
		const activeId = terminalsStore.state.activeId;
		const diffActive = diffTabsStore.state.activeId;
		const mdActive = mdTabsStore.state.activeId;
		const editActive = editorTabsStore.state.activeId;
		// Track any active id to trigger the effect
		void (activeId || diffActive || mdActive || editActive);
		requestAnimationFrame(() => {
			if (!tabsRef) return;
			const activeEl = tabsRef.querySelector(`.${s.active}`) as HTMLElement | null;
			activeEl?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
			updateScrollState();
		});
	});

	/** Collect tab IDs and names that are clipped in the given direction */
	const getOverflowItems = (direction: "left" | "right"): ContextMenuItem[] => {
		const el = tabsRef;
		if (!el) return [];
		const containerRect = el.getBoundingClientRect();
		const items: ContextMenuItem[] = [];

		const tabEls = el.querySelectorAll(`.${s.tab}`) as NodeListOf<HTMLElement>;
		for (const tabEl of tabEls) {
			const rect = tabEl.getBoundingClientRect();
			const clipped = direction === "left" ? rect.left < containerRect.left - 1 : rect.right > containerRect.right + 1;
			if (!clipped) continue;

			const nameEl = tabEl.querySelector(`.${s.tabName}`) as HTMLElement | null;
			const label = nameEl?.textContent ?? "Tab";

			// Determine which tab this DOM element represents by finding its click handler
			// We read the data from the element title or text content
			items.push({
				label,
				action: () => {
					tabEl.click();
					tabEl.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
				},
			});
		}

		return items;
	};

	const [overflowItems, setOverflowItems] = createSignal<ContextMenuItem[]>([]);

	const openOverflowMenu = (e: MouseEvent, direction: "left" | "right") => {
		e.preventDefault();
		e.stopPropagation();
		const items = getOverflowItems(direction);
		if (items.length === 0) return;
		setOverflowItems(items);
		const btn = e.currentTarget as HTMLElement;
		const rect = btn.getBoundingClientRect();
		overflowMenu.openAt(rect.left, rect.bottom + 4);
	};

	const chevronLeft = (
		<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
			<path
				fill-rule="evenodd"
				d="M11.354 1.646a.5.5 0 010 .708L5.707 8l5.647 5.646a.5.5 0 01-.708.708l-6-6a.5.5 0 010-.708l6-6a.5.5 0 01.708 0z"
			/>
		</svg>
	);
	const chevronRight = (
		<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
			<path
				fill-rule="evenodd"
				d="M4.646 1.646a.5.5 0 01.708 0l6 6a.5.5 0 010 .708l-6 6a.5.5 0 01-.708-.708L10.293 8 4.646 2.354a.5.5 0 010-.708z"
			/>
		</svg>
	);

	return (
		<div
			class={s.tabBarWrapper}
			data-drop-target="tab-bar"
			onDragOver={(e) => {
				if (e.dataTransfer?.types?.includes("application/x-tuic-path")) {
					e.preventDefault();
					e.dataTransfer.dropEffect = "copy";
				}
			}}
			onDrop={(e) => {
				const path = e.dataTransfer?.getData("application/x-tuic-path");
				if (!path) return;
				e.preventDefault();
				openPathsAsTabs([path]);
			}}
		>
			<div class={s.scrollRegion}>
				{/* Left scroll arrow */}
				<Button
					geometry="stretch"
					variant="ghost"
					class={cx(s.scrollArrow, s.scrollArrowLeft, canScrollLeft() && s.visible)}
					onClick={() => scrollBy(-200)}
					onContextMenu={(e) => openOverflowMenu(e, "left")}
					title="Scroll tabs left"
				>
					{chevronLeft}
				</Button>

				{/* Left fade gradient */}
				<div class={cx(s.fadeGradient, s.fadeLeft, canScrollLeft() && s.visible)} />

				<div class={s.tabs} ref={tabsRef} onScroll={updateScrollState}>
					{/* Terminal tabs (not rendered here in free mode — included in merged list) */}
					<Show when={!isFreeMode()}>
						<For each={activeTerminals()}>
							{(id, index) => (
								<TerminalTabView
									id={id}
									index={index()}
									paneRects={paneRects()}
									isDragging={draggingId() === id}
									isDragOver={dragOverId() === id && draggingId() !== id}
									dragOverSide={dragOverSide()}
									dragInvalid={dragInvalid()}
									isEditing={editingId() === id}
									tabHintsActive={Boolean(props.tabHintsActive)}
									quickSwitcherActive={Boolean(props.quickSwitcherActive)}
									showWorkspaceMetadata={true}
									designModeStatus={designModeForTab(id)?.status}
									onSelect={props.onTabSelect}
									onClose={props.onTabClose}
									onFocusDetached={props.onFocusDetachedTab}
									onContextMenu={openTabContextMenu}
									onPointerDown={handleMouseDrag}
									onStartEditing={(editingId) => setEditingId(editingId || null)}
									onCommitRename={commitRename}
								/>
							)}
						</For>
					</Show>

					{/* Non-terminal tabs: grouped mode renders 3 separate For blocks */}
					<Show when={isGroupedMode()}>
						<For each={visibleDiffIds()}>
							{(id) => (
								<DiffTabView
									id={id}
									paneRects={paneRects()}
									isDragging={draggingId() === id}
									isDragOver={dragOverId() === id && draggingId() !== id}
									dragOverSide={dragOverSide()}
									dragInvalid={dragInvalid()}
									onSelect={props.onTabSelect}
									onClose={props.onTabClose}
									onContextMenu={openTabContextMenu}
									onPointerDown={handleMouseDrag}
									showPinned={true}
									richIcon={true}
								/>
							)}
						</For>
						<For each={visibleMdIds()}>
							{(id) => (
								<MarkdownTabView
									id={id}
									paneRects={paneRects()}
									isDragging={draggingId() === id}
									isDragOver={dragOverId() === id && draggingId() !== id}
									dragOverSide={dragOverSide()}
									dragInvalid={dragInvalid()}
									onSelect={props.onTabSelect}
									onClose={props.onTabClose}
									onContextMenu={openTabContextMenu}
									onPointerDown={handleMouseDrag}
									showPinned={true}
									richIcon={true}
									onFocusDetached={props.onFocusDetachedTab}
								/>
							)}
						</For>
						<For each={visibleEditIds()}>
							{(id) => (
								<EditorTabView
									id={id}
									paneRects={paneRects()}
									isDragging={draggingId() === id}
									isDragOver={dragOverId() === id && draggingId() !== id}
									dragOverSide={dragOverSide()}
									dragInvalid={dragInvalid()}
									onSelect={props.onTabSelect}
									onClose={props.onTabClose}
									onContextMenu={openTabContextMenu}
									onPointerDown={handleMouseDrag}
									showPinned={true}
								/>
							)}
						</For>
					</Show>

					{/* Non-terminal tabs: merged mode (terminals-first / free) */}
					<Show when={!isGroupedMode()}>
						<For each={mergedNonTerminalIds()}>
							{(id) => {
								const type = () => tabTypeOf(id);
								return (
									<Switch>
										<Match when={type() === "terminal"}>
											<TerminalTabView
												id={id}
												index={activeTerminals().indexOf(id)}
												paneRects={paneRects()}
												isDragging={draggingId() === id}
												isDragOver={dragOverId() === id && draggingId() !== id}
												dragOverSide={dragOverSide()}
												dragInvalid={dragInvalid()}
												isEditing={editingId() === id}
												tabHintsActive={Boolean(props.tabHintsActive)}
												quickSwitcherActive={Boolean(props.quickSwitcherActive)}
												showWorkspaceMetadata={false}
												designModeStatus={designModeForTab(id)?.status}
												onSelect={props.onTabSelect}
												onClose={props.onTabClose}
												onFocusDetached={props.onFocusDetachedTab}
												onContextMenu={openTabContextMenu}
												onPointerDown={handleMouseDrag}
												onStartEditing={(editingId) => setEditingId(editingId || null)}
												onCommitRename={commitRename}
											/>
										</Match>
										<Match when={type() === "diff"}>
											<DiffTabView
												id={id}
												paneRects={paneRects()}
												isDragging={draggingId() === id}
												isDragOver={dragOverId() === id && draggingId() !== id}
												dragOverSide={dragOverSide()}
												dragInvalid={dragInvalid()}
												onSelect={props.onTabSelect}
												onClose={props.onTabClose}
												onContextMenu={openTabContextMenu}
												onPointerDown={handleMouseDrag}
												showPinned={false}
												richIcon={false}
											/>
										</Match>
										<Match when={type() === "markdown"}>
											<MarkdownTabView
												id={id}
												paneRects={paneRects()}
												isDragging={draggingId() === id}
												isDragOver={dragOverId() === id && draggingId() !== id}
												dragOverSide={dragOverSide()}
												dragInvalid={dragInvalid()}
												onSelect={props.onTabSelect}
												onClose={props.onTabClose}
												onContextMenu={openTabContextMenu}
												onPointerDown={handleMouseDrag}
												showPinned={false}
												richIcon={false}
												onFocusDetached={props.onFocusDetachedTab}
											/>
										</Match>
										<Match when={type() === "editor"}>
											<EditorTabView
												id={id}
												paneRects={paneRects()}
												isDragging={draggingId() === id}
												isDragOver={dragOverId() === id && draggingId() !== id}
												dragOverSide={dragOverSide()}
												dragInvalid={dragInvalid()}
												onSelect={props.onTabSelect}
												onClose={props.onTabClose}
												onContextMenu={openTabContextMenu}
												onPointerDown={handleMouseDrag}
												showPinned={false}
											/>
										</Match>
									</Switch>
								);
							}}
						</For>
					</Show>
				</div>
				{/* end .tabs */}

				{/* Right fade gradient */}
				<div class={cx(s.fadeGradient, s.fadeRight, canScrollRight() && s.visible)} />

				{/* Right scroll arrow */}
				<Button
					geometry="stretch"
					variant="ghost"
					class={cx(s.scrollArrow, s.scrollArrowRight, canScrollRight() && s.visible)}
					onClick={() => scrollBy(200)}
					onContextMenu={(e) => openOverflowMenu(e, "right")}
					title="Scroll tabs right"
				>
					{chevronRight}
				</Button>
			</div>
			{/* end .scrollRegion */}

			{/* New Tab button: outside scroll region so arrows don't overlap it */}
			<Button
				geometry="stretch"
				variant="ghost"
				class={s.newBtn}
				onClick={handleNewTabClick}
				{...newAgentMenu.buttonHandlers}
				title={`${t("tabBar.newTab", "New Tab")} (${keyFor("new-terminal")})`}
			>
				+
			</Button>

			<ContextMenu
				items={getTabContextMenuItems()}
				x={tabMenu.position().x}
				y={tabMenu.position().y}
				visible={tabMenu.visible()}
				onClose={tabMenu.close}
			/>
			<ContextMenu
				items={newAgentMenu.items()}
				x={newAgentMenu.position().x}
				y={newAgentMenu.position().y}
				visible={newAgentMenu.visible()}
				onClose={newAgentMenu.close}
			/>
			<ContextMenu
				items={overflowItems()}
				x={overflowMenu.position().x}
				y={overflowMenu.position().y}
				visible={overflowMenu.visible()}
				onClose={overflowMenu.close}
			/>
		</div>
	);
};

export default TabBar;
