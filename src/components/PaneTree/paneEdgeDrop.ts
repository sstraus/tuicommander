/**
 * Drag a tab onto the edge of a pane to split that pane.
 *
 * Every pane has five drop zones: four edges and a center. An edge splits the
 * pane 50/50 and puts the dragged tab on that side. The center keeps the
 * existing behavior (move the tab into the pane), which the callers handle.
 * Without a split, the whole terminal area is one pane: an edge drop creates
 * the first split around the tab that is on screen.
 */
import { refitPaneTerminals } from "../../hooks/useSplitPanes";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { type PaneEdge, type PaneTab, type PaneTabType, paneLayoutStore } from "../../stores/paneLayout";
import { terminalsStore } from "../../stores/terminals";

export type PaneDropZone = PaneEdge | "center";

export interface Rect {
	left: number;
	top: number;
	width: number;
	height: number;
}

/** Fraction of a pane's width or height that belongs to each edge zone. */
export const EDGE_ZONE_FRACTION = 0.3;

/** The zone of `rect` under the point: the nearest edge when the point is
 *  within EDGE_ZONE_FRACTION of it, otherwise the center. */
export function dropZoneAt(rect: Rect, x: number, y: number): PaneDropZone {
	if (rect.width <= 0 || rect.height <= 0) return "center";
	const fx = (x - rect.left) / rect.width;
	const fy = (y - rect.top) / rect.height;
	const distances: Array<[PaneEdge, number]> = [
		["left", fx],
		["right", 1 - fx],
		["top", fy],
		["bottom", 1 - fy],
	];
	let nearest = distances[0];
	for (const d of distances) if (d[1] < nearest[1]) nearest = d;
	return nearest[1] < EDGE_ZONE_FRACTION ? nearest[0] : "center";
}

/** The part of `rect` that the dropped tab will occupy. */
export function zonePreviewRect(rect: Rect, zone: PaneDropZone): Rect {
	const halfW = rect.width / 2;
	const halfH = rect.height / 2;
	switch (zone) {
		case "left":
			return { ...rect, width: halfW };
		case "right":
			return { ...rect, left: rect.left + halfW, width: halfW };
		case "top":
			return { ...rect, height: halfH };
		case "bottom":
			return { ...rect, top: rect.top + halfH, height: halfH };
		default:
			return rect;
	}
}

/** Tab type of an open tab, from the store that owns it. */
export function paneTabTypeOf(tabId: string): PaneTabType | null {
	if (terminalsStore.get(tabId)) return "terminal";
	if (diffTabsStore.get(tabId)) return "diff";
	if (editorTabsStore.get(tabId)) return "editor";
	if (mdTabsStore.get(tabId)) return "markdown";
	return null;
}

/** The tab on screen when there is no split. Activation is exclusive across
 *  the tab stores, so at most one of them has an active ID. */
function displayedTab(): PaneTab | null {
	const candidates: Array<[string | null, PaneTabType]> = [
		[terminalsStore.state.activeId, "terminal"],
		[diffTabsStore.state.activeId, "diff"],
		[mdTabsStore.state.activeId, "markdown"],
		[editorTabsStore.state.activeId, "editor"],
	];
	for (const [id, type] of candidates) if (id) return { id, type };
	return null;
}

function activateTab(tab: PaneTab): void {
	if (tab.type === "terminal") terminalsStore.setActive(tab.id);
	else if (tab.type === "diff") diffTabsStore.setActive(tab.id);
	else if (tab.type === "editor") editorTabsStore.setActive(tab.id);
	else mdTabsStore.setActive(tab.id);
}

export interface EdgeDropTarget {
	/** Target pane group, or null for the unsplit terminal area. */
	groupId: string | null;
	zone: PaneDropZone;
	rect: Rect;
}

/** The tab that stays in the first pane of an unsplit area. Normally the tab on
 *  screen; when the dragged tab is the one on screen, `companion` (a neighbour in
 *  the tab strip) takes its place. Null when no other tab exists. */
function unsplitBase(tabId: string, companion: PaneTab | null): PaneTab | null {
	const shown = displayedTab();
	if (shown && shown.id !== tabId) return shown;
	return companion && companion.id !== tabId ? companion : null;
}

/** The pane and zone under the point for a drag of `tabId`, or null when the
 *  point is outside every pane or the drop would change nothing. An edge that cannot split (depth limit, or the
 *  tab would leave its own pane empty) is reported as the center. */
export function edgeDropTargetAt(
	x: number,
	y: number,
	tabId: string,
	companion: PaneTab | null = null,
): EdgeDropTarget | null {
	const el = document.elementFromPoint(x, y);
	if (!el) return null;
	if (paneLayoutStore.isSplit()) {
		const pane = el.closest<HTMLElement>('[data-drop-target="pane"][data-group-id]');
		const groupId = pane?.dataset.groupId;
		if (!pane || !groupId) return null;
		const rect = pane.getBoundingClientRect();
		let zone = dropZoneAt(rect, x, y);
		if (zone !== "center" && !paneLayoutStore.canSplitAtEdge(groupId, tabId)) zone = "center";
		// A center drop into the pane that already holds the tab changes nothing.
		if (zone === "center" && paneLayoutStore.getGroupForTab(tabId) === groupId) return null;
		return { groupId, zone, rect };
	}
	const area = el.closest<HTMLElement>("#terminal-panes");
	if (!area || !unsplitBase(tabId, companion)) return null;
	const rect = area.getBoundingClientRect();
	const zone = dropZoneAt(rect, x, y);
	// Without a split, the center has no drop action.
	return zone === "center" ? null : { groupId: null, zone, rect };
}

/** Split for an edge target. Returns true when the drop was handled here;
 *  a center target is left to the caller. */
export function applyEdgeDrop(target: EdgeDropTarget | null, tab: PaneTab, companion: PaneTab | null = null): boolean {
	if (!target || target.zone === "center") return false;
	let groupId = target.groupId;
	if (groupId === null) {
		const base = unsplitBase(tab.id, companion);
		if (!base) return false;
		groupId = paneLayoutStore.createGroup();
		paneLayoutStore.addTab(groupId, base);
		paneLayoutStore.setRoot({ type: "leaf", id: groupId });
	}
	if (!paneLayoutStore.splitAtEdge(groupId, target.zone, tab)) {
		if (target.groupId === null) paneLayoutStore.reset();
		return false;
	}
	activateTab(tab);
	refitPaneTerminals();
	return true;
}

let previewEl: HTMLDivElement | null = null;

/** Show where the dragged tab will land. Hidden when `target` is null. */
export function showEdgeDropPreview(target: EdgeDropTarget | null): void {
	if (!target) {
		hideEdgeDropPreview();
		return;
	}
	if (!previewEl) {
		previewEl = document.createElement("div");
		previewEl.className = "pane-edge-drop-preview";
		document.body.appendChild(previewEl);
	}
	const r = zonePreviewRect(target.rect, target.zone);
	Object.assign(previewEl.style, {
		left: `${r.left}px`,
		top: `${r.top}px`,
		width: `${r.width}px`,
		height: `${r.height}px`,
	});
}

export function hideEdgeDropPreview(): void {
	previewEl?.remove();
	previewEl = null;
}
