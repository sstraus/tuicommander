import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

async function documentStores() {
	// A real reload destroys the previous document and all its pending timers.
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.resetModules();
	const [
		{ mdTabsStore: md },
		{ editorTabsStore: editor },
		{ diffTabsStore: diff },
		reload,
		{ tabOrderingStore: ordering },
	] = await Promise.all([
		import("../../stores/mdTabs"),
		import("../../stores/editorTabs"),
		import("../../stores/diffTabs"),
		import("../../stores/tabReload"),
		import("../../stores/tabManager"),
	]);
	const { appLogger } = await import("../../stores/appLogger");
	vi.useFakeTimers();
	return {
		appLogger,
		md,
		editor,
		diff,
		reload,
		ordering,
		finish: () => {
			md.restoreAfterReload();
			reload.restoreNonTerminalTabsAfterReload();
		},
	};
}

describe("non-terminal tabs across window reload", () => {
	beforeEach(() => sessionStorage.clear());
	afterEach(() => {
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.restoreAllMocks();
	});

	// Catches: reload restores only terminals and MCP Markdown, losing ordinary tabs.
	it("retains user files, generated panels, iframe content, diffs and editor tabs", async () => {
		const old = await documentStores();
		old.finish();
		const file = old.md.add("/repo", "README.md");
		old.md.openUiTab("report", "Report", "<p>Retained report</p>", false);
		old.md.addVirtual("Plan", "plan:file?path=/repo/plans/a.md", "/repo");
		old.md.addTasks();
		old.md.addClaudeUsage("claude-session");
		old.md.addCodexUsage("codex-session");
		old.md.addGrokUsage("grok-session");
		old.md.addGithubOps("/repo");
		old.md.addCommandOverview();
		old.md.addPrDiff("/repo", 42, "Example PR", "diff --git a/a.ts b/a.ts");
		old.md.addHtmlPreview("/repo", "preview.html");
		const diff = old.diff.add("/repo", "src/main.ts", "M");
		const edit = old.editor.add("/repo", "src/main.ts", 12);
		old.md.setPinned(file, true);
		old.ordering.reorder(edit, file, "before");
		const order = [...old.ordering.state.order];
		const next = await documentStores();
		next.finish();
		expect(next.md.getCount()).toBe(11);
		expect(
			Object.values(next.md.state.tabs)
				.map((tab) => tab.type)
				.sort(),
		).toEqual([
			"claude-usage",
			"codex-usage",
			"command-overview",
			"file",
			"github-ops",
			"grok-usage",
			"html-preview",
			"plugin-panel",
			"pr-diff",
			"tasks",
			"virtual",
		]);
		expect(Object.values(next.md.state.tabs)).toEqual(
			expect.arrayContaining([
				expect.objectContaining({ type: "file", filePath: "README.md", repoPath: "/repo", pinned: true }),
				expect.objectContaining({ type: "plugin-panel", title: "Report", html: "<p>Retained report</p>" }),
				expect.objectContaining({ type: "virtual", contentUri: "plan:file?path=/repo/plans/a.md" }),
				expect.objectContaining({ type: "tasks" }),
			]),
		);
		expect(next.diff.get(diff)).toMatchObject({ filePath: "src/main.ts", status: "M" });
		expect(next.editor.get(edit)).toMatchObject({ filePath: "src/main.ts", initialLine: 12 });
		expect(next.editor.state.activeId).toBe(edit);
		expect(next.md.state.activeId).toBeNull();
		expect(next.diff.state.activeId).toBeNull();
		expect(next.ordering.state.order).toEqual(order);
	});

	// Catches: stale snapshots resurrect closes or allocate an already-open ID.
	it("persists close and pin changes without unload and reserves restored IDs", async () => {
		const old = await documentStores();
		old.finish();
		const keep = old.editor.add("/repo", "keep.ts");
		const closed = old.editor.add("/repo", "closed.ts");
		old.editor.setPinned(keep, true);
		old.editor.remove(closed);
		const next = await documentStores();
		next.finish();
		expect(next.editor.getIds()).toEqual([keep]);
		expect(next.editor.get(keep)?.pinned).toBe(true);
		const added = next.editor.add("/repo", "new.ts");
		expect(added).not.toBe(keep);
		expect(next.editor.get(keep)?.filePath).toBe("keep.ts");
	});

	// Catches: startup deactivation overwrites the unread reload snapshot.
	it("preserves unread definitions through an interrupted boot and keeps background tabs inactive", async () => {
		const old = await documentStores();
		old.finish();
		old.md.addFileBackground("/repo", "background.md");
		const interrupted = await documentStores();
		interrupted.reload.saveNonTerminalTabsForReload();
		const next = await documentStores();
		next.finish();
		expect(next.md.getCount()).toBe(1);
		expect(next.md.state.activeId).toBeNull();
	});

	// Catches: a saved selection overrides an explicit document opened during startup.
	it("keeps a new startup open selected and does not replay selection twice", async () => {
		const old = await documentStores();
		old.finish();
		old.editor.add("/repo", "old.ts");
		const next = await documentStores();
		const fresh = next.md.openUiTab("fresh", "Fresh", "<p>New</p>", false);
		next.finish();
		expect(next.md.state.activeId).toBe(fresh);
		expect(next.editor.state.activeId).toBeNull();
		next.md.setActive(null);
		next.reload.restoreNonTerminalTabsAfterReload();
		expect(next.md.state.activeId).toBeNull();
	});

	// Catches: only the first dirty transition is persisted, losing later edits without unload.
	it("keeps the latest draft and its disk baseline without needing unload, including empty text", async () => {
		const old = await documentStores();
		old.finish();

		const id = old.editor.add("/repo", "draft.ts");
		old.editor.setReloadDraft(id, { text: "first edit", savedContent: "disk baseline" });
		old.editor.setDirty(id, true);
		old.editor.setReloadDraft(id, { text: "latest edit", savedContent: "disk baseline" });
		vi.advanceTimersByTime(400);
		expect(
			JSON.parse(sessionStorage.getItem("tui-commander-tabs-reload-editor")!).tabs.find(
				(tab: { id: string }) => tab.id === id,
			)?.reloadDraft,
		).toEqual({ text: "latest edit", savedContent: "disk baseline" });

		const next = await documentStores();
		next.finish();
		expect(next.editor.get(id)?.reloadDraft).toEqual({ text: "latest edit", savedContent: "disk baseline" });

		next.editor.setReloadDraft(id, { text: "", savedContent: "disk baseline" });
		vi.advanceTimersByTime(400);

		const empty = await documentStores();
		empty.finish();
		expect(empty.editor.get(id)?.reloadDraft).toEqual({ text: "", savedContent: "disk baseline" });
	});

	// Catches: an MCP identity moved to an editor at boot returns as stale Markdown too.
	it("keeps the current MCP editor identity instead of reviving its old Markdown tab", async () => {
		const old = await documentStores();
		old.finish();
		old.md.addMcpFile("shared-document", "/repo", "old.md", false, false);
		const next = await documentStores();
		next.md.closeMcpFile("shared-document");
		const fresh = next.editor.addMcpFile("shared-document", "/repo", "new.ts", undefined, false, {
			background: false,
			externalEditable: false,
		});
		next.finish();
		expect(next.md.getCount()).toBe(0);
		expect(next.editor.getCount()).toBe(1);
		expect(next.editor.state.activeId).toBe(fresh);
		expect(next.editor.get(fresh)?.filePath).toBe("new.ts");
	});
	// Catches: per-keystroke serialization blocks input, or an unload misses the pending draft.
	it("coalesces editor writes but flushes a pending edit synchronously", async () => {
		const old = await documentStores();

		old.finish();
		const id = old.editor.add("/repo", "typing.ts");
		old.editor.setDirty(id, true);
		old.editor.setReloadDraft(id, { text: "a", savedContent: "" });
		vi.advanceTimersByTime(400);

		const writes = vi.spyOn(sessionStorage, "setItem");
		old.editor.setCursor(id, 2, 3);
		old.editor.setReloadDraft(id, { text: "ab", savedContent: "" });
		old.editor.setReloadDraft(id, { text: "abc", savedContent: "" });
		expect(writes).not.toHaveBeenCalled();
		old.reload.saveNonTerminalTabsForReload();
		expect(JSON.parse(sessionStorage.getItem("tui-commander-tabs-reload-editor")!).tabs[0].reloadDraft.text).toBe(
			"abc",
		);
		writes.mockRestore();
	});

	// Catches: one large draft exhausts storage and erases every editor tab definition.
	it("omits an oversized draft while retaining tab definitions and smaller drafts", async () => {
		const old = await documentStores();
		old.finish();
		const warn = vi.spyOn(old.appLogger, "warn").mockImplementation(() => {});
		const big = old.editor.add("/repo", "big.ts");
		const small = old.editor.add("/repo", "small.ts");
		old.editor.setReloadDraft(big, { text: "x".repeat(1_000_001), savedContent: "" });
		old.editor.setReloadDraft(small, { text: "small edit", savedContent: "base" });
		old.reload.saveNonTerminalTabsForReload();
		old.reload.saveNonTerminalTabsForReload();
		const next = await documentStores();
		next.finish();
		expect(next.editor.getIds()).toEqual([big, small]);
		expect(next.editor.get(big)?.reloadDraft).toBeUndefined();
		expect(next.editor.get(small)?.reloadDraft?.text).toBe("small edit");
		expect(warn).toHaveBeenCalledTimes(1);
		warn.mockRestore();
	});
	// Catches: shared storage pressure discards clean editor definitions along with draft data.
	it("retries editor definitions without drafts when storage rejects the full snapshot", async () => {
		const old = await documentStores();
		old.finish();
		const dirty = old.editor.add("/repo", "draft.ts");
		const clean = old.editor.add("/repo", "clean.ts");
		const write = sessionStorage.setItem.bind(sessionStorage);
		const warning = vi.spyOn(old.appLogger, "warn").mockImplementation(() => {});
		vi.spyOn(sessionStorage, "setItem").mockImplementation((key, value) => {
			if (key === "tui-commander-tabs-reload-editor" && value.length > 1500)
				throw new DOMException("quota", "QuotaExceededError");
			write(key, value);
		});
		old.editor.setReloadDraft(dirty, { text: "x".repeat(2000), savedContent: "base" });
		old.reload.saveNonTerminalTabsForReload();
		expect(warning).toHaveBeenCalledWith(
			"store",
			"Editor reload snapshot retained tab definitions but could not retain drafts",
			expect.any(DOMException),
		);
		const next = await documentStores();
		next.finish();
		expect(next.editor.getIds()).toEqual([dirty, clean]);
		expect(next.editor.get(dirty)?.reloadDraft).toBeUndefined();
	});
});
