import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastContainer } from "../../components/ToastContainer/ToastContainer";
import { appLogger } from "../../stores/appLogger";
import { progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";

/**
 * An agent's `ui action=toast` names the repo it came from, but a repo holds
 * many tabs. The toast carries the originating TUIC session id so the explicit
 * Go to repo button can land on the exact terminal. Tapping the body dismisses.
 */
describe("ToastContainer", () => {
	beforeEach(() => {
		vi.restoreAllMocks();
		vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		vi.spyOn(appLogger, "info").mockImplementation(() => {});
		vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		vi.useFakeTimers();
		progressStore.resetForTests();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	});

	afterEach(() => {
		cleanup();
		vi.runOnlyPendingTimers();
		vi.useRealTimers();
	});

	function addTerminal(sessionId: string): string {
		return terminalsStore.add({
			sessionId,
			fontSize: 12,
			name: sessionId,
			nameIsCustom: false,
			cwd: null,
			awaitingInput: null,
			agentType: null,
			ptyDescription: null,
		});
	}

	it("keeps the current terminal when dismissing another terminal's toast", () => {
		const other = addTerminal("session-other");
		addTerminal("session-origin");
		terminalsStore.setActive(other);

		toastsStore.add("done", "tuicommander · built", "info", false, undefined, undefined, undefined, "session-origin");
		render(() => <ToastContainer />);
		fireEvent.click(screen.getByText("done"));

		expect(terminalsStore.state.activeId).toBe(other);
		expect(toastsStore.toasts).toHaveLength(0);
	});

	it("keeps the current repo and tab strip when dismissing a toast from another repo", () => {
		repositoriesStore.add({ path: "/here", displayName: "Here" });
		repositoriesStore.setWorkspace("/here", "main", { worktreePath: "/here" });
		repositoriesStore.add({ path: "/there", displayName: "There" });
		repositoriesStore.setWorkspace("/there", "feature", { worktreePath: "/there" });
		const here = addTerminal("session-here");
		const there = addTerminal("session-there");
		repositoriesStore.addTerminalToWorkspace("/here", "main", here);
		repositoriesStore.addTerminalToWorkspace("/there", "feature", there);
		repositoriesStore.setActive("/here");
		terminalsStore.setActive(here);

		toastsStore.add("built", "ok", "info", false, undefined, undefined, "/there", "session-there");
		render(() => <ToastContainer />);
		fireEvent.click(screen.getByText("built"));

		// Focusing the terminal without its repo left the sidebar and the tab strip
		// on the old repo while the pane drew the new terminal — no tab for it.
		expect(terminalsStore.state.activeId).toBe(here);
		expect(repositoriesStore.state.activeRepoPath).toBe("/here");
	});

	it("offers a repo action when the registered origin differs and focuses its live session", () => {
		repositoriesStore.add({ path: "/toast-current", displayName: "Current" });
		repositoriesStore.setWorkspace("/toast-current", "main", { worktreePath: "/toast-current" });
		repositoriesStore.add({ path: "/toast-origin", displayName: "Origin" });
		repositoriesStore.setWorkspace("/toast-origin", "feature", { worktreePath: "/toast-origin" });
		const current = addTerminal("session-current-action");
		const origin = addTerminal("session-origin-action");
		repositoriesStore.addTerminalToWorkspace("/toast-current", "main", current);
		repositoriesStore.addTerminalToWorkspace("/toast-origin", "feature", origin);
		repositoriesStore.setActive("/toast-current");
		terminalsStore.setActive(current);

		toastsStore.add(
			"ready",
			"review it",
			"info",
			false,
			undefined,
			undefined,
			"/toast-origin",
			"session-origin-action",
		);
		render(() => <ToastContainer />);
		const repoAction = screen.getByRole("button", { name: "Go to repo" });
		expect(repoAction.tabIndex).toBe(0);
		fireEvent.click(repoAction);

		expect(repositoriesStore.state.activeRepoPath).toBe("/toast-origin");
		expect(terminalsStore.state.activeId).toBe(origin);
		expect(toastsStore.toasts).toHaveLength(0);
	});

	it("opens the reporting terminal in its worktree from a progress toast", () => {
		repositoriesStore.add({ path: "/progress-current", displayName: "Current" });
		repositoriesStore.setWorkspace("/progress-current", "main", { worktreePath: "/progress-current" });
		repositoriesStore.add({ path: "/progress-repo", displayName: "Progress" });
		repositoriesStore.setWorkspace("/progress-repo", "main", { worktreePath: "/progress-repo" });
		repositoriesStore.setWorkspace("/progress-repo", "feature", { worktreePath: "/progress-worktree" });
		const current = addTerminal("pty-current");
		const origin = addTerminal("pty-reporting");
		repositoriesStore.addTerminalToWorkspace("/progress-current", "main", current);
		repositoriesStore.addTerminalToWorkspace("/progress-repo", "feature", origin);
		repositoriesStore.setActive("/progress-current");
		terminalsStore.setActive(current);

		progressStore.presentLive({
			repo_path: "/progress-repo",
			payload: {
				entry: {
					id: 1,
					project: "/progress-repo",
					ptyId: "pty-reporting",
					createdAtMs: 1,
					type: "done",
					text: "Build complete",
					step: "Build",
				},
			},
		});
		render(() => <ToastContainer />);
		fireEvent.click(screen.getByRole("button", { name: "Go to repo" }));

		expect(repositoriesStore.state.activeRepoPath).toBe("/progress-repo");
		expect(repositoriesStore.get("/progress-repo")?.activeWorkspaceId).toBe("feature");
		expect(terminalsStore.state.activeId).toBe(origin);
	});

	it("dismisses a Progress toast body without switching to the reporting repository", () => {
		repositoriesStore.add({ path: "/progress-dismiss-current", displayName: "Current" });
		repositoriesStore.add({ path: "/progress-dismiss-origin", displayName: "Origin" });
		repositoriesStore.setWorkspace("/progress-dismiss-current", "main", { worktreePath: "/progress-dismiss-current" });
		repositoriesStore.setWorkspace("/progress-dismiss-origin", "main", { worktreePath: "/progress-dismiss-origin" });
		const current = addTerminal("pty-dismiss-current");
		const origin = addTerminal("pty-dismiss-origin");
		repositoriesStore.addTerminalToWorkspace("/progress-dismiss-current", "main", current);
		repositoriesStore.addTerminalToWorkspace("/progress-dismiss-origin", "main", origin);
		repositoriesStore.setActive("/progress-dismiss-current");
		terminalsStore.setActive(current);

		progressStore.presentLive({
			repo_path: "/progress-dismiss-origin",
			payload: {
				entry: {
					id: 17,
					project: "/progress-dismiss-origin",
					ptyId: "pty-dismiss-origin",
					createdAtMs: 17,
					type: "done",
					text: "Build complete",
				},
			},
		});
		render(() => <ToastContainer />);
		fireEvent.click(screen.getByText("Build complete"));

		expect(toastsStore.toasts).toHaveLength(0);
		expect(repositoriesStore.state.activeRepoPath).toBe("/progress-dismiss-current");
		expect(terminalsStore.state.activeId).toBe(current);
	});

	it("lands in the repository and logs when a progress terminal has closed", () => {
		repositoriesStore.add({ path: "/progress-before", displayName: "Before" });
		repositoriesStore.add({ path: "/progress-gone", displayName: "Gone" });
		repositoriesStore.setActive("/progress-before");
		const current = addTerminal("pty-still-open");
		terminalsStore.setActive(current);
		const closed = addTerminal("pty-closed");
		terminalsStore.remove(closed);
		const warn = vi.spyOn(appLogger, "warn");
		progressStore.presentLive({
			repo_path: "/progress-gone",
			payload: {
				entry: {
					id: 2,
					project: "/progress-gone",
					ptyId: "pty-closed",
					createdAtMs: 2,
					type: "blocked",
					text: "Review needed",
					step: "Review",
				},
			},
		});
		render(() => <ToastContainer />);
		fireEvent.click(screen.getByRole("button", { name: "Go to repo" }));

		expect(repositoriesStore.state.activeRepoPath).toBe("/progress-gone");
		expect(terminalsStore.state.activeId).toBeNull();
		expect(warn).toHaveBeenCalledWith(
			"app",
			expect.stringContaining("terminal"),
			expect.objectContaining({ sessionId: "pty-closed" }),
		);
	});

	it("places the repo action before the primary toast action", () => {
		repositoriesStore.add({ path: "/toast-current-order", displayName: "Current" });
		repositoriesStore.add({ path: "/toast-origin-order", displayName: "Origin" });
		repositoriesStore.setActive("/toast-current-order");
		const openProgress = vi.fn();
		toastsStore.add(
			"verified",
			"ready to review",
			"info",
			false,
			{ label: "Open Progress", onClick: openProgress },
			undefined,
			"/toast-origin-order",
		);

		const { container } = render(() => <ToastContainer />);
		const buttons = Array.from(container.querySelectorAll(".actions button"));

		// Dialog footers put the secondary action before the primary one, so DOM
		// and keyboard focus order match the visual left-to-right order.
		expect(buttons.map((button) => button.textContent)).toEqual(["Go to repo", "Open Progress"]);

		// Each handler moved with its button: the primary one runs, the repository stays.
		fireEvent.click(buttons[1]);
		expect(openProgress).toHaveBeenCalledOnce();
		expect(repositoriesStore.state.activeRepoPath).toBe("/toast-current-order");
	});

	it("switches a plain UI toast's repository without claiming a missing terminal", () => {
		repositoriesStore.add({ path: "/plain-before", displayName: "Before" });
		repositoriesStore.add({ path: "/plain-origin", displayName: "Origin" });
		repositoriesStore.setActive("/plain-before");
		const warn = vi.spyOn(appLogger, "warn");
		toastsStore.add("Plain notice", "No PTY binding", "info", false, undefined, undefined, "/plain-origin");

		render(() => <ToastContainer />);
		fireEvent.click(screen.getByRole("button", { name: "Go to repo" }));

		expect(repositoriesStore.state.activeRepoPath).toBe("/plain-origin");
		expect(warn).not.toHaveBeenCalled();
	});

	it("does not offer a repo action for the active repository", () => {
		repositoriesStore.add({ path: "/toast-active", displayName: "Active" });
		repositoriesStore.setActive("/toast-active");
		toastsStore.add("local", "already here", "info", false, undefined, undefined, "/toast-active");

		render(() => <ToastContainer />);

		expect(screen.queryByRole("button", { name: "Go to repo" })).toBeNull();
	});

	it("does not offer a repo action after the origin repository is removed", () => {
		repositoriesStore.add({ path: "/toast-present", displayName: "Present" });
		repositoriesStore.add({ path: "/toast-removed", displayName: "Removed" });
		repositoriesStore.setActive("/toast-present");
		toastsStore.add("stale repo", "gone", "warn", false, undefined, undefined, "/toast-removed");

		render(() => <ToastContainer />);
		expect(screen.getByRole("button", { name: "Go to repo" })).toBeTruthy();
		repositoriesStore.remove("/toast-removed");

		expect(screen.queryByRole("button", { name: "Go to repo" })).toBeNull();
	});

	it("keeps the toast when its repo disappears before the repo action runs", () => {
		repositoriesStore.add({ path: "/toast-present-click", displayName: "Present" });
		repositoriesStore.add({ path: "/toast-removed-click", displayName: "Removed" });
		repositoriesStore.setActive("/toast-present-click");
		toastsStore.add("stale repo", "gone", "warn", false, undefined, undefined, "/toast-removed-click");

		const originalGet = repositoriesStore.get;
		let repoVanished = false;
		vi.spyOn(repositoriesStore, "get").mockImplementation((path) =>
			path === "/toast-removed-click" && repoVanished ? undefined : originalGet(path),
		);
		render(() => <ToastContainer />);
		const repoAction = screen.getByRole("button", { name: "Go to repo" });
		repoVanished = true;
		fireEvent.click(repoAction);

		expect(toastsStore.toasts).toHaveLength(1);
	});

	it("dismisses the toast after its own action even when that action returns false", () => {
		// Only the repo action reports a failed navigation. A toast's own action is
		// typed () => void, so whatever it happens to return must not keep the toast.
		const returnsFalse = vi.fn(() => false);
		toastsStore.add("done", "open it", "info", false, { label: "Open", onClick: returnsFalse });

		render(() => <ToastContainer />);
		fireEvent.click(screen.getByRole("button", { name: "Open" }));

		expect(returnsFalse).toHaveBeenCalledOnce();
		expect(toastsStore.toasts).toHaveLength(0);
	});

	it("keeps the repo action node while unrelated repository state changes", () => {
		repositoriesStore.add({ path: "/toast-current-stable", displayName: "Current" });
		repositoriesStore.add({ path: "/toast-other-stable", displayName: "Other" });
		repositoriesStore.add({ path: "/toast-origin-stable", displayName: "Origin" });
		repositoriesStore.setActive("/toast-current-stable");
		toastsStore.add("ready", "review it", "info", false, undefined, undefined, "/toast-origin-stable");

		render(() => <ToastContainer />);
		const repoAction = screen.getByRole("button", { name: "Go to repo" });
		repoAction.focus();
		repositoriesStore.setActive("/toast-other-stable");

		expect(screen.getByRole("button", { name: "Go to repo" })).toBe(repoAction);
		expect(document.activeElement).toBe(repoAction);
	});

	it("names the repo the toast came from", () => {
		repositoriesStore.add({ path: "/Gits/personal/ego", displayName: "Ego" });

		toastsStore.add("built", "ok", "info", false, undefined, undefined, "/Gits/personal/ego", undefined);
		render(() => <ToastContainer />);

		expect(screen.getByText("ego")).toBeTruthy();
	});

	it("falls back to the speaking terminal's repo when the origin resolved to none", () => {
		repositoriesStore.add({ path: "/Gits/personal/mdkb", displayName: "Mdkb" });
		repositoriesStore.setWorkspace("/Gits/personal/mdkb", "main", { worktreePath: "/Gits/personal/mdkb" });
		const speaker = addTerminal("session-speaker");
		repositoriesStore.addTerminalToWorkspace("/Gits/personal/mdkb", "main", speaker);

		toastsStore.add("built", "ok", "info", false, undefined, undefined, undefined, "session-speaker");
		render(() => <ToastContainer />);

		expect(screen.getByText("mdkb")).toBeTruthy();
	});

	it("only dismisses when the toast carries no session", () => {
		const only = addTerminal("session-only");
		terminalsStore.setActive(only);

		toastsStore.add("plain", "no origin", "info");
		render(() => <ToastContainer />);
		fireEvent.click(screen.getByText("plain"));

		expect(terminalsStore.state.activeId).toBe(only);
		expect(toastsStore.toasts).toHaveLength(0);
	});

	it("only dismisses when the originating session is already gone", () => {
		const survivor = addTerminal("session-survivor");
		terminalsStore.setActive(survivor);

		toastsStore.add("stale", "closed since", "warn", false, undefined, undefined, undefined, "session-closed");
		render(() => <ToastContainer />);
		fireEvent.click(screen.getByText("stale"));

		// A missing origin cannot make dismiss navigate or warn.
		expect(terminalsStore.state.activeId).toBe(survivor);
		expect(toastsStore.toasts).toHaveLength(0);
	});
});
