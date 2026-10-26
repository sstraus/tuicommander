import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Child tabs + panel chrome are stubbed to capture the repoPath prop they
// receive. The unit under test is GitPanel's "visible-gating" perf guard:
// when the panel is hidden it must pass repoPath=null to the active tab so no
// hidden tab fetches git data.
const h = vi.hoisted(() => ({ changes: [] as Array<{ repoPath: unknown; storeRepoPath: unknown }> }));

vi.mock("../../components/GitPanel/ChangesTab", () => ({
	ChangesTab: (props: { repoPath: unknown; storeRepoPath: unknown }) => {
		h.changes.push({ repoPath: props.repoPath, storeRepoPath: props.storeRepoPath });
		return null;
	},
}));
vi.mock("../../components/GitPanel/LogTab", () => ({ LogTab: () => null }));
vi.mock("../../components/GitPanel/StashesTab", () => ({ StashesTab: () => null }));
vi.mock("../../components/GitPanel/BranchesTab", () => ({ BranchesTab: () => null }));
vi.mock("../../components/GitPanel/HistoryTab", () => ({ HistoryTab: () => null }));
vi.mock("../../components/GitPanel/BlameTab", () => ({ BlameTab: () => null }));
vi.mock("../../components/ui/PanelWindowControls", () => ({ PanelWindowControls: () => null }));
vi.mock("../../components/ui/PanelResizeHandle", () => ({ PanelResizeHandle: () => null }));

import { GitPanel } from "../../components/GitPanel/GitPanel";

describe("GitPanel visible-gating perf guard", () => {
	beforeEach(() => {
		h.changes = [];
	});
	afterEach(() => vi.clearAllMocks());

	it("passes null repoPath to the active tab when the panel is hidden", () => {
		render(() => <GitPanel visible={false} repoPath="/repo" onClose={vi.fn()} />);
		expect(h.changes).toHaveLength(1);
		expect(h.changes[0].repoPath).toBeNull();
		expect(h.changes[0].storeRepoPath).toBeNull();
	});

	it("passes the real repoPath to the active tab when the panel is visible", () => {
		render(() => <GitPanel visible={true} repoPath="/repo" onClose={vi.fn()} />);
		expect(h.changes).toHaveLength(1);
		expect(h.changes[0].repoPath).toBe("/repo");
		expect(h.changes[0].storeRepoPath).toBe("/repo");
	});

	it("prefers fsRoot (worktree path) over repoPath when visible", () => {
		render(() => <GitPanel visible={true} repoPath="/repo" fsRoot="/repo/.wt/feature" onClose={vi.fn()} />);
		expect(h.changes[0].repoPath).toBe("/repo/.wt/feature"); // gitPath() = fsRoot || repoPath
		expect(h.changes[0].storeRepoPath).toBe("/repo"); // storeRepoPath stays the canonical repo
	});
});

it("catches shared Git view tabs losing keyboard selection and changes remount", async () => {
	const { fireEvent, screen } = await import("@solidjs/testing-library");
	render(() => <GitPanel visible={true} repoPath="/repo" onClose={vi.fn()} />);
	const changes = screen.getByRole("tab", { name: "Changes" });
	expect(changes).toHaveAttribute("aria-selected", "true");
	fireEvent.keyDown(changes, { key: "ArrowRight" });
	expect(screen.getByRole("tab", { name: "Log" })).toHaveAttribute("aria-selected", "true");
	expect(screen.getByRole("tab", { name: "Log" })).toHaveFocus();
	fireEvent.keyDown(screen.getByRole("tab", { name: "Log" }), { key: "Home" });
	expect(changes).toHaveAttribute("aria-selected", "true");
	expect(h.changes.at(-1)?.repoPath).toBe("/repo");
});
