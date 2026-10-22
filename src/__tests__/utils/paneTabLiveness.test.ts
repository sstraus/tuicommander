import { beforeEach, describe, expect, it, vi } from "vitest";

const mockInvoke = vi.hoisted(() => vi.fn(() => Promise.resolve(null)));
vi.mock("../../invoke", () => ({ invoke: mockInvoke }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import type { PaneTab } from "../../stores/paneLayout";
import { terminalsStore } from "../../stores/terminals";
import { isPaneTabLive } from "../../utils/paneTabLiveness";

describe("isPaneTabLive", () => {
	beforeEach(() => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		diffTabsStore.clearAll();
		mdTabsStore.clearAll();
		editorTabsStore.clearAll();
	});

	it("terminal: true when the session exists in terminalsStore", () => {
		const id = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T", cwd: null, awaitingInput: null });
		expect(isPaneTabLive({ id, type: "terminal" })).toBe(true);
	});

	it("terminal: false for a ghost — no matching terminalsStore entry", () => {
		expect(isPaneTabLive({ id: "ghost-session", type: "terminal" })).toBe(false);
	});

	it("diff: true when the tab exists in diffTabsStore", () => {
		const id = diffTabsStore.add("/repo", "file.ts", "M");
		expect(isPaneTabLive({ id, type: "diff" })).toBe(true);
	});

	it("diff: false for a ghost — no matching diffTabsStore entry", () => {
		expect(isPaneTabLive({ id: "ghost-diff", type: "diff" })).toBe(false);
	});

	it("markdown: true when the tab exists in mdTabsStore", () => {
		const id = mdTabsStore.add("/repo", "notes.md");
		expect(isPaneTabLive({ id, type: "markdown" })).toBe(true);
	});

	it("markdown: false for a ghost — no matching mdTabsStore entry", () => {
		expect(isPaneTabLive({ id: "ghost-md", type: "markdown" })).toBe(false);
	});

	it("editor: true when the tab exists in editorTabsStore", () => {
		const id = editorTabsStore.add("/repo", "main.rs");
		expect(isPaneTabLive({ id, type: "editor" })).toBe(true);
	});

	it("editor: false for a ghost — no matching editorTabsStore entry", () => {
		expect(isPaneTabLive({ id: "ghost-editor", type: "editor" })).toBe(false);
	});

	it("a group with one live and one ghost tab still has live content", () => {
		const id = terminalsStore.add({ sessionId: null, fontSize: 14, name: "T", cwd: null, awaitingInput: null });
		const tabs: PaneTab[] = [
			{ id: "ghost-session", type: "terminal" },
			{ id, type: "terminal" },
		];
		expect(tabs.some(isPaneTabLive)).toBe(true);
	});

	it("a group with only ghost tabs across mixed types has no live content", () => {
		const tabs: PaneTab[] = [
			{ id: "ghost-term", type: "terminal" },
			{ id: "ghost-diff", type: "diff" },
			{ id: "ghost-md", type: "markdown" },
			{ id: "ghost-editor", type: "editor" },
		];
		expect(tabs.some(isPaneTabLive)).toBe(false);
	});
});
