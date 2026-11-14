import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getModifierSymbol } from "../../platform";

// Mock IdeLauncher to avoid Tauri invoke calls from that component
vi.mock("../../components/IdeLauncher", () => ({
	IdeLauncher: (props: Record<string, unknown>) => (
		<div
			data-testid="ide-launcher"
			data-repo-path={(props.repoPath as string) || ""}
			data-cursor-line={props.cursorLine === undefined ? "" : String(props.cursorLine)}
			data-cursor-col={props.cursorCol === undefined ? "" : String(props.cursorCol)}
		/>
	),
}));

// Mock PrDetailPopover to avoid needing GitHub store setup
vi.mock("../../components/PrDetailPopover/PrDetailPopover", () => ({
	PrDetailPopover: (props: Record<string, unknown>) => (
		<div class="pr-detail-popover" data-repo={props.repoPath as string} data-branch={props.branch as string} />
	),
}));

// Mock Tauri core (for any transitive imports)
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({
		listen: vi.fn().mockResolvedValue(vi.fn()),
		setTitle: vi.fn().mockResolvedValue(undefined),
	})),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
	open: vi.fn().mockResolvedValue(null),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
	openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { CommandPalette } from "../../components/CommandPalette/CommandPalette";
import { Toolbar } from "../../components/Toolbar/Toolbar";
import { activityStore } from "../../stores/activityStore";
import { commandPaletteStore } from "../../stores/commandPalette";
import { editorTabsStore } from "../../stores/editorTabs";
import { githubStore } from "../../stores/github";
import { prNotificationsStore } from "../../stores/prNotifications";
import { progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import { makeTerminal } from "../helpers/store";

function setTauriEnv(on: boolean) {
	const global = globalThis as Record<string, unknown>;
	if (on) {
		global.__TAURI_INTERNALS__ = {};
		delete global.__TAURI_SHIM__;
	} else {
		delete global.__TAURI_INTERNALS__;
	}
}

describe("Toolbar", () => {
	beforeEach(() => {
		setTauriEnv(true);
		localStorage.clear();
		commandPaletteStore.close();
		for (const path of repositoriesStore.getPaths()) {
			repositoriesStore.remove(path);
		}
		repositoriesStore.setActive(null);
		uiStore.setSidebarVisible(true);
		prNotificationsStore.clearAll();
		activityStore.clearAll();
		progressStore.resetForTests();
		editorTabsStore.clearAll();
	});

	afterEach(() => {
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		setTauriEnv(true);
		commandPaletteStore.close();
		prNotificationsStore._testCancelPendingTimers();
		repositoriesStore._testCancelPendingSave();
		uiStore._testCancelPendingSave();
	});

	function addTestNotif(overrides: Partial<Parameters<typeof prNotificationsStore.add>[0]> = {}) {
		prNotificationsStore.add({
			repoPath: "/repo",
			branch: "feature",
			prNumber: 42,
			title: "Test PR",
			type: "ci_failed",
			...overrides,
		});
	}

	function addTestActivityItem(
		overrides: Partial<{ id: string; title: string; subtitle: string; sectionId: string; contentUri: string }> = {},
	) {
		activityStore.addItem({
			id: overrides.id ?? "act-1",
			pluginId: "test",
			sectionId: overrides.sectionId ?? "plan",
			title: overrides.title ?? "Test Item",
			icon: "<svg/>",
			dismissible: true,
			subtitle: overrides.subtitle,
			contentUri: overrides.contentUri,
		});
	}

	it("renders toolbar element", () => {
		const { container } = render(() => <Toolbar />);
		expect(container.querySelector(".toolbar")).not.toBeNull();
	});

	// Catches: history arrows enabled at the ends of the history, or a tooltip that does not name the target terminal.
	it("history arrows are disabled at the ends and their tooltip names the target terminal", () => {
		const first = terminalsStore.add(makeTerminal({ name: "Alpha" }));
		const second = terminalsStore.add(makeTerminal({ name: "Beta" }));
		try {
			terminalsStore.setActive(first);
			terminalsStore.setActive(second);
			const { container } = render(() => <Toolbar />);
			const back = container.querySelector("[data-testid='history-back']") as HTMLButtonElement;
			const forward = container.querySelector("[data-testid='history-forward']") as HTMLButtonElement;
			expect(back.disabled).toBe(false);
			expect(back.title).toContain("Alpha");
			expect(forward.disabled).toBe(true);
			fireEvent.click(back);
			expect(back.disabled).toBe(true);
			expect(forward.disabled).toBe(false);
			expect(forward.title).toContain("Beta");
		} finally {
			terminalsStore.remove(first);
			terminalsStore.remove(second);
		}
	});

	// Catches: the sidebar layout mode readable only through title/tooltip, which touch never shows.
	it("cycles R, C, H without a separate harness button — catches Auto or duplicate H controls", () => {
		const { container } = render(() => <Toolbar />);
		const label = () => container.querySelector("[data-testid='sidebar-density-label']")?.textContent;
		const toggle = container.querySelector("[data-testid='sidebar-density-toggle']") as HTMLElement;
		const richRowGlyph = toggle.querySelector("svg path")?.getAttribute("d");
		for (const [letter, harness] of [
			["R", false],
			["C", false],
			["H", true],
			["R", false],
		] as const) {
			expect(label()).toBe(letter);
			expect(uiStore.state.repoFilterHarnessOnly).toBe(harness);
			if (harness) {
				// Catches: an H-shaped glyph duplicating the visible H label.
				expect(toggle.querySelector("svg path")?.getAttribute("d")).toBe(richRowGlyph);
			}
			fireEvent.click(toggle);
		}
		// The final click leaves compact; restore rich for other toolbar cases.
		uiStore.cycleSidebarDensityMode();
		uiStore.cycleSidebarDensityMode();
		expect(container.querySelector("[data-testid='toolbar-filter-harness']")).toBeNull();
		expect(container.querySelector("button[title*='open terminals']")).not.toBeNull();
	});

	it("renders sidebar toggle button", () => {
		const { container } = render(() => <Toolbar />);
		const toggle = container.querySelector(".sidebarToggle");
		expect(toggle).not.toBeNull();
	});

	it("renders left, center, and right sections", () => {
		const { container } = render(() => <Toolbar />);
		expect(container.querySelector(".left")).not.toBeNull();
		expect(container.querySelector(".center")).not.toBeNull();
		expect(container.querySelector(".right")).not.toBeNull();
	});

	it("browser toolbar button opens, closes, and reopens a focused palette", async () => {
		setTauriEnv(false);
		const actions = [
			{
				id: "search-files",
				label: "Search Files",
				category: "Search",
				keybinding: "",
				execute: vi.fn(),
			},
		];
		const { container } = render(() => (
			<>
				<Toolbar />
				<CommandPalette actions={actions} browserMode />
			</>
		));

		const button = container.querySelector<HTMLButtonElement>('button[aria-controls="command-palette"]');
		expect(button).not.toBeNull();
		expect(button?.type).toBe("button");
		expect(button?.getAttribute("aria-expanded")).toBe("false");

		fireEvent.click(button!);
		await waitFor(() => {
			expect(container.querySelector('[role="dialog"][aria-label="Command palette"]')).not.toBeNull();
			expect(document.activeElement).toBe(container.querySelector('[aria-label="Command palette search"]'));
		});
		expect(button?.getAttribute("aria-expanded")).toBe("true");

		fireEvent.click(container.querySelector<HTMLElement>(".overlayTop")!);
		await waitFor(() => expect(container.querySelector('[role="dialog"]')).toBeNull());
		expect(button?.getAttribute("aria-expanded")).toBe("false");

		fireEvent.click(button!);
		await waitFor(() => {
			expect(container.querySelector('[role="dialog"]')).not.toBeNull();
			expect(document.activeElement).toBe(container.querySelector('[aria-label="Command palette search"]'));
		});
	});

	it("has data-tauri-drag-region attribute", () => {
		const { container } = render(() => <Toolbar />);
		const toolbar = container.querySelector(".toolbar");
		expect(toolbar!.getAttribute("data-tauri-drag-region")).not.toBeNull();
	});

	it("sidebar toggle button click calls uiStore.toggleSidebar", () => {
		const spy = vi.spyOn(uiStore, "toggleSidebar");
		const { container } = render(() => <Toolbar />);
		const toggle = container.querySelector(".sidebarToggle")!;
		fireEvent.click(toggle);
		expect(spy).toHaveBeenCalledOnce();
		spy.mockRestore();
	});

	it("sidebar toggle title reflects sidebar visible state", () => {
		uiStore.setSidebarVisible(true);
		const { container } = render(() => <Toolbar />);
		const toggle = container.querySelector(".sidebarToggle")!;
		expect(toggle.getAttribute("title")).toBe(`Hide Sidebar (${getModifierSymbol()}[)`);
	});

	it("sidebar toggle title when sidebar hidden", () => {
		uiStore.setSidebarVisible(false);
		const { container } = render(() => <Toolbar />);
		const toggle = container.querySelector(".sidebarToggle")!;
		expect(toggle.getAttribute("title")).toBe(`Show Sidebar (${getModifierSymbol()}[)`);
	});

	it("shows branch name when active repo has active branch", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		repositoriesStore.setWorkspace(repoPath, "feature-x", { branchName: "feature-x" });
		repositoriesStore.setActiveWorkspace(repoPath, "feature-x");

		const { container } = render(() => <Toolbar />);
		const branchBtn = container.querySelector(".branch");
		expect(branchBtn).not.toBeNull();
		const branchName = container.querySelector(".branchName");
		expect(branchName!.textContent).toBe("feature-x");
	});

	it("branch button calls onBranchClick when clicked", () => {
		const handleBranchClick = vi.fn();
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		repositoriesStore.setWorkspace(repoPath, "main", { branchName: "main" });
		repositoriesStore.setActiveWorkspace(repoPath, "main");

		const { container } = render(() => <Toolbar onBranchClick={handleBranchClick} />);
		const branchBtn = container.querySelector(".branch")!;
		fireEvent.click(branchBtn);
		expect(handleBranchClick).toHaveBeenCalledOnce();
	});

	it("no branch button when no active branch", () => {
		const { container } = render(() => <Toolbar />);
		expect(container.querySelector(".branch")).toBeNull();
	});

	it("no branch button when activeRepoPath but no activeBranch", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);

		const { container } = render(() => <Toolbar />);
		expect(container.querySelector(".branch")).toBeNull();
	});

	it("no branch button when activeRepoPath and activeBranch not in branches record", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		repositoriesStore.setActiveWorkspace(repoPath, "nonexistent");

		const { container } = render(() => <Toolbar />);
		expect(container.querySelector(".branch")).toBeNull();
	});

	it("branch button has title 'Rename branch'", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		repositoriesStore.setWorkspace(repoPath, "main", { branchName: "main" });
		repositoriesStore.setActiveWorkspace(repoPath, "main");

		const { container } = render(() => <Toolbar />);
		const branchBtn = container.querySelector(".branch")!;
		expect(branchBtn.getAttribute("title")).toBe("Rename branch");
	});

	it("branch button contains branch icon", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		repositoriesStore.setWorkspace(repoPath, "dev", { branchName: "dev" });
		repositoriesStore.setActiveWorkspace(repoPath, "dev");

		const { container } = render(() => <Toolbar />);
		const icon = container.querySelector(".branchIcon");
		expect(icon).not.toBeNull();
		expect(icon!.tagName.toLowerCase()).toBe("svg");
	});

	it("IdeLauncher receives worktreePath from active branch", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		repositoriesStore.setWorkspace(repoPath, "feature", {
			branchName: "feature",
			worktreePath: "/test/repo-worktrees/feature",
		});
		repositoriesStore.setActiveWorkspace(repoPath, "feature");

		const { container } = render(() => <Toolbar repoPath={repoPath} />);
		const launcher = container.querySelector("[data-testid='ide-launcher']")!;
		expect(launcher.getAttribute("data-repo-path")).toBe("/test/repo-worktrees/feature");
	});

	it("IdeLauncher falls back to repoPath when no worktreePath", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		repositoriesStore.setWorkspace(repoPath, "main", {
			branchName: "main",
			worktreePath: null,
		});
		repositoriesStore.setActiveWorkspace(repoPath, "main");

		const { container } = render(() => <Toolbar repoPath={repoPath} />);
		const launcher = container.querySelector("[data-testid='ide-launcher']")!;
		expect(launcher.getAttribute("data-repo-path")).toBe(repoPath);
	});

	it("IdeLauncher receives repoPath when no active branch", () => {
		const repoPath = "/test/repo";

		const { container } = render(() => <Toolbar repoPath={repoPath} />);
		const launcher = container.querySelector("[data-testid='ide-launcher']")!;
		expect(launcher.getAttribute("data-repo-path")).toBe(repoPath);
	});

	it("IdeLauncher receives the active editor tab's cursor position", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		const id = editorTabsStore.add(repoPath, "src/main.ts");
		editorTabsStore.setActive(id);
		editorTabsStore.setCursor(id, 42, 7);

		const { container } = render(() => <Toolbar repoPath={repoPath} />);
		const launcher = container.querySelector("[data-testid='ide-launcher']")!;
		expect(launcher.getAttribute("data-cursor-line")).toBe("42");
		expect(launcher.getAttribute("data-cursor-col")).toBe("7");
	});

	// -------------------------------------------------------------------------
	// Plan button is REMOVED (now plan items go through Activity Center)
	// -------------------------------------------------------------------------
	it("plan button is not rendered (plan moved to Activity Center)", () => {
		const { container } = render(() => <Toolbar />);
		expect(container.querySelector(".plan-button")).toBeNull();
		expect(container.querySelector(".plan-button-group")).toBeNull();
	});

	// -------------------------------------------------------------------------
	// Bell (Activity Center) — PR notifications
	// -------------------------------------------------------------------------
	describe("PR notification bell", () => {
		it("keeps Progress available with no unread entries and opens the active repository", () => {
			repositoriesStore.add({ path: "/repo/progress", displayName: "Progress Repo" });
			repositoriesStore.setActive("/repo/progress");
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".notifCount")).toBeNull();
			fireEvent.click(container.querySelector(".bell")!);
			const item = Array.from(container.querySelectorAll(".activityItem")).find((node) =>
				node.textContent?.includes("Terminal Progress"),
			);
			expect(item).toBeDefined();
			expect(item?.textContent).toContain("Open project journal");
			fireEvent.click(item!);
			expect(progressStore.dialogVisible()).toBe(true);
			expect(progressStore.requestedProject()).toBe("/repo/progress");
		});

		it("keeps the unread badge and count when Progress is available", () => {
			progressStore.presentLive({
				repo_path: "/repo",
				payload: {
					entry: { id: 1, project: "/repo", createdAtMs: 1, type: "done", text: "Starting" },
				},
			});
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".notifCount")?.textContent).toBe("1");
			fireEvent.click(container.querySelector(".bell")!);
			expect(container.querySelector(".activityItemSubtitle")?.textContent).toBe("1 new update");
			progressStore.presentLive({
				repo_path: "/repo",
				payload: {
					entry: { id: 2, project: "/repo", createdAtMs: 2, type: "done", text: "Continuing" },
				},
			});
			expect(container.querySelector(".notifCount")?.textContent).toBe("2");
			expect(container.querySelector(".activityItemSubtitle")?.textContent).toBe("2 new updates");
		});
		// Catches: static plugin launchers contribute an impossible-to-dismiss badge.
		it("keeps a status-only bell unbadged while its plugin row remains visible", () => {
			activityStore.registerSection({ id: "status", label: "STATUS", priority: 10, canDismissAll: false });
			activityStore.addItem({
				id: "status",
				pluginId: "test",
				sectionId: "status",
				title: "Plugin status",
				icon: "<svg/>",
				dismissible: false,
			});
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".notifCount")).toBeNull();
			fireEvent.click(container.querySelector(".bell")!);
			expect(container.textContent).toContain("Plugin status");
			expect(container.textContent).toContain("Open project journal");
			addTestNotif();
			expect(container.querySelector(".notifCount")?.textContent).toBe("1");
			prNotificationsStore.dismissAll();
			expect(container.querySelector(".notifCount")).toBeNull();
		});

		// Catches: Progress unread counts have no explicit acknowledgement without opening the journal.
		it("marks Progress read explicitly without clearing other notifications or opening the journal", () => {
			progressStore.presentLive({
				repo_path: "/repo",
				payload: { entry: { id: 1, project: "/repo", createdAtMs: 1, type: "done", text: "Finished" } },
			});
			addTestNotif();
			const { container, getByRole, queryByRole } = render(() => <Toolbar />);
			expect(container.querySelector(".notifCount")?.textContent).toBe("2");
			fireEvent.click(container.querySelector(".bell")!);
			fireEvent.click(getByRole("button", { name: "Mark as read" }));
			expect(progressStore.unreadCount).toBe(0);
			expect(progressStore.dialogVisible()).toBe(false);
			expect(container.querySelector(".notifCount")?.textContent).toBe("1");
			expect(container.textContent).toContain("Open project journal");
			expect(queryByRole("button", { name: "Mark as read" })).toBeNull();
			progressStore.presentLive({
				repo_path: "/repo",
				payload: { entry: { id: 2, project: "/repo", createdAtMs: 2, type: "done", text: "Next" } },
			});
			expect(container.querySelector(".notifCount")?.textContent).toBe("2");
		});

		it("bell is always visible even with no notifications", () => {
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".bell")).not.toBeNull();
		});

		it("badge is hidden when no active notifications or activity items", () => {
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".notifCount")).toBeNull();
		});

		it("badge is visible when there are active PR notifications", () => {
			addTestNotif();
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".notifCount")).not.toBeNull();
		});

		it("badge is visible when there are active activity items", () => {
			activityStore.registerSection({ id: "plan", label: "PLAN", priority: 10, canDismissAll: false });
			addTestActivityItem();
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".notifCount")).not.toBeNull();
		});

		it("bell badge count is sum of PR + activity items", () => {
			activityStore.registerSection({ id: "plan", label: "PLAN", priority: 10, canDismissAll: false });
			addTestNotif();
			addTestActivityItem({ id: "a1" });
			addTestActivityItem({ id: "a2" });
			const { container } = render(() => <Toolbar />);
			const count = container.querySelector(".notifCount");
			expect(count?.textContent).toBe("3"); // 1 PR + 2 activity
		});

		it("counts the badge in one pass, without building a sorted list per section (613-00e8 F108)", () => {
			for (const id of ["plan", "build", "review"]) {
				activityStore.registerSection({ id, label: id.toUpperCase(), priority: 10, canDismissAll: false });
				addTestActivityItem({ id: `${id}-1`, sectionId: id });
				addTestActivityItem({ id: `${id}-2`, sectionId: id });
			}
			const forSection = vi.spyOn(activityStore, "getForSection");
			const { container } = render(() => <Toolbar />);

			expect(container.querySelector(".notifCount")?.textContent).toBe("6");
			// The popover is closed, so nothing legitimately needs a section's list.
			// A call here means the badge scanned and sorted the items once per section
			// only to read a `.length` off the result.
			expect(forSection).not.toHaveBeenCalled();
			forSection.mockRestore();
		});

		it("clicking bell toggles popover open", () => {
			addTestNotif();
			const { container } = render(() => <Toolbar />);
			const bell = container.querySelector(".bell")!;
			fireEvent.click(bell);
			expect(container.querySelector(".popover")).not.toBeNull();
		});

		it("clicking bell again closes popover", () => {
			addTestNotif();
			const { container } = render(() => <Toolbar />);
			const bell = container.querySelector(".bell")!;
			fireEvent.click(bell);
			fireEvent.click(bell);
			expect(container.querySelector(".popover")).toBeNull();
		});

		it("popover shows PR notification items", () => {
			addTestNotif({ type: "ci_failed", prNumber: 99 });
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			const items = container.querySelectorAll(".notifItem");
			expect(items.length).toBe(1);
			expect(items[0].textContent).toContain("#99");
		});

		it("dismiss all button dismisses all PR notifications and closes popover", () => {
			addTestNotif({ branch: "a" });
			addTestNotif({ branch: "b", prNumber: 43 });
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			fireEvent.click(container.querySelector(".dismissAll")!);
			expect(prNotificationsStore.getActive().length).toBe(0);
			expect(container.querySelector(".popover")).toBeNull();
		});

		it("individual PR dismiss button removes notification", () => {
			addTestNotif();
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			const closeBtn = container.querySelector(".notifClose")!;
			fireEvent.click(closeBtn);
			expect(prNotificationsStore.getActive().length).toBe(0);
		});

		// Catches: an old notification following a reused head branch to a different PR.
		it("opens a PR notification using its captured conversation URL without a cached branch", async () => {
			const open = vi.spyOn(window, "open").mockReturnValue(null);
			addTestNotif({ url: "https://github.com/acme/remote/pull/42" });
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			fireEvent.click(container.querySelector(".notifItem")!);
			await waitFor(() =>
				expect(invoke).toHaveBeenCalledWith("open_default_app", {
					target: "https://github.com/acme/remote/pull/42",
					kind: "url",
				}),
			);
			open.mockRestore();
		});

		it("clicking PR notification item opens PrDetailPopover", async () => {
			addTestNotif({ repoPath: "/my/repo", branch: "feature", prNumber: 42 });
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			const item = container.querySelector(".notifItem")!;
			fireEvent.click(item);

			await waitFor(() => {
				expect(container.querySelector(".pr-detail-popover")).not.toBeNull();
			});
		});
	});

	// -------------------------------------------------------------------------
	// Bell — Activity Center plugin sections
	// -------------------------------------------------------------------------
	describe("activity sections in bell popover", () => {
		beforeEach(() => {
			activityStore.registerSection({ id: "plan", label: "ACTIVE PLAN", priority: 10, canDismissAll: false });
		});

		it("shows activity section header with label", () => {
			addTestActivityItem({ title: "My Plan" });
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			const header = Array.from(container.querySelectorAll(".sectionLabel")).find(
				(node) => node.textContent === "ACTIVE PLAN",
			);
			expect(header?.textContent).toBe("ACTIVE PLAN");
		});

		it("shows activity item title", () => {
			addTestActivityItem({ title: "My Plan" });
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			const title = Array.from(container.querySelectorAll(".activityItemTitle")).find(
				(node) => node.textContent === "My Plan",
			);
			expect(title?.textContent).toBe("My Plan");
		});

		it("shows activity item subtitle when present", () => {
			addTestActivityItem({ title: "My Plan", subtitle: "/repo/plans/foo.md" });
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			const item = Array.from(container.querySelectorAll(".activityItem")).find((node) =>
				node.textContent?.includes("My Plan"),
			);
			const subtitle = item?.querySelector(".activityItemSubtitle");
			expect(subtitle?.textContent).toContain("/repo/plans/foo.md");
		});

		it("clicking dismissible activity item shows dismiss button", () => {
			addTestActivityItem();
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			expect(container.querySelector(".activityItemDismiss")).not.toBeNull();
		});

		it("clicking activity item dismiss removes it from store", () => {
			addTestActivityItem({ id: "act-x" });
			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			fireEvent.click(container.querySelector(".activityItemDismiss")!);
			expect(activityStore.getActive().find((i) => i.id === "act-x")).toBeUndefined();
		});
	});

	// -------------------------------------------------------------------------
	// Repo-scoped plan filtering
	// -------------------------------------------------------------------------
	describe("plan items filtered by active repo", () => {
		beforeEach(() => {
			activityStore.registerSection({ id: "plan", label: "ACTIVE PLAN", priority: 10, canDismissAll: false });
			repositoriesStore.add({ path: "/repo/a", displayName: "Repo A" });
			repositoriesStore.add({ path: "/repo/b", displayName: "Repo B" });
		});

		it("bell popover shows only plans matching active repo", () => {
			repositoriesStore.setActive("/repo/a");
			activityStore.addItem({
				id: "p1",
				pluginId: "plan",
				sectionId: "plan",
				title: "Plan A",
				icon: "<svg/>",
				dismissible: true,
				repoPath: "/repo/a",
			});
			activityStore.addItem({
				id: "p2",
				pluginId: "plan",
				sectionId: "plan",
				title: "Plan B",
				icon: "<svg/>",
				dismissible: true,
				repoPath: "/repo/b",
			});

			const { container } = render(() => <Toolbar />);
			fireEvent.click(container.querySelector(".bell")!);
			const titles = Array.from(container.querySelectorAll(".activityItemTitle")).map((el) => el.textContent);
			expect(titles).toContain("Plan A");
			expect(titles).not.toContain("Plan B");
		});

		it("last-item button shows plan from active repo only", () => {
			repositoriesStore.setActive("/repo/b");
			activityStore.addItem({
				id: "p1",
				pluginId: "plan",
				sectionId: "plan",
				title: "Plan A",
				icon: "<svg/>",
				dismissible: true,
				repoPath: "/repo/a",
			});
			activityStore.addItem({
				id: "p2",
				pluginId: "plan",
				sectionId: "plan",
				title: "Plan B",
				icon: "<svg/>",
				dismissible: true,
				repoPath: "/repo/b",
			});

			const { container } = render(() => <Toolbar />);
			const btn = container.querySelector(".lastItemBtn");
			expect(btn?.textContent).toContain("Plan B");
		});

		it("switching active repo updates visible plans", () => {
			repositoriesStore.setActive("/repo/a");
			activityStore.addItem({
				id: "p1",
				pluginId: "plan",
				sectionId: "plan",
				title: "Plan A",
				icon: "<svg/>",
				dismissible: true,
				repoPath: "/repo/a",
			});
			activityStore.addItem({
				id: "p2",
				pluginId: "plan",
				sectionId: "plan",
				title: "Plan B",
				icon: "<svg/>",
				dismissible: true,
				repoPath: "/repo/b",
			});

			const { container } = render(() => <Toolbar />);
			const btn1 = container.querySelector(".lastItemBtn");
			expect(btn1?.textContent).toContain("Plan A");

			repositoriesStore.setActive("/repo/b");
			const btn2 = container.querySelector(".lastItemBtn");
			expect(btn2?.textContent).toContain("Plan B");
		});
	});

	// -------------------------------------------------------------------------
	// Last-item shortcut button
	// -------------------------------------------------------------------------
	describe("last-item shortcut button", () => {
		it("hidden when no items exist", () => {
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".lastItemBtn")).toBeNull();
		});

		it("visible when an activity item exists", () => {
			activityStore.registerSection({ id: "plan", label: "PLAN", priority: 10, canDismissAll: false });
			addTestActivityItem({ title: "My Plan" });
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".lastItemBtn")).not.toBeNull();
		});

		it("visible when a PR notification exists", () => {
			addTestNotif({ branch: "my-feature" });
			const { container } = render(() => <Toolbar />);
			expect(container.querySelector(".lastItemBtn")).not.toBeNull();
		});

		it("shows activity item title in last-item button", () => {
			activityStore.registerSection({ id: "plan", label: "PLAN", priority: 10, canDismissAll: false });
			addTestActivityItem({ title: "Active Plan" });
			const { container } = render(() => <Toolbar />);
			const btn = container.querySelector(".lastItemBtn");
			expect(btn?.textContent).toContain("Active Plan");
		});

		it("shows PR branch name when PR is the last item", () => {
			addTestNotif({ branch: "hotfix/critical" });
			const { container } = render(() => <Toolbar />);
			const btn = container.querySelector(".lastItemBtn");
			expect(btn?.textContent).toContain("hotfix/critical");
		});
	});

	// The app name used to be position:absolute at left:50% inside .left, so at a
	// narrow sidebar it was painted straight over the right-aligned toggles.
	describe("left region layout", () => {
		// sidebarWidth is module-level state; don't leak a narrow width into later tests.
		afterEach(() => uiStore.setSidebarWidth(300));

		it("keeps the app name in the flex flow, never absolutely positioned", () => {
			const { container } = render(() => <Toolbar />);
			const left = container.querySelector(".left");
			expect(left).not.toBeNull();
			// Spacers on both sides make the name a shrinkable flex item that stays
			// visually centred without being able to overlap the toggles.
			expect(left?.querySelectorAll(".leftSpacer").length).toBe(2);
			const name = left?.querySelector(".appName");
			expect(name).not.toBeNull();
			expect(name?.previousElementSibling?.classList.contains("leftSpacer")).toBe(true);
		});
	});
	describe("ahead/behind count", () => {
		/** Answers `get_github_status` per checkout path, like the Rust command does. */
		function mockCheckoutStatus(byPath: Record<string, { ahead: number; behind: number }>) {
			vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
				if (cmd !== "get_github_status") return undefined;
				const path = (args as { path: string }).path;
				const counts = byPath[path] ?? { ahead: 0, behind: 0 };
				return { has_remote: true, current_branch: "b", ...counts };
			});
		}

		function selectBranch(repoPath: string, branch: string, worktreePath: string | null) {
			repositoriesStore.add({ path: repoPath, displayName: "Repo" });
			repositoriesStore.setActive(repoPath);
			repositoriesStore.setWorkspace(repoPath, branch, { branchName: branch, worktreePath });
			repositoriesStore.setActiveWorkspace(repoPath, branch);
		}

		afterEach(() => {
			vi.mocked(invoke).mockReset();
			vi.mocked(invoke).mockResolvedValue(undefined);
		});

		it("does not show the repo root count next to a worktree branch without upstream", async () => {
			const root = "/ab1/repo";
			// Main checkout is 1004 ahead of origin/main; the worktree branch has no upstream (0/0).
			githubStore.setRemoteStatus(root, { has_remote: true, current_branch: "main", ahead: 1004, behind: 0 });
			mockCheckoutStatus({ "/ab1/repo-wt/feat": { ahead: 0, behind: 0 } });
			selectBranch(root, "feat", "/ab1/repo-wt/feat");

			const { container } = render(() => <Toolbar repoPath={root} />);

			await waitFor(() =>
				expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_github_status", { path: "/ab1/repo-wt/feat" }),
			);
			expect(container.querySelector(".aheadBehind")).toBeNull();
		});

		it("shows the worktree's own count when its branch is ahead of its upstream", async () => {
			const root = "/ab2/repo";
			githubStore.setRemoteStatus(root, { has_remote: true, current_branch: "main", ahead: 1004, behind: 0 });
			mockCheckoutStatus({ "/ab2/repo-wt/feat": { ahead: 3, behind: 2 } });
			selectBranch(root, "feat", "/ab2/repo-wt/feat");

			const { container } = render(() => <Toolbar repoPath={root} />);

			await waitFor(() => expect(container.querySelector(".aheadBehind")?.textContent).toBe(" ↑3 ↓2"));
		});

		it("shows the repo root count for the main checkout", () => {
			const root = "/ab3/repo";
			githubStore.setRemoteStatus(root, { has_remote: true, current_branch: "main", ahead: 5, behind: 0 });
			selectBranch(root, "main", null);

			const { container } = render(() => <Toolbar repoPath={root} />);

			expect(container.querySelector(".aheadBehind")?.textContent).toBe(" ↑5");
		});

		it("shows no count for the main checkout when it is level with its upstream", () => {
			const root = "/ab4/repo";
			githubStore.setRemoteStatus(root, { has_remote: true, current_branch: "main", ahead: 0, behind: 0 });
			selectBranch(root, "main", null);

			const { container } = render(() => <Toolbar repoPath={root} />);

			expect(container.querySelector(".aheadBehind")).toBeNull();
		});
	});
});
