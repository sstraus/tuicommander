import { type Component, createSignal, For, Show } from "solid-js";
import { t } from "../../i18n";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { mdTabsStore } from "../../stores/mdTabs";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { uiStore } from "../../stores/ui";
import { cx } from "../../utils";
import { keyFor } from "../../utils/hotkey";
import { markdownDocumentPanelId } from "../../utils/markdownDocumentPanelId";
import type { LeafRect } from "../../utils/paneTreeGeometry";
import { getRepoColor } from "../../utils/repoColor";
import { GlobeIcon } from "../GlobeIcon";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import s from "./TabBar.module.css";

type TabKind = "diff" | "editor" | "markdown" | "terminal";

interface SharedTabViewProps {
	id: string;
	paneRects: LeafRect[];
	isDragging: boolean;
	isDragOver: boolean;
	dragOverSide: "left" | "right" | null;
	dragInvalid: boolean;
	onSelect: (id: string) => void;
	onClose: (id: string, skipConfirm?: boolean) => void;
	onContextMenu: (event: MouseEvent, id: string) => void;
	onPointerDown: (event: PointerEvent, id: string, kind?: TabKind) => void;
}

const PanePositionIcon: Component<{ tabId: string; rects: LeafRect[] }> = (props) => {
	const groupId = () => paneLayoutStore.getGroupForTab(props.tabId);
	const width = 14;
	const height = 10;
	const padding = 0.5;
	const gap = 0.8;

	return (
		<Show when={paneLayoutStore.isSplit() && groupId()}>
			<svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} class={s.panePositionIcon} fill="none">
				<For each={props.rects}>
					{(rect) => {
						const active = () => rect.groupId === groupId();
						return (
							<rect
								x={rect.x * (width - gap) + padding}
								y={rect.y * (height - gap) + padding}
								width={rect.w * (width - gap) - gap}
								height={rect.h * (height - gap) - gap}
								rx="1"
								fill={active() ? "currentColor" : "none"}
								stroke="currentColor"
								stroke-width="0.7"
								opacity={active() ? 0.9 : 0.35}
							/>
						);
					}}
				</For>
			</svg>
		</Show>
	);
};

const PinIcon: Component = () => (
	<span class={s.pinIcon}>
		<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
			<circle cx="8" cy="4" r="3.5" />
			<path d="M6.2 7l-1.7 4.5a.5.5 0 0 0 .13.54l2.84 2.84a.75.75 0 0 0 1.06 0l2.84-2.84a.5.5 0 0 0 .13-.54L9.8 7H6.2z" />
		</svg>
	</span>
);

function dragClasses(props: SharedTabViewProps) {
	return [
		props.isDragging && s.dragging,
		props.isDragOver && props.dragOverSide === "left" && s.dragOverLeft,
		props.isDragOver && props.dragOverSide === "right" && s.dragOverRight,
		props.isDragOver && props.dragInvalid && s.dragInvalid,
	];
}

const AWAITING_CLASSES: Record<string, string> = {
	question: s.awaitingQuestion,
	error: s.awaitingError,
};

interface TerminalTabViewProps extends SharedTabViewProps {
	index: number;
	designModeStatus?: "armed" | "stopped";
	quickSwitcherActive: boolean;
	tabHintsActive?: boolean;
	isEditing: boolean;
	showWorkspaceMetadata: boolean;
	onFocusDetached?: (id: string) => void;
	onStartEditing: (id: string) => void;
	onCommitRename: (id: string, input: HTMLInputElement) => void;
}

export const TerminalTabView: Component<TerminalTabViewProps> = (props) => {
	const terminal = () => terminalsStore.get(props.id);
	const isActive = () => terminalsStore.state.activeId === props.id;
	const isDetached = () => terminalsStore.isDetached(props.id);
	const isBusy = () => terminalsStore.isBusy(props.id);
	const isIdle = () => !isBusy() && terminal()?.shellState === "idle";
	const isExited = () => terminal()?.shellState === "exited";
	const isUnseen = () => !isActive() && terminal()?.unseen;
	const awaitingInput = () => terminal()?.awaitingInput;
	const progress = () => terminal()?.progress;
	const isPromoted = () => globalWorkspaceStore.isPromoted(props.id);
	const [hovered, setHovered] = createSignal(false);
	const repoName = () => repositoriesStore.getRepoForTerminal(props.id);
	const repoColor = () => {
		if (!props.showWorkspaceMetadata || !globalWorkspaceStore.isActive()) return undefined;
		const path = repositoriesStore.getRepoPathForTerminal(props.id);
		return path ? getRepoColor(path) : undefined;
	};

	const select = () => {
		if (isDetached()) props.onFocusDetached?.(props.id);
		else props.onSelect(props.id);
	};

	return (
		<Show when={terminal()}>
			<div
				class={cx(
					s.tab,
					isActive() && !isDetached() && s.active,
					isDetached() && s.detached,
					awaitingInput() && s.awaitingInput,
					awaitingInput() && AWAITING_CLASSES[awaitingInput()!],
					!awaitingInput() && !terminal()?.providerRecovery && isBusy() && s.shellBusy,
					!awaitingInput() && !isBusy() && isUnseen() && s.shellUnseen,
					!awaitingInput() && isIdle() && !isUnseen() && s.shellIdle,
					isExited() && s.shellExited,
					terminal()?.isExternallyCreated && s.remoteTab,
					terminal()?.standby && s.standby,
					terminal()?.suspended && s.suspended,
					...dragClasses(props),
				)}
				data-tab-id={props.id}
				style={repoColor() ? ({ "--repo-color": repoColor() } as Record<string, string>) : undefined}
				onClick={select}
				onAuxClick={(event) => {
					if (event.button === 1) {
						event.preventDefault();
						event.stopPropagation();
						props.onClose(props.id, true);
					}
				}}
				onContextMenu={(event) => props.onContextMenu(event, props.id)}
				title={`${terminal()?.alias ?? `Terminal ${props.index + 1}`}${props.index < 9 ? ` (${keyFor(`switch-tab-${props.index + 1}`)})` : ""}`}
				onPointerDown={(event) => !props.isEditing && props.onPointerDown(event, props.id)}
				onMouseEnter={() => setHovered(true)}
				onMouseLeave={() => setHovered(false)}
				onDblClick={(event) => {
					event.stopPropagation();
					props.onStartEditing(props.id);
				}}
			>
				<span class={s.tabIcon}>●</span>
				<Show when={props.designModeStatus === "armed"}>
					<span
						class={cx(s.designModeBadge, s.designModeArmed)}
						title={t("tabBar.designModeArmed", "Design Mode armed")}
					>
						D
					</span>
				</Show>
				<Show
					when={props.isEditing}
					fallback={
						<span class={s.tabName}>
							<Show when={terminal()?.isExternallyCreated && terminal()?.agentType}>
								<span class={s.ptyPrefix}>PTY · </span>
							</Show>
							{terminal()?.name}
							<Show when={terminal()?.providerRecovery}>
								<span> · recovering</span>
							</Show>
							<Show when={terminal()?.standby}>
								<span class={s.standbyBadge} title="Standby (paused)">
									<svg viewBox="0 0 8 10" width="8" height="10" fill="currentColor">
										<rect x="0" y="0" width="3" height="10" />
										<rect x="5" y="0" width="3" height="10" />
									</svg>
								</span>
							</Show>
							<Show when={terminal()?.suspended}>
								<span class={s.suspendedBadge} title={t("tabBar.suspended", "Suspended")}>
									zz
								</span>
							</Show>
							<Show when={isDetached()}>
								<svg
									class={s.detachedIcon}
									viewBox="0 0 12 12"
									width="10"
									height="10"
									fill="none"
									stroke="currentColor"
									stroke-width="1.5"
								>
									<path d="M7 1h4v4M11 1L6 6M5 2H2v8h8V7" />
								</svg>
							</Show>
						</span>
					}
				>
					<Input
						compact
						class={s.tabNameInput}
						type="text"
						value={terminal()?.name || ""}
						ref={(element) =>
							requestAnimationFrame(() => {
								element.focus();
								element.select();
							})
						}
						onClick={(event) => event.stopPropagation()}
						onBlur={(event) => props.onCommitRename(props.id, event.currentTarget)}
						onKeyDown={(event) => {
							if (event.key === "Enter") props.onCommitRename(props.id, event.currentTarget);
							else if (event.key === "Escape") props.onStartEditing("");
						}}
					/>
				</Show>
				{progress() !== null && progress() !== undefined && (
					<div class={s.progress} style={{ transform: `scaleX(${progress()! / 100})` }} />
				)}
				<PanePositionIcon tabId={props.id} rects={props.paneRects} />
				<Show when={props.showWorkspaceMetadata && isPromoted() && !globalWorkspaceStore.isActive()}>
					<Button
						size="compact"
						variant="ghost"
						class={s.globeIcon}
						title={t("tabBar.removeFromWorkspace", "Remove from Global Workspace")}
						onClick={(event) => {
							event.stopPropagation();
							globalWorkspaceStore.unpromote(props.id);
						}}
					>
						<GlobeIcon size={11} />
					</Button>
				</Show>
				<Show when={(props.tabHintsActive || props.quickSwitcherActive) && props.index < 9}>
					<span class={s.shortcutBadge}>
						{keyFor(`${props.tabHintsActive ? "switch-tab" : "switch-branch"}-${props.index + 1}`)}
					</span>
				</Show>
				<Show when={props.showWorkspaceMetadata && hovered() && globalWorkspaceStore.isActive() && repoName()}>
					<span class={s.repoOverlay}>{repoName()}</span>
				</Show>
				<Button
					size="compact"
					variant="ghost"
					class={s.tabClose}
					title={t("tabBar.close", "Close")}
					onClick={(event) => {
						event.preventDefault();
						event.stopPropagation();
						props.onClose(props.id);
					}}
				>
					×
				</Button>
			</div>
		</Show>
	);
};

interface FileTabViewProps extends SharedTabViewProps {
	showPinned: boolean;
	richIcon: boolean;
	onFocusDetached?: (id: string) => void;
}

export const DiffTabView: Component<FileTabViewProps> = (props) => {
	const tab = () => diffTabsStore.get(props.id);
	return (
		<Show when={tab()}>
			<div
				class={cx(s.tab, s.diffTab, diffTabsStore.state.activeId === props.id && s.active, ...dragClasses(props))}
				data-tab-id={props.id}
				onClick={() => {
					diffTabsStore.setActive(props.id);
					props.onSelect(props.id);
				}}
				onAuxClick={(event) => {
					if (event.button === 1) {
						event.preventDefault();
						diffTabsStore.remove(props.id);
						props.onClose(props.id, true);
					}
				}}
				onContextMenu={(event) => props.onContextMenu(event, props.id)}
				title={tab()?.filePath}
				onPointerDown={(event) => props.onPointerDown(event, props.id, "diff")}
			>
				<span class={s.tabIcon}>
					{props.richIcon && !tab()?.filePath ? (
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path d="M2 2h12v1H2zm0 3h12v1H2zm0 3h10v1H2zm0 3h8v1H2z" />
						</svg>
					) : (
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path
								fill-rule="evenodd"
								d="M3 1a1 1 0 00-1 1v12a1 1 0 001 1h10a1 1 0 001-1V5.5L9.5 1H3zm6.5 1.5v2.5H12L9.5 2.5zM8 6a.5.5 0 01.5.5v1h1a.5.5 0 010 1h-1v1a.5.5 0 01-1 0v-1h-1a.5.5 0 010-1h1v-1A.5.5 0 018 6zm-3 5a.5.5 0 000 1h5a.5.5 0 000-1H5z"
							/>
						</svg>
					)}
				</span>
				<Show when={props.showPinned && tab()?.pinned}>
					<PinIcon />
				</Show>
				<span class={s.tabName}>
					{tab()?.fileName}
					{tab()?.scope ? ` (${tab()?.scope?.slice(0, 7)})` : ""}
				</span>
				<PanePositionIcon tabId={props.id} rects={props.paneRects} />
				<Button
					size="compact"
					variant="ghost"
					class={s.tabClose}
					title={t("tabBar.close", "Close")}
					onClick={(event) => {
						event.stopPropagation();
						diffTabsStore.remove(props.id);
						props.onClose(props.id);
					}}
				>
					×
				</Button>
			</div>
		</Show>
	);
};

export const MarkdownTabView: Component<FileTabViewProps> = (props) => {
	const tab = () => mdTabsStore.get(props.id);
	const isDetached = () => uiStore.isDetached(markdownDocumentPanelId(props.id));
	const title = () => {
		const current = tab();
		return current?.type === "file"
			? current.filePath
			: current?.type === "pr-diff"
				? `PR #${current.prNumber}: ${current.prTitle}`
				: current?.title;
	};
	const label = () => {
		const current = tab();
		return current?.type === "file" ? current.fileName : current?.title;
	};
	return (
		<Show when={tab()}>
			<div
				class={cx(
					s.tab,
					tab()?.type === "file" ? s.mdTab : tab()?.type === "pr-diff" ? s.diffTab : s.panelTab,
					mdTabsStore.state.activeId === props.id && !isDetached() && s.active,
					isDetached() && s.detached,
					...dragClasses(props),
				)}
				data-tab-id={props.id}
				onClick={() => {
					if (isDetached()) {
						props.onFocusDetached?.(props.id);
						return;
					}
					mdTabsStore.setActive(props.id);
					props.onSelect(props.id);
				}}
				onAuxClick={(event) => {
					if (event.button === 1) {
						event.preventDefault();
						mdTabsStore.remove(props.id);
						props.onClose(props.id, true);
					}
				}}
				onContextMenu={(event) => props.onContextMenu(event, props.id)}
				title={title()}
				onPointerDown={(event) => props.onPointerDown(event, props.id, "markdown")}
			>
				<span class={s.tabIcon}>
					{props.richIcon && tab()?.type === "pr-diff" ? (
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path
								fill-rule="evenodd"
								d="M7.177 3.073L9.573.677A.25.25 0 0110 .854v4.792a.25.25 0 01-.427.177L7.177 3.427a.25.25 0 010-.354zM3.75 2.5a.75.75 0 100 1.5.75.75 0 000-1.5zm-2.25.75a2.25 2.25 0 113 2.122v5.256a2.251 2.251 0 11-1.5 0V5.372A2.25 2.25 0 011.5 3.25zM11 2.5h-1V4h1a1 1 0 011 1v5.628a2.251 2.251 0 101.5 0V5A2.5 2.5 0 0011 2.5zm1 10.25a.75.75 0 111.5 0 .75.75 0 01-1.5 0zM3.75 12a.75.75 0 100 1.5.75.75 0 000-1.5z"
							/>
						</svg>
					) : (
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path
								fill-rule="evenodd"
								d="M3 1a1 1 0 00-1 1v12a1 1 0 001 1h10a1 1 0 001-1V5.5L9.5 1H3zm6.5 1.5v2.5H12L9.5 2.5zM4.5 7.5h7a.5.5 0 010 1h-7a.5.5 0 010-1zm0 2.5h7a.5.5 0 010 1h-7a.5.5 0 010-1zm0 2.5h4a.5.5 0 010 1h-4a.5.5 0 010-1z"
							/>
						</svg>
					)}
				</span>
				<Show when={props.showPinned && tab()?.pinned}>
					<PinIcon />
				</Show>
				<span class={s.tabName}>{label()}</span>
				<PanePositionIcon tabId={props.id} rects={props.paneRects} />
				<Button
					size="compact"
					variant="ghost"
					class={s.tabClose}
					title={t("tabBar.close", "Close")}
					onClick={(event) => {
						event.stopPropagation();
						mdTabsStore.remove(props.id);
						props.onClose(props.id);
					}}
				>
					×
				</Button>
			</div>
		</Show>
	);
};

export const EditorTabView: Component<Omit<FileTabViewProps, "richIcon">> = (props) => {
	const tab = () => editorTabsStore.get(props.id);
	return (
		<Show when={tab()}>
			<div
				class={cx(s.tab, s.editTab, editorTabsStore.state.activeId === props.id && s.active, ...dragClasses(props))}
				data-tab-id={props.id}
				onClick={() => {
					editorTabsStore.setActive(props.id);
					props.onSelect(props.id);
				}}
				onAuxClick={(event) => {
					if (event.button === 1) {
						event.preventDefault();
						props.onClose(props.id, true);
					}
				}}
				onContextMenu={(event) => props.onContextMenu(event, props.id)}
				title={tab()?.filePath}
				onPointerDown={(event) => props.onPointerDown(event, props.id, "editor")}
			>
				<span class={s.tabIcon}>
					{tab()?.isDirty ? (
						<svg width="8" height="8" viewBox="0 0 8 8" fill="currentColor">
							<circle cx="4" cy="4" r="4" />
						</svg>
					) : (
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path d="M11.13 1.47a1.5 1.5 0 012.12 0l1.28 1.28a1.5 1.5 0 010 2.12L5.9 13.5a1 1 0 01-.5.27l-3.5.87a.5.5 0 01-.6-.6l.87-3.5a1 1 0 01.27-.5L11.13 1.47zm1.07 1.06L3.74 11l-.58 2.34 2.34-.58 8.47-8.46-1.77-1.77z" />
						</svg>
					)}
				</span>
				<Show when={props.showPinned && tab()?.pinned}>
					<PinIcon />
				</Show>
				<span class={s.tabName}>{tab()?.fileName}</span>
				<PanePositionIcon tabId={props.id} rects={props.paneRects} />
				<Button
					size="compact"
					variant="ghost"
					class={s.tabClose}
					title={t("tabBar.close", "Close")}
					onClick={(event) => {
						event.stopPropagation();
						props.onClose(props.id);
					}}
				>
					×
				</Button>
			</div>
		</Show>
	);
};
