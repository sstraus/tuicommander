import { createComputed, createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneLayoutState } from "../../stores/paneLayout";

const io = vi.hoisted(() => ({ invoke: vi.fn(), subscribe: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: io.invoke }));
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: io.subscribe,
}));

const empty = { root: null, groups: {}, activeGroupId: null };
const single: PaneLayoutState = {
	root: { type: "leaf", id: "g1" },
	groups: { g1: { id: "g1", tabs: [{ id: "term-1", type: "terminal" }], activeTabId: "term-1" } },
	activeGroupId: "g1",
};
const split: PaneLayoutState = {
	root: {
		type: "branch",
		direction: "vertical",
		children: [
			{ type: "leaf", id: "g2" },
			{ type: "leaf", id: "g3" },
		],
		ratios: [0.5, 0.5],
	},
	groups: {
		g2: { id: "g2", tabs: [{ id: "term-2", type: "terminal" }], activeTabId: "term-2" },
		g3: { id: "g3", tabs: [{ id: "term-3", type: "terminal" }], activeTabId: "term-3" },
	},
	activeGroupId: "g3",
};
const flush = async () => {
	for (let i = 0; i < 16; i++) await Promise.resolve();
};

describe("pane layout settings refresh", () => {
	let document: Partial<PaneLayoutState> | null;
	let handlers: Array<Record<string, (payload: unknown) => void>>;
	let cleanups: Array<() => void>;
	let warnings: unknown[][];
	let generation: number;

	beforeEach(() => {
		vi.resetModules();
		document = null;
		handlers = [];
		cleanups = [];
		warnings = [];
		generation = 0;
		io.invoke.mockReset().mockImplementation(async (command: string) => {
			if (command === "load_pane_layout") return structuredClone(document);
		});
		io.subscribe.mockReset().mockImplementation(async (callbacks) => {
			handlers.push(callbacks);
			return vi.fn();
		});
	});
	afterEach(() => {
		for (const cleanup of cleanups.reverse()) cleanup();
		vi.restoreAllMocks();
	});

	async function windowStore() {
		const store = (await import("../../stores/paneLayout")).paneLayoutStore;
		const { orderTerminalsByPaneLayout } = await import("../../utils/paneTabOrder");
		const { appLogger } = await import("../../stores/appLogger");
		vi.spyOn(appLogger, "warn").mockImplementation((...args) => {
			warnings.push(args);
		});
		vi.spyOn(appLogger, "info").mockImplementation(() => {});
		await store.loadFromDisk();
		cleanups.push(() => store._testCancelPendingSave());
		cleanups.push(store.startRemoteSync());
		await flush();
		return { store, orderTerminalsByPaneLayout };
	}

	async function refresh(next: Partial<PaneLayoutState> | null) {
		document = next;
		generation++;
		for (const window of handlers) {
			window["settings-changed"]({ section: "pane_layout", generation, persisted: true, applied: true });
		}
		await flush();
	}

	// Catches: raw legacy {} becoming an undefined root in the actual refresh/restore path.
	it("refreshes_legacy_empty_layout_without_publishing_undefined_root", async () => {
		const { store, orderTerminalsByPaneLayout } = await windowStore();
		createRoot((dispose) => {
			cleanups.push(dispose);
			createComputed(() => orderTerminalsByPaneLayout(["term-1"]));
		});
		await refresh({});
		expect(warnings).toEqual([]);
		expect(store.getRoot()).toBeNull();
		expect(store.serialize()).toEqual(empty);
		expect(orderTerminalsByPaneLayout(["term-1"])).toEqual(["term-1"]);
	});

	// Catches: restore notifying tree readers while groups/focus still belong to the old layout.
	it("publishes_tree_groups_and_focus_as_one_restored_snapshot", async () => {
		document = single;
		const { store, orderTerminalsByPaneLayout } = await windowStore();
		const seen: unknown[] = [];
		createRoot((dispose) => {
			cleanups.push(dispose);
			createComputed(() => {
				seen.push({
					root: store.getRoot(),
					groups: Object.keys(store.state.groups),
					active: store.state.activeGroupId,
					order: orderTerminalsByPaneLayout(["term-3", "term-2"]),
				});
			});
		});
		seen.length = 0;
		await refresh(split);
		expect(warnings).toEqual([]);
		expect(seen).toEqual([{ root: split.root, groups: ["g2", "g3"], active: "g3", order: ["term-2", "term-3"] }]);
	});

	// Catches: defaults being applied only at restore, leaving a raw merge baseline that blocks the next peer update.
	it("adopts_missing_partial_single_and_split_layouts_in_two_windows_without_echo_saves", async () => {
		document = {};
		const first = await windowStore();
		vi.resetModules();
		const second = await windowStore();
		for (const [incoming, expected] of [
			[null, empty],
			[{}, empty],
			[{ groups: {} }, empty],
			[
				{ root: single.root, groups: single.groups },
				{ ...single, activeGroupId: null },
			],
			[split, split],
			[single, single],
			[{}, empty],
		] as const) {
			await refresh(incoming);
			for (const { store } of [first, second]) expect.soft(store.serialize()).toEqual(expected);
		}
		expect(warnings).toEqual([]);
		expect(io.invoke.mock.calls.filter(([command]) => command === "save_pane_layout")).toEqual([]);
	});
});
