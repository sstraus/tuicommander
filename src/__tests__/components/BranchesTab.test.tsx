import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BranchDetail } from "../../components/GitPanel/types";

const h = vi.hoisted(() => ({
	invoke: vi.fn(),
	writeClipboard: vi.fn().mockResolvedValue(undefined),
	handleOpenUrl: vi.fn(),
	toastAdd: vi.fn(),
	appLoggerError: vi.fn(),
	appLoggerWarn: vi.fn(),
	appLoggerInfo: vi.fn(),
}));

vi.mock("../../invoke", () => ({ invoke: h.invoke }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: h.writeClipboard }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: h.handleOpenUrl }));
vi.mock("../../components/SmartButtonStrip/SmartButtonStrip", () => ({ SmartButtonStrip: () => null }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: h.toastAdd } }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: {
		debug: vi.fn(),
		info: h.appLoggerInfo,
		warn: h.appLoggerWarn,
		error: h.appLoggerError,
	},
}));

import { BranchesTab, remoteUrlToGitHub } from "../../components/GitPanel/BranchesTab";

const REPO = "/repo/branches-tab-test";

const MAIN: BranchDetail = {
	name: "main",
	is_current: true,
	is_remote: false,
	is_main: true,
	is_merged: false,
	ahead: 0,
	behind: 0,
	upstream: "origin/main",
	last_commit_date: new Date().toISOString(),
	last_commit_message: "Initial commit",
	last_commit_author: "Alice",
	base_ahead: null,
	base_behind: null,
	base_branch: null,
};

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

const REMOTE_ONLY: BranchDetail = {
	name: "origin/feature/bar",
	is_current: false,
	is_remote: true,
	is_main: false,
	is_merged: false,
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

const GIT_OK = { success: true, stdout: "", stderr: "", exit_code: 0 };

/** Default RPC responses, overridable per test via `overrides`. */
function mockInvokeDefaults(overrides: Record<string, (args: Record<string, unknown>) => unknown> = {}) {
	h.invoke.mockImplementation((cmd: string, args: Record<string, unknown>) => {
		if (overrides[cmd]) return Promise.resolve(overrides[cmd](args));
		switch (cmd) {
			case "get_branches_detail":
				return Promise.resolve([MAIN, FEATURE, REMOTE_ONLY]);
			case "get_recent_branches":
				// Default: no "Recent" section, so branch names appear exactly once —
				// the dedicated "Recent" section test opts back in explicitly.
				return Promise.resolve([]);
			case "list_base_ref_options":
				return Promise.resolve([]);
			case "run_git_command":
				return Promise.resolve(GIT_OK);
			default:
				return Promise.resolve(undefined);
		}
	});
}

function openContextMenuFor(getByText: (t: string) => HTMLElement, branchName: string) {
	fireEvent.contextMenu(getByText(branchName));
}

describe("BranchesTab", () => {
	beforeEach(() => {
		h.invoke.mockReset();
		h.writeClipboard.mockReset().mockResolvedValue(undefined);
		h.handleOpenUrl.mockReset();
		h.toastAdd.mockReset();
		h.appLoggerError.mockReset();
		h.appLoggerWarn.mockReset();
		h.appLoggerInfo.mockReset();
		mockInvokeDefaults();
	});

	afterEach(() => {
		// Restore vi.spyOn spies (e.g. repositoriesStore.bumpGitRevision) so one test's
		// calls can never satisfy another test's assertion.
		vi.restoreAllMocks();
	});

	describe("rendering", () => {
		it("shows a loading state, then the fetched branches", async () => {
			const { getByText, findByText } = render(() => <BranchesTab repoPath={REPO} />);
			expect(getByText("Loading branches...")).toBeTruthy();

			await findByText("main");
			expect(getByText("feature/foo")).toBeTruthy();
			expect(getByText("origin/feature/bar")).toBeTruthy();
		});

		it("shows an empty state when there are no branches", async () => {
			mockInvokeDefaults({ get_branches_detail: () => [], get_recent_branches: () => [] });
			const { findByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("No branches");
		});

		it("renders a Recent section with branches matching recent reflog names", async () => {
			mockInvokeDefaults({ get_recent_branches: () => ["feature/foo"] });
			const { findByText, getByText, getAllByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");

			expect(getByText("Recent")).toBeTruthy();
			expect(getByText("Local")).toBeTruthy();
			expect(getByText("Remote")).toBeTruthy();
			// "feature/foo" now appears in both Recent and Local.
			expect(getAllByText("feature/foo")).toHaveLength(2);
		});

		it("does not render a Remote section when there are no remote branches", async () => {
			mockInvokeDefaults({ get_branches_detail: () => [MAIN, FEATURE], get_recent_branches: () => [] });
			const { findByText, queryByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			expect(queryByText("Remote")).toBeNull();
		});

		it("marks merged branches and stale/ahead/behind indicators", async () => {
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			expect(getByText("merged")).toBeTruthy();
			expect(getByText("↑2")).toBeTruthy();
			expect(getByText("↓1")).toBeTruthy();
		});

		it("re-fetches when the repo's git revision bumps, not on an unrelated revision", async () => {
			const { repositoriesStore } = await import("../../stores/repositories");
			const { findByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			const callsBefore = h.invoke.mock.calls.filter((c) => c[0] === "get_branches_detail").length;

			repositoriesStore.bumpGitRevision(REPO);
			await waitFor(() => {
				expect(h.invoke.mock.calls.filter((c) => c[0] === "get_branches_detail").length).toBeGreaterThan(callsBefore);
			});
		});

		it("logs and clears branches when the fetch fails", async () => {
			mockInvokeDefaults({
				get_branches_detail: () => Promise.reject(new Error("git error")),
			});
			const { findByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("No branches");
			expect(h.appLoggerError).toHaveBeenCalledWith("git", "Failed to load branches", expect.any(Error));
		});
	});

	describe("search filtering", () => {
		it("filters local and remote branches by query, always keeping the current branch", async () => {
			const { findByText, getByPlaceholderText, queryByText, getByText } = render(() => (
				<BranchesTab repoPath={REPO} />
			));
			await findByText("main");

			fireEvent.input(getByPlaceholderText("Filter branches... (/ to focus)"), { target: { value: "bar" } });

			expect(getByText("main")).toBeTruthy(); // current branch always shown
			expect(queryByText("feature/foo")).toBeNull();
			expect(getByText("origin/feature/bar")).toBeTruthy();
		});
	});

	describe("checkout", () => {
		it("checks out a local branch and bumps the git revision", async () => {
			const { repositoriesStore } = await import("../../stores/repositories");
			const bumpSpy = vi.spyOn(repositoriesStore, "bumpGitRevision");
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");

			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Checkout"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("switch_branch", {
					repoPath: REPO,
					branchName: "feature/foo",
					force: false,
					stash: false,
				});
			});
			expect(bumpSpy).toHaveBeenCalledWith(REPO);
		});

		it("checks out a remote branch via checkout_remote_branch, stripping the origin/ prefix", async () => {
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("origin/feature/bar");

			openContextMenuFor(getByText, "origin/feature/bar");
			fireEvent.click(getByText("Checkout (create local)"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("checkout_remote_branch", {
					repoPath: REPO,
					branchName: "feature/bar",
				});
			});
		});

		it("opens the dirty-worktree dialog when checkout fails with a dirty working tree, and Stash and Switch retries with stash=true", async () => {
			mockInvokeDefaults({
				switch_branch: () => Promise.reject("error: your local changes would be overwritten (dirty)"),
			});
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");

			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Checkout"));

			await findByText("Uncommitted changes");
			mockInvokeDefaults(); // subsequent switch_branch calls succeed
			fireEvent.click(getByText("Stash and Switch"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("switch_branch", {
					repoPath: REPO,
					branchName: "feature/foo",
					force: false,
					stash: true,
				});
			});
		});

		it("Force Switch retries the dirty checkout with force=true", async () => {
			mockInvokeDefaults({ switch_branch: () => Promise.reject("dirty") });
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Checkout"));
			await findByText("Uncommitted changes");

			mockInvokeDefaults();
			fireEvent.click(getByText("Force Switch"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("switch_branch", {
					repoPath: REPO,
					branchName: "feature/foo",
					force: true,
					stash: false,
				});
			});
		});

		it("Cancel on the dirty checkout dialog leaves the branch unswitched", async () => {
			mockInvokeDefaults({ switch_branch: () => Promise.reject("dirty") });
			const { findByText, getByText, queryByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Checkout"));
			await findByText("Uncommitted changes");

			fireEvent.click(getByText("Cancel"));
			expect(queryByText("Uncommitted changes")).toBeNull();
		});

		it("logs (not a dirty dialog) when checkout fails for a non-dirty reason", async () => {
			mockInvokeDefaults({ switch_branch: () => Promise.reject(new Error("network error")) });
			const { findByText, getByText, queryByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Checkout"));

			await waitFor(() => {
				expect(h.appLoggerError).toHaveBeenCalledWith("git", "Failed to switch to feature/foo", expect.any(Error));
			});
			expect(queryByText("Uncommitted changes")).toBeNull();
		});
	});

	describe("create branch", () => {
		it("opens the create form, fetches base refs, and creates a branch on submit", async () => {
			mockInvokeDefaults({
				list_base_ref_options: () => [{ name: "develop", kind: "local", is_default: false }],
			});
			const { findByText, getByTitle, getByPlaceholderText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");

			fireEvent.click(getByTitle("New branch"));
			await findByText("develop");

			fireEvent.input(getByPlaceholderText("New branch name..."), { target: { value: "feature/new" } });
			fireEvent.click(getByText("Create"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("create_branch", {
					path: REPO,
					name: "feature/new",
					startPoint: null,
					checkout: true,
				});
			});
		});

		it("keeps the create form open and toasts on failure", async () => {
			mockInvokeDefaults({ create_branch: () => Promise.reject(new Error("already exists")) });
			const { findByText, getByTitle, getByPlaceholderText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");

			fireEvent.click(getByTitle("New branch"));
			fireEvent.input(getByPlaceholderText("New branch name..."), { target: { value: "main" } });
			fireEvent.click(getByText("Create"));

			await waitFor(() => {
				expect(h.toastAdd).toHaveBeenCalledWith("Create branch failed", expect.stringContaining("main"), "error", true);
			});
			expect(getByPlaceholderText("New branch name...")).toBeTruthy();
		});

		it("closes the form without creating when the name is blank", async () => {
			const { findByText, getByTitle, getByText, queryByPlaceholderText } = render(() => (
				<BranchesTab repoPath={REPO} />
			));
			await findByText("main");
			fireEvent.click(getByTitle("New branch"));
			fireEvent.click(getByText("Create"));

			expect(queryByPlaceholderText("New branch name...")).toBeNull();
			expect(h.invoke).not.toHaveBeenCalledWith("create_branch", expect.anything());
		});

		it("Cancel closes the create form", async () => {
			const { findByText, getByTitle, getByText, queryByPlaceholderText } = render(() => (
				<BranchesTab repoPath={REPO} />
			));
			await findByText("main");
			fireEvent.click(getByTitle("New branch"));
			fireEvent.click(getByText("Cancel"));
			expect(queryByPlaceholderText("New branch name...")).toBeNull();
		});
	});

	describe("delete branch", () => {
		it("refuses to start deleting the current branch", async () => {
			const { findByText, getByText, queryByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			// "main" is both current and the main branch — its context menu has no Delete item at all,
			// so exercise the guard function via a non-main current branch instead is not possible here;
			// assert the menu simply omits Delete for the current branch.
			openContextMenuFor(getByText, "main");
			expect(queryByText("Delete")).toBeNull();
		});

		it("opens a confirm dialog and deletes on confirm (git branch -d)", async () => {
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Delete"));

			await findByText(/Delete branch "feature\/foo"\?/);
			fireEvent.click(getByText("Delete", { selector: "button.primaryBtn, button" }));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("delete_branch", { path: REPO, name: "feature/foo", force: false });
			});
			await waitFor(() =>
				expect(h.toastAdd).toHaveBeenCalledWith("Branch deleted", expect.stringContaining("feature/foo"), "info"),
			);
		});

		it("Cancel on the delete dialog does not delete", async () => {
			const { findByText, getByText, queryByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Delete"));
			await findByText(/Delete branch/);
			fireEvent.click(getByText("Cancel"));
			expect(queryByText(/Delete branch/)).toBeNull();
			expect(h.invoke).not.toHaveBeenCalledWith("delete_branch", expect.anything());
		});

		it("toasts on delete failure", async () => {
			mockInvokeDefaults({ delete_branch: () => Promise.reject(new Error("not fully merged")) });
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Delete"));
			await findByText(/Delete branch/);
			fireEvent.click(getByText("Delete", { selector: "button" }));

			await waitFor(() => {
				expect(h.toastAdd).toHaveBeenCalledWith("Delete failed", expect.stringContaining("feature/foo"), "error", true);
			});
		});

		it("deletes all merged branches in bulk via the cleanup button", async () => {
			const { findByText, getByText, getByTitle } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");

			fireEvent.click(getByTitle(/Delete 1 merged branch/));
			await findByText(/Delete 1 merged branch\?/);
			fireEvent.click(getByText("Delete merged"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("delete_branch", { path: REPO, name: "feature/foo", force: false });
			});
		});
	});

	describe("rename branch", () => {
		it("renames on Enter and bumps the git revision", async () => {
			const { findByText, getByText, getByDisplayValue } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main"); // main is current, so Rename is offered directly
			openContextMenuFor(getByText, "main");
			fireEvent.click(getByText("Rename"));

			const input = getByDisplayValue("main");
			fireEvent.input(input, { target: { value: "trunk" } });
			fireEvent.keyDown(input, { key: "Enter" });

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("rename_branch", { path: REPO, oldName: "main", newName: "trunk" });
			});
		});

		it("cancels renaming on Escape without invoking rename_branch", async () => {
			const { findByText, getByText, getByDisplayValue, queryByDisplayValue } = render(() => (
				<BranchesTab repoPath={REPO} />
			));
			await findByText("main");
			openContextMenuFor(getByText, "main");
			fireEvent.click(getByText("Rename"));
			const input = getByDisplayValue("main");
			fireEvent.keyDown(input, { key: "Escape" });

			expect(queryByDisplayValue("main")).toBeNull();
			expect(h.invoke).not.toHaveBeenCalledWith("rename_branch", expect.anything());
		});

		it("logs on rename failure", async () => {
			mockInvokeDefaults({ rename_branch: () => Promise.reject(new Error("invalid name")) });
			const { findByText, getByText, getByDisplayValue } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			openContextMenuFor(getByText, "main");
			fireEvent.click(getByText("Rename"));
			const input = getByDisplayValue("main");
			fireEvent.input(input, { target: { value: "trunk" } });
			fireEvent.keyDown(input, { key: "Enter" });

			await waitFor(() => {
				expect(h.appLoggerError).toHaveBeenCalledWith("git", "Failed to rename branch main", expect.any(Error));
			});
		});
	});

	describe("merge", () => {
		it("merges via run_git_command and bumps revision on success", async () => {
			const { repositoriesStore } = await import("../../stores/repositories");
			const bumpSpy = vi.spyOn(repositoriesStore, "bumpGitRevision");
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Merge into current"));
			await findByText(/Merge "feature\/foo" into "main"\?/);
			fireEvent.click(getByText("Merge"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("run_git_command", { path: REPO, args: ["merge", "feature/foo"] });
			});
			expect(bumpSpy).toHaveBeenCalledWith(REPO);
			await waitFor(() =>
				expect(h.toastAdd).toHaveBeenCalledWith(
					"Merged",
					expect.stringContaining("feature/foo"),
					"info",
					false,
					expect.any(Object),
				),
			);
		});

		it("reports 'already up to date' without a generic merged toast", async () => {
			mockInvokeDefaults({
				run_git_command: () => ({ success: true, stdout: "Already up to date.", stderr: "", exit_code: 0 }),
			});
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Merge into current"));
			await findByText(/Merge "feature\/foo"/);
			fireEvent.click(getByText("Merge"));

			await waitFor(() => {
				expect(h.toastAdd).toHaveBeenCalledWith("Already up to date", expect.stringContaining("feature/foo"), "info");
			});
		});

		it("toasts a conflict message when the merge command reports failure", async () => {
			mockInvokeDefaults({
				run_git_command: () => ({ success: false, stdout: "", stderr: "CONFLICT in foo.ts", exit_code: 1 }),
			});
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Merge into current"));
			await findByText(/Merge "feature\/foo"/);
			fireEvent.click(getByText("Merge"));

			await waitFor(() => {
				expect(h.toastAdd).toHaveBeenCalledWith("Merge failed", expect.stringContaining("CONFLICT"), "error", true);
			});
		});

		it("toasts when the merge IPC call itself rejects", async () => {
			mockInvokeDefaults({ run_git_command: () => Promise.reject(new Error("ipc down")) });
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Merge into current"));
			await findByText(/Merge "feature\/foo"/);
			fireEvent.click(getByText("Merge"));

			await waitFor(() => {
				expect(h.toastAdd).toHaveBeenCalledWith("Merge failed", expect.stringContaining("feature/foo"), "error", true);
			});
		});

		it("Cancel on the merge dialog runs no command", async () => {
			const { findByText, getByText, queryByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Merge into current"));
			await findByText(/Merge "feature\/foo"/);
			fireEvent.click(getByText("Cancel"));
			expect(queryByText(/Merge "feature\/foo"/)).toBeNull();
			expect(h.invoke).not.toHaveBeenCalledWith(
				"run_git_command",
				expect.objectContaining({ args: ["merge", "feature/foo"] }),
			);
		});
	});

	describe("rebase", () => {
		it("rebases via run_git_command and logs success", async () => {
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Rebase onto"));
			await findByText(/Rebase current branch "main" onto "feature\/foo"\?/);
			fireEvent.click(getByText("Rebase"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("run_git_command", { path: REPO, args: ["rebase", "feature/foo"] });
			});
			await waitFor(() => expect(h.appLoggerInfo).toHaveBeenCalledWith("git", expect.stringContaining("Rebased")));
		});

		it("toasts a failure message when rebase reports a conflict", async () => {
			mockInvokeDefaults({
				run_git_command: () => ({ success: false, stdout: "", stderr: "CONFLICT", exit_code: 1 }),
			});
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Rebase onto"));
			await findByText(/Rebase current branch/);
			fireEvent.click(getByText("Rebase"));

			await waitFor(() => {
				expect(h.toastAdd).toHaveBeenCalledWith("Rebase failed", expect.stringContaining("CONFLICT"), "error", true);
			});
		});
	});

	describe("push / pull / fetch", () => {
		it("pushes with -u origin when there is no upstream", async () => {
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Push"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("run_git_command", {
					path: REPO,
					args: ["push", "-u", "origin", "feature/foo"],
				});
			});
		});

		it("pushes plainly when an upstream is already set", async () => {
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			openContextMenuFor(getByText, "main");
			fireEvent.click(getByText("Push"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("run_git_command", { path: REPO, args: ["push"] });
			});
		});

		it("pulls only for the current branch", async () => {
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			openContextMenuFor(getByText, "main");
			fireEvent.click(getByText("Pull"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("run_git_command", { path: REPO, args: ["pull"] });
			});
		});

		it("fetches a given branch", async () => {
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("origin/feature/bar");
			openContextMenuFor(getByText, "origin/feature/bar");
			fireEvent.click(getByText("Fetch"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("run_git_command", {
					path: REPO,
					args: ["fetch", "origin", "origin/feature/bar"],
				});
			});
		});

		it("update from base runs a rebase strategy and logs the result", async () => {
			mockInvokeDefaults({ update_from_base: () => "Rebased onto main" });
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			openContextMenuFor(getByText, "main");
			fireEvent.click(getByText("Update from base (rebase)"));

			await waitFor(() => {
				expect(h.invoke).toHaveBeenCalledWith("update_from_base", {
					path: REPO,
					branchName: "main",
					strategy: "rebase",
				});
			});
			await waitFor(() => expect(h.appLoggerInfo).toHaveBeenCalledWith("git", "Rebased onto main"));
		});
	});

	describe("compare", () => {
		it("logs the number of differing files when comparing with the current branch", async () => {
			mockInvokeDefaults({
				run_git_command: () => ({ success: true, stdout: "M\tfoo.ts\nA\tbar.ts\n", stderr: "" }),
			});
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Compare with current"));

			await waitFor(() => {
				expect(h.appLoggerInfo).toHaveBeenCalledWith(
					"git",
					expect.stringContaining("2 file(s) differ"),
					expect.objectContaining({ files: ["M\tfoo.ts", "A\tbar.ts"] }),
				);
			});
		});

		it("logs 'no differences' when the diff is empty", async () => {
			mockInvokeDefaults({ run_git_command: () => ({ success: true, stdout: "", stderr: "" }) });
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Compare with current"));

			await waitFor(() => {
				expect(h.appLoggerInfo).toHaveBeenCalledWith("git", expect.stringContaining("no differences"));
			});
		});
	});

	describe("open on GitHub", () => {
		it("opens the branch's GitHub tree URL, stripping the remote prefix", async () => {
			mockInvokeDefaults({ get_remote_url: () => "git@github.com:acme/widgets.git" });
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("origin/feature/bar");
			openContextMenuFor(getByText, "origin/feature/bar");
			fireEvent.click(getByText("Open on GitHub"));

			await waitFor(() => {
				expect(h.handleOpenUrl).toHaveBeenCalledWith("https://github.com/acme/widgets/tree/feature%2Fbar");
			});
		});

		it("does nothing when the remote isn't a GitHub URL", async () => {
			mockInvokeDefaults({ get_remote_url: () => "https://gitlab.com/acme/widgets.git" });
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("origin/feature/bar");
			openContextMenuFor(getByText, "origin/feature/bar");
			fireEvent.click(getByText("Open on GitHub"));

			await waitFor(() => expect(h.invoke).toHaveBeenCalledWith("get_remote_url", { path: REPO }));
			expect(h.handleOpenUrl).not.toHaveBeenCalled();
		});
	});

	describe("remoteUrlToGitHub (pure)", () => {
		it("converts an SSH remote", () => {
			expect(remoteUrlToGitHub("git@github.com:acme/widgets.git")).toBe("https://github.com/acme/widgets");
		});
		it("converts an HTTPS remote", () => {
			expect(remoteUrlToGitHub("https://github.com/acme/widgets.git")).toBe("https://github.com/acme/widgets");
		});
		it("returns null for a non-GitHub remote", () => {
			expect(remoteUrlToGitHub("https://gitlab.com/acme/widgets.git")).toBeNull();
		});
	});

	describe("copy branch name (clipboard regression coverage)", () => {
		it("copies the branch name to the clipboard on success", async () => {
			const { findByText, getByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/foo");
			openContextMenuFor(getByText, "feature/foo");
			fireEvent.click(getByText("Copy Name"));

			await waitFor(() => expect(h.writeClipboard).toHaveBeenCalledWith("feature/foo"));
			expect(h.appLoggerError).not.toHaveBeenCalled();
		});
	});

	describe("keyboard navigation", () => {
		it("ArrowDown/ArrowUp move the selected row and wrap", async () => {
			const { findByText, container } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			const root = container.querySelector('[tabindex="-1"]') as HTMLElement;
			const selectedRows = () =>
				Array.from(container.querySelectorAll<HTMLElement>('[class*="selected"]')).map((el) => el.textContent ?? "");
			const total = container.querySelectorAll('[class*="branchRow"]').length;
			expect(total).toBeGreaterThan(1);
			expect(selectedRows()).toEqual([]);

			fireEvent.keyDown(root, { key: "ArrowDown" });
			expect(selectedRows()).toHaveLength(1);
			expect(selectedRows()[0]).toContain("main");

			fireEvent.keyDown(root, { key: "ArrowDown" });
			expect(selectedRows()).toHaveLength(1);
			expect(selectedRows()[0]).toContain("feature/foo");

			fireEvent.keyDown(root, { key: "ArrowUp" });
			expect(selectedRows()[0]).toContain("main");

			// ArrowUp from the first row wraps to the last row.
			fireEvent.keyDown(root, { key: "ArrowUp" });
			expect(selectedRows()).toHaveLength(1);
			expect(selectedRows()[0]).toContain("origin/feature/bar");

			// ArrowDown from the last row wraps back to the first.
			fireEvent.keyDown(root, { key: "ArrowDown" });
			expect(selectedRows()[0]).toContain("main");
		});

		it("'/' focuses the search input", async () => {
			const { findByText, container, getByPlaceholderText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			const root = container.querySelector('[tabindex="-1"]') as HTMLElement;
			fireEvent.keyDown(root, { key: "/" });
			expect(document.activeElement).toBe(getByPlaceholderText("Filter branches... (/ to focus)"));
		});

		it("Escape clears an active search query first, then clears selection", async () => {
			const { findByText, container, getByPlaceholderText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("main");
			const search = getByPlaceholderText("Filter branches... (/ to focus)") as HTMLInputElement;
			fireEvent.input(search, { target: { value: "foo" } });
			expect(search.value).toBe("foo");

			const root = container.querySelector('[tabindex="-1"]') as HTMLElement;
			fireEvent.keyDown(root, { key: "Escape" });
			expect((getByPlaceholderText("Filter branches... (/ to focus)") as HTMLInputElement).value).toBe("");
		});
	});

	describe("prefix grouping", () => {
		it("groups branches sharing a prefix under a collapsible header, toggled by the folder button", async () => {
			mockInvokeDefaults({
				get_branches_detail: () => [
					MAIN,
					{ ...FEATURE, name: "feature/a" },
					{ ...FEATURE, name: "feature/b", is_merged: false },
				],
				get_recent_branches: () => [],
			});
			const { findByText, getByText, queryByText } = render(() => <BranchesTab repoPath={REPO} />);
			await findByText("feature/");

			expect(getByText("feature/a")).toBeTruthy();
			fireEvent.click(getByText("feature/"));
			expect(queryByText("feature/a")).toBeNull();
		});
	});
});
