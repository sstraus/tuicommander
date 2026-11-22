import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({
	invoke: vi.fn(() => Promise.resolve(null)),
}));

import {
	applyEdgeDrop,
	dropZoneAt,
	hideEdgeDropPreview,
	type Rect,
	showEdgeDropPreview,
	zonePreviewRect,
} from "../../components/PaneTree/paneEdgeDrop";
import { _testCancelPendingTimers } from "../../hooks/useSplitPanes";
import { allLeafIds, type PaneBranch, paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { terminalsStore } from "../../stores/terminals";
import { testInScope } from "../helpers/store";

const tabsOf = (groupId: string) => paneLayoutStore.state.groups[groupId]?.tabs.map((t) => t.id);
const leafTabs = () => allLeafIds(paneLayoutStore.getRoot()!).map(tabsOf);
const addTerminal = (name: string) =>
	terminalsStore.add({ sessionId: null, fontSize: 14, name, cwd: null, awaitingInput: null });

beforeEach(() => {
	paneLayoutStore.reset();
	resetGroupCounter();
});
afterEach(() => {
	paneLayoutStore._testCancelPendingSave();
	_testCancelPendingTimers();
});

describe("dropZoneAt", () => {
	const rect = { left: 100, top: 50, width: 400, height: 200 };

	// catches: axis mix-up (x fraction compared against top/bottom), or zones measured
	// from the viewport instead of from the pane's own origin.
	it("maps each edge band to its side and the middle to the center", () => {
		expect(dropZoneAt(rect, 120, 150)).toBe("left");
		expect(dropZoneAt(rect, 480, 150)).toBe("right");
		expect(dropZoneAt(rect, 300, 60)).toBe("top");
		expect(dropZoneAt(rect, 300, 240)).toBe("bottom");
		expect(dropZoneAt(rect, 300, 150)).toBe("center");
	});
});

/** Same shape as the browser's DOMRect: geometry lives in prototype getters,
 *  not in own properties, so an object spread copies none of it. */
class GetterRect implements Rect {
	readonly #r: Rect;
	constructor(left: number, top: number, width: number, height: number) {
		this.#r = { left, top, width, height };
	}
	get left() {
		return this.#r.left;
	}
	get top() {
		return this.#r.top;
	}
	get width() {
		return this.#r.width;
	}
	get height() {
		return this.#r.height;
	}
}

describe("zonePreviewRect", () => {
	// Terminal area to the right of a 125px sidebar, below a 40px tab bar.
	const pane = () => new GetterRect(125, 40, 640, 460);

	// catches: the preview copied with `{ ...rect }`, which drops every getter of a
	// real DOMRect, so the left zone started at the window edge (over the sidebar).
	it.each([
		["left", { left: 125, top: 40, width: 320, height: 460 }],
		["right", { left: 445, top: 40, width: 320, height: 460 }],
		["top", { left: 125, top: 40, width: 640, height: 230 }],
		["bottom", { left: 125, top: 270, width: 640, height: 230 }],
		["center", { left: 125, top: 40, width: 640, height: 460 }],
	] as const)("covers the %s part of a DOMRect-shaped pane", (zone, expected) => {
		expect({ ...zonePreviewRect(pane(), zone) }).toEqual(expected);
	});
});

describe("showEdgeDropPreview", () => {
	afterEach(() => hideEdgeDropPreview());
	const preview = () => document.querySelector<HTMLElement>(".pane-edge-drop-preview");
	const box = () => {
		const s = preview()!.style;
		return { left: s.left, top: s.top, width: s.width, height: s.height };
	};

	// catches: a zone change that leaves the previous zone's left/top/height on the
	// reused preview element (the stale overlay seen when moving between edges).
	it("redraws the full geometry when the drag moves from one edge to another", () => {
		const rect = new GetterRect(125, 40, 640, 460);
		showEdgeDropPreview({ groupId: null, zone: "bottom", rect });
		expect(box()).toEqual({ left: "125px", top: "270px", width: "640px", height: "230px" });
		showEdgeDropPreview({ groupId: null, zone: "left", rect });
		expect(box()).toEqual({ left: "125px", top: "40px", width: "320px", height: "460px" });
		showEdgeDropPreview({ groupId: "g1", zone: "right", rect: new GetterRect(400, 300, 200, 100) });
		expect(box()).toEqual({ left: "500px", top: "300px", width: "100px", height: "100px" });
	});

	// catches: a null target that leaves the last overlay on screen after the
	// pointer leaves every pane.
	it("removes the overlay when there is no target", () => {
		showEdgeDropPreview({ groupId: null, zone: "top", rect: new GetterRect(0, 0, 100, 100) });
		expect(preview()).not.toBeNull();
		showEdgeDropPreview(null);
		expect(preview()).toBeNull();
	});
});

describe("paneLayoutStore.splitAtEdge", () => {
	// catches: the new pane always appended after the target, so a left or top
	// drop lands on the right or bottom.
	it("puts the tab before the target pane for a left drop", () => {
		testInScope(() => {
			const g1 = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(g1, { id: "a", type: "terminal" });
			paneLayoutStore.addTab(g1, { id: "b", type: "terminal" });
			paneLayoutStore.setRoot({ type: "leaf", id: g1 });

			expect(paneLayoutStore.splitAtEdge(g1, "left", { id: "b", type: "terminal" })).not.toBeNull();

			expect((paneLayoutStore.getRoot() as PaneBranch).direction).toBe("vertical");
			expect(leafTabs()).toEqual([["b"], ["a"]]);
		});
	});

	// catches: the source pane left empty after its only tab moves out, or the whole
	// layout reset as the cross-pane move does.
	it("closes the pane that the moved tab leaves empty", () => {
		testInScope(() => {
			const g1 = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(g1, { id: "a", type: "terminal" });
			paneLayoutStore.setRoot({ type: "leaf", id: g1 });
			const g2 = paneLayoutStore.split(g1, "vertical")!;
			paneLayoutStore.addTab(g2, { id: "b", type: "terminal" });

			paneLayoutStore.splitAtEdge(g1, "bottom", { id: "b", type: "terminal" });

			const root = paneLayoutStore.getRoot() as PaneBranch;
			expect(root.direction).toBe("horizontal");
			expect(leafTabs()).toEqual([["a"], ["b"]]);
		});
	});

	// catches: a pane split against its own only tab, which leaves one empty pane.
	it("refuses to split a pane around its only tab", () => {
		testInScope(() => {
			const g1 = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(g1, { id: "a", type: "terminal" });
			paneLayoutStore.setRoot({ type: "leaf", id: g1 });

			expect(paneLayoutStore.splitAtEdge(g1, "right", { id: "a", type: "terminal" })).toBeNull();
			expect(paneLayoutStore.getRoot()).toEqual({ type: "leaf", id: g1 });
			expect(Object.keys(paneLayoutStore.state.groups)).toEqual([g1]);
		});
	});
});

describe("applyEdgeDrop without a split", () => {
	// catches: the first split drops the tab that was on screen, or leaves the dragged
	// tab inactive so the active-orphan overlay hides the new split.
	it("splits around the displayed tab and shows the dropped tab", () => {
		testInScope(() => {
			const shown = addTerminal("shown");
			const dragged = addTerminal("dragged");
			terminalsStore.setActive(shown);

			const handled = applyEdgeDrop(
				{ groupId: null, zone: "right", rect: { left: 0, top: 0, width: 800, height: 600 } },
				{ id: dragged, type: "terminal" },
			);

			expect(handled).toBe(true);
			expect(paneLayoutStore.isSplit()).toBe(true);
			expect(leafTabs()).toEqual([[shown], [dragged]]);
			expect(terminalsStore.state.activeId).toBe(dragged);
			terminalsStore.remove(shown);
			terminalsStore.remove(dragged);
		});
	});

	// catches: dragging the tab that is on screen is ignored, so the natural gesture
	// (drag the visible tab to an edge) never splits.
	it("splits around the companion when the dragged tab is the one on screen", () => {
		testInScope(() => {
			const other = addTerminal("other");
			const dragged = addTerminal("dragged");
			terminalsStore.setActive(dragged);
			const rect = { left: 0, top: 0, width: 800, height: 600 };
			const tab = { id: dragged, type: "terminal" as const };
			const companion = { id: other, type: "terminal" as const };

			expect(applyEdgeDrop({ groupId: null, zone: "right", rect }, tab)).toBe(false);
			const handled = applyEdgeDrop({ groupId: null, zone: "left", rect }, tab, companion);

			expect(handled).toBe(true);
			expect(leafTabs()).toEqual([[dragged], [other]]);
			expect(terminalsStore.state.activeId).toBe(dragged);
			terminalsStore.remove(other);
			terminalsStore.remove(dragged);
		});
	});
});
