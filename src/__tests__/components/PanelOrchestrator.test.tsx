import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Every child panel is stubbed to capture the `visible` prop it receives. The
// unit under test is PanelOrchestrator's visibility wiring, not the panels
// themselves.
const h = vi.hoisted(() => ({
	fileBrowser: [] as Array<{ visible: unknown }>,
	gitPanel: [] as Array<{ visible: unknown }>,
	markdown: [] as Array<{ visible: unknown }>,
}));

vi.mock("../../components/FileBrowserPanel", () => ({
	FileBrowserPanel: (props: { visible: unknown }) => {
		h.fileBrowser.push({ visible: props.visible });
		return null;
	},
}));
vi.mock("../../components/GitPanel/GitPanel", () => ({
	GitPanel: (props: { visible: unknown }) => {
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
		h.gitPanel = [];
		h.markdown = [];
		uiStore.setFileBrowserPanelVisible(false);
		uiStore.setGitPanelVisible(false);
		if (globalWorkspaceStore.isActive()) globalWorkspaceStore.deactivate();
		globalWorkspaceStore.setScope(MANUAL_SCOPE);
	});

	afterEach(() => {
		uiStore._testCancelPendingSave();
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
		expect(h.fileBrowser[0].visible).toBe(false);
	});

	it("suppresses Git Panel while the manual cross-repo workspace is active", () => {
		uiStore.setGitPanelVisible(true);
		globalWorkspaceStore.activate();
		expect(globalWorkspaceStore.getScope()).toBe(MANUAL_SCOPE);

		renderOrchestrator();
		expect(h.gitPanel[0].visible).toBe(false);
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
