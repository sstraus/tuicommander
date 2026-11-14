import { render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BranchDetail } from "../../components/GitPanel/types";

const h = vi.hoisted(() => ({ invoke: vi.fn(), appLoggerWarn: vi.fn() }));

vi.mock("../../invoke", () => ({ invoke: h.invoke }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: vi.fn() }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn() }));
vi.mock("../../components/SmartButtonStrip/SmartButtonStrip", () => ({ SmartButtonStrip: () => null }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), info: vi.fn(), warn: h.appLoggerWarn, error: vi.fn() },
}));

import { BranchesTab } from "../../components/GitPanel/BranchesTab";

function local(name: string): BranchDetail {
	return {
		name,
		is_current: false,
		is_remote: false,
		is_main: false,
		ahead: null,
		behind: null,
		upstream: null,
		last_commit_date: null,
		last_commit_message: null,
		last_commit_author: null,
		base_ahead: null,
		base_behind: null,
		base_branch: null,
	};
}

describe("BranchesTab merged badges (#2777-1579)", () => {
	let resolveMerged: (names: string[]) => void;
	let rejectMerged: (err: Error) => void;

	beforeEach(() => {
		h.appLoggerWarn.mockReset();
		h.invoke.mockReset().mockImplementation((cmd: string) => {
			if (cmd === "get_branches_detail") return Promise.resolve([local("feat-done"), local("feat-open")]);
			if (cmd === "get_merged_branches")
				return new Promise<string[]>((resolve, reject) => {
					resolveMerged = resolve;
					rejectMerged = reject;
				});
			if (cmd === "get_recent_branches" || cmd === "list_base_ref_options") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});
	});

	// Catches: the list waiting for the per-branch merge classification, which
	// held the Branches tab empty for tens of seconds on a 528-branch repository.
	it("renders the rows before the merge classification finishes", async () => {
		const { findByText, queryByText } = render(() => <BranchesTab repoPath="/repo/many" />);
		await findByText("feat-done");
		await findByText("feat-open");
		expect(queryByText("merged")).toBeNull();
		resolveMerged([]);
		await waitFor(() => expect(h.invoke).toHaveBeenCalledWith("get_merged_branches", { path: "/repo/many" }));
	});

	// Catches: the merged badge lost when it no longer arrives with the list.
	it("marks exactly the merged branch once the classification resolves", async () => {
		const { findByText, getAllByText } = render(() => <BranchesTab repoPath="/repo/many" />);
		const done = await findByText("feat-done");
		resolveMerged(["feat-done"]);
		await waitFor(() => expect(getAllByText("merged")).toHaveLength(1));
		expect(done.closest("[title]")?.textContent).toContain("merged");
		const open = await findByText("feat-open");
		expect(open.closest("[title]")?.textContent).not.toContain("merged");
	});

	// Catches: a failed classification hiding the list or failing silently.
	it("keeps the rows and logs a warning when the classification fails", async () => {
		const { findByText, queryByText } = render(() => <BranchesTab repoPath="/repo/many" />);
		await findByText("feat-open");
		const err = new Error("boom");
		rejectMerged(err);
		await waitFor(() =>
			expect(h.appLoggerWarn).toHaveBeenCalledWith("git", "Failed to determine merged branches", err),
		);
		expect(queryByText("feat-open")).not.toBeNull();
		expect(queryByText("merged")).toBeNull();
	});
});
