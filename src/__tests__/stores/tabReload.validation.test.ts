import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

async function freshDocument() {
	// A real reload destroys the previous document and all its pending timers.
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.resetModules();
	const reload = await import("../../stores/tabReload");
	const { diffTabsStore } = await import("../../stores/diffTabs");
	const { editorTabsStore } = await import("../../stores/editorTabs");
	const { mdTabsStore } = await import("../../stores/mdTabs");
	const { appLogger } = await import("../../stores/appLogger");
	/** Same order as initApp's finally block. */
	const finishBoot = () => {
		try {
			mdTabsStore.restoreAfterReload();
		} finally {
			reload.restoreNonTerminalTabsAfterReload();
		}
	};
	vi.useFakeTimers();
	return { reload, diffTabsStore, editorTabsStore, mdTabsStore, appLogger, finishBoot };
}

describe("non-terminal tab reload recovery boundaries", () => {
	beforeEach(() => sessionStorage.clear());
	afterEach(() => {
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.restoreAllMocks();
	});

	// Catches: user drag order inside a store is reset to insertion order by reload.
	it("preserves a reordered tab sequence", async () => {
		const old = await freshDocument();
		old.finishBoot();
		const a = old.diffTabsStore.add("/repo", "a.ts", "M");
		const b = old.diffTabsStore.add("/repo", "b.ts", "M");
		old.diffTabsStore.reorderByIds(b, a, "before");
		const next = await freshDocument();
		next.finishBoot();
		expect(next.diffTabsStore.getVisibleIds("/repo|main")).toEqual([b, a]);
	});

	// Catches: an MCP Markdown document restored by both snapshots appears twice.
	it("restores an MCP Markdown document exactly once, keeping its id", async () => {
		const old = await freshDocument();
		old.finishBoot();
		const id = old.mdTabsStore.addMcpFile("digest", "", "/abs/digest.md", true, false);
		const next = await freshDocument();
		next.finishBoot();
		expect(next.mdTabsStore.getIds()).toEqual([id]);
		expect(next.mdTabsStore.get(id)).toMatchObject({ mcpUiId: "digest", pinned: true });
	});

	// Catches: a corrupt snapshot throws at store construction and breaks the whole app import.
	it.each([
		["not JSON", "{"],
		["wrong version", JSON.stringify({ version: 2, tabs: [{ id: "diff-1" }], order: ["diff-1"] })],
		["foreign id prefix", JSON.stringify({ version: 1, tabs: [{ id: "edit-1" }], order: ["edit-1"] })],
		["zero id", JSON.stringify({ version: 1, tabs: [{ id: "diff-0" }], order: [] })],
	])("ignores an invalid diff snapshot (%s)", async (_label, raw) => {
		const warning = vi.spyOn(console, "warn").mockImplementation(() => {});
		sessionStorage.setItem("tui-commander-tabs-reload-diff", raw);
		const next = await freshDocument();
		next.finishBoot();
		expect(next.diffTabsStore.getCount()).toBe(0);
		expect(warning).toHaveBeenCalledWith("[store]", expect.stringMatching(/reload/));
	});

	// Catches: a structurally broken tab (no filePath) is mounted and crashes the
	// editor pane on every reload of this window.
	it("drops a restored editor tab that lacks its file path", async () => {
		const warning = vi.spyOn(console, "warn").mockImplementation(() => {});
		sessionStorage.setItem(
			"tui-commander-tabs-reload-editor",
			JSON.stringify({ version: 1, tabs: [{ id: "edit-1", isDirty: false }], order: ["edit-1"], activeId: "edit-1" }),
		);
		const next = await freshDocument();
		next.finishBoot();
		expect(next.editorTabsStore.getCount()).toBe(0);
		expect(warning).toHaveBeenCalledWith("[store]", "Dropped 1 invalid editor reload tabs");
	});

	// Catches: a draft with a non-string text reaches CodeMirror and breaks the pane.
	it("drops a restored editor tab whose draft is malformed", async () => {
		const warning = vi.spyOn(console, "warn").mockImplementation(() => {});
		const tab = { id: "edit-1", repoPath: "/r", fsRoot: "/r", filePath: "a.ts", fileName: "a.ts", isDirty: true };
		sessionStorage.setItem(
			"tui-commander-tabs-reload-editor",
			JSON.stringify({
				version: 1,
				tabs: [
					{ ...tab, reloadDraft: { text: 42, savedContent: "" } },
					{ ...tab, id: "edit-2" },
				],
				order: ["edit-1", "edit-2"],
			}),
		);
		const next = await freshDocument();
		next.finishBoot();
		expect(next.editorTabsStore.getIds()).toEqual(["edit-2"]);
		expect(warning).toHaveBeenCalledWith("[store]", "Dropped 1 invalid editor reload tabs");
	});

	// Catches: a failed write (quota: PR diff bodies, plugin HTML and editor drafts
	// share ~5 MB) leaves the previous snapshot, so a closed tab returns; and the
	// failure must be visible, not a silent false success.
	it("a failed snapshot write never leaves an older snapshot to resurrect a closed tab", async () => {
		const old = await freshDocument();
		old.finishBoot();
		old.diffTabsStore.add("/repo", "a.ts", "M");
		const gone = old.diffTabsStore.add("/repo", "b.ts", "M");
		const setItem = vi.spyOn(sessionStorage, "setItem").mockImplementation(() => {
			throw new DOMException("quota", "QuotaExceededError");
		});
		const warn = vi.spyOn(old.appLogger, "warn").mockImplementation(() => {});
		old.diffTabsStore.remove(gone);
		setItem.mockRestore();
		expect(warn).toHaveBeenCalledWith("store", "Could not save diff tabs for reload", expect.any(DOMException));
		const next = await freshDocument();
		next.finishBoot();
		expect(next.diffTabsStore.getIds()).not.toContain(gone);
	});
});
