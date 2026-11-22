import { cleanup, render } from "@solidjs/testing-library";
import { onCleanup } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Every child panel is stubbed to capture the `visible` prop it receives. The
// unit under test is PanelOrchestrator's visibility wiring, not the panels
// themselves.
const h = vi.hoisted(() => ({
	disposed: [] as string[],
	retainGit: undefined as ((retain: boolean) => void) | undefined,
	fileBrowser: [] as Array<{ visible: unknown }>,
	gitPanel: [] as Array<{ visible: unknown }>,
	markdown: [] as Array<{ visible: unknown }>,
}));

vi.mock("../../components/FileBrowserPanel", () => ({
	FileBrowserPanel: (props: { visible: unknown }) => {
		h.fileBrowser.push({ visible: props.visible });
		onCleanup(() => h.disposed.push("files"));
		return null;
	},
}));
vi.mock("../../components/GitPanel/GitPanel", () => ({
	GitPanel: (props: { visible: unknown; onRetainChange: (retain: boolean) => void }) => {
		h.retainGit = props.onRetainChange;
		onCleanup(() => h.disposed.push("git"));
		h.gitPanel.push({ visible: props.visible });
		return null;
	},
}));
vi.mock("../../components/MarkdownPanel", () => ({
	MarkdownPanel: (props: { visible: unknown }) => {
		h.markdown.push({ visible: props.visible });
		return null;
	},
}));
vi.mock("../../components/IdeasPanel", () => ({ IdeasPanel: () => null }));
vi.mock("../../components/OutlinePanel", () => ({ OutlinePanel: () => null }));
vi.mock("../../components/ReferencesPanel", () => ({ ReferencesPanel: () => null }));
vi.mock("../../components/AIChatPanel", () => ({ AIChatPanel: () => null }));

import { PanelOrchestrator } from "../../components/PanelOrchestrator";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../../stores/globalWorkspace";
import { uiStore } from "../../stores/ui";

function renderOrchestrator() {
	return render(() => <PanelOrchestrator repoPath="/repo" fsRoot={null} onFileOpen={() => {}} />);
}

describe("PanelOrchestrator", () => {
	beforeEach(() => {
		h.fileBrowser = [];
		h.disposed = [];
		h.retainGit = undefined;
		h.gitPanel = [];
		h.markdown = [];
		uiStore.setFileBrowserPanelVisible(false);
		uiStore.setGitPanelVisible(false);
		if (globalWorkspaceStore.isActive()) globalWorkspaceStore.deactivate();
		globalWorkspaceStore.setScope(MANUAL_SCOPE);
	});

	afterEach(() => {
		cleanup();
		uiStore._testCancelPendingSave();
		vi.useRealTimers();
	});

	it("does not mount closed panels or let an old close timer evict a reopened panel", () => {
		vi.useFakeTimers();
		renderOrchestrator();
		expect(h.fileBrowser).toHaveLength(0);
		expect(h.gitPanel).toHaveLength(0);
		uiStore.setFileBrowserPanelVisible(true);
		uiStore.setGitPanelVisible(true);
		vi.advanceTimersByTime(59_999);
		expect(h.disposed).toEqual([]);
		uiStore.setFileBrowserPanelVisible(true);
		vi.advanceTimersByTime(60_000);
		expect(h.disposed).toEqual(["git"]);
		expect(h.fileBrowser).toHaveLength(1);
		uiStore.setFileBrowserPanelVisible(false);
		vi.advanceTimersByTime(60_000);
		expect(h.disposed).toEqual(["git", "files"]);
		uiStore.setFileBrowserPanelVisible(true);
		expect(h.fileBrowser).toHaveLength(2);
	});

	it("does not evict Git while its active tab holds a draft", () => {
		vi.useFakeTimers();
		uiStore.setGitPanelVisible(true);
		renderOrchestrator();
		h.retainGit!(true);
		uiStore.setGitPanelVisible(false);
		vi.advanceTimersByTime(120_000);
		expect(h.disposed).toEqual([]);
		h.retainGit!(false);
		vi.advanceTimersByTime(60_000);
		expect(h.disposed).toEqual(["git"]);
	});

	it("shows File Browser when open and no global workspace is active", () => {
		uiStore.setFileBrowserPanelVisible(true);
		renderOrchestrator();
		expect(h.fileBrowser[0].visible).toBe(true);
	});

	it("shows Git Panel when open and no global workspace is active", () => {
		uiStore.setGitPanelVisible(true);
		renderOrchestrator();
		expect(h.gitPanel[0].visible).toBe(true);
	});

	it("suppresses File Browser while the manual cross-repo workspace is active", () => {
		uiStore.setFileBrowserPanelVisible(true);
		globalWorkspaceStore.activate();
		expect(globalWorkspaceStore.getScope()).toBe(MANUAL_SCOPE);

		renderOrchestrator();
		expect(h.fileBrowser).toHaveLength(0);
	});

	it("suppresses Git Panel while the manual cross-repo workspace is active", () => {
		uiStore.setGitPanelVisible(true);
		globalWorkspaceStore.activate();
		expect(globalWorkspaceStore.getScope()).toBe(MANUAL_SCOPE);

		renderOrchestrator();
		expect(h.gitPanel).toHaveLength(0);
	});

	// Regression test for the bug: a repo with its "auto-consolidate worktrees"
	// setting on (#e767) activates the global workspace under its own repo-path
	// scope, not MANUAL_SCOPE. That scope still has one well-defined repo, so
	// File Browser / Git Panel must keep working instead of going dark on every
	// tab of that repo.
	it("does not suppress File Browser when a per-repo consolidated workspace is active", () => {
		uiStore.setFileBrowserPanelVisible(true);
		globalWorkspaceStore.setScope("/repo/a");
		globalWorkspaceStore.activate();
		expect(globalWorkspaceStore.isActive()).toBe(true);
		expect(globalWorkspaceStore.getScope()).toBe("/repo/a");

		renderOrchestrator();
		expect(h.fileBrowser[0].visible).toBe(true);
	});

	it("does not suppress Git Panel when a per-repo consolidated workspace is active", () => {
		uiStore.setGitPanelVisible(true);
		globalWorkspaceStore.setScope("/repo/a");
		globalWorkspaceStore.activate();
		expect(globalWorkspaceStore.isActive()).toBe(true);
		expect(globalWorkspaceStore.getScope()).toBe("/repo/a");

		renderOrchestrator();
		expect(h.gitPanel[0].visible).toBe(true);
	});

	it("never gates unrelated panels on global workspace state", () => {
		uiStore.setMarkdownPanelVisible(true);
		globalWorkspaceStore.activate();

		renderOrchestrator();
		expect(h.markdown[0].visible).toBe(true);
	});
});
