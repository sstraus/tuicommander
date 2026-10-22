import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BranchDetail } from "../../components/GitPanel/types";

const h = vi.hoisted(() => ({
	invoke: vi.fn(),
	writeClipboard: vi.fn(),
	appLoggerError: vi.fn(),
}));

vi.mock("../../invoke", () => ({ invoke: h.invoke }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: h.writeClipboard }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn() }));
vi.mock("../../components/SmartButtonStrip/SmartButtonStrip", () => ({ SmartButtonStrip: () => null }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: h.appLoggerError },
}));

import { BranchesTab } from "../../components/GitPanel/BranchesTab";

const FEATURE: BranchDetail = {
	name: "feature/foo",
	is_current: false,
	is_remote: false,
	is_main: false,
	is_merged: true,
	ahead: 2,
	behind: 1,
	upstream: null,
	last_commit_date: new Date().toISOString(),
	last_commit_message: "Add foo",
	last_commit_author: "Bob",
	base_ahead: 0,
	base_behind: 3,
	base_branch: "main",
};

describe("BranchesTab Copy Name clipboard handling", () => {
	beforeEach(() => {
		h.invoke.mockReset().mockImplementation((cmd: string) => {
			if (cmd === "get_branches_detail") return Promise.resolve([FEATURE]);
			if (cmd === "get_recent_branches" || cmd === "list_base_ref_options") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});
		h.writeClipboard.mockReset().mockResolvedValue(undefined);
		h.appLoggerError.mockReset();
	});

	it("writes the branch name and logs nothing on success", async () => {
		const { findByText, getByText } = render(() => <BranchesTab repoPath="/repo/clip" />);
		await findByText("feature/foo");
		fireEvent.contextMenu(getByText("feature/foo"));
		fireEvent.click(getByText("Copy Name"));

		await waitFor(() => expect(h.writeClipboard).toHaveBeenCalledWith("feature/foo"));
		expect(h.appLoggerError).not.toHaveBeenCalled();
	});

	it("logs a denied clipboard write instead of leaving an unhandled rejection", async () => {
		const unhandled = vi.fn();
		process.on("unhandledRejection", unhandled);
		try {
			const err = new DOMException("Write permission denied.", "NotAllowedError");
			h.writeClipboard.mockRejectedValueOnce(err);
			const { findByText, getByText } = render(() => <BranchesTab repoPath="/repo/clip" />);
			await findByText("feature/foo");
			fireEvent.contextMenu(getByText("feature/foo"));
			fireEvent.click(getByText("Copy Name"));

			await waitFor(() => expect(h.appLoggerError).toHaveBeenCalledWith("git", "Failed to copy branch name", err));
			await new Promise((r) => setTimeout(r, 20));
			expect(unhandled).not.toHaveBeenCalled();
		} finally {
			process.off("unhandledRejection", unhandled);
		}
	});
});
