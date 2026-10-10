// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { onMount } from "solid-js";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import MobileApp from "../MobileApp";
import type { SessionInfo } from "../useSessions";

const { rpc, mockSessions, mockRepos, outputMounts } = vi.hoisted(() => ({
	rpc: vi.fn(),
	mockSessions: { current: [] as SessionInfo[] },
	mockRepos: { current: {} as Record<string, unknown> },
	outputMounts: { count: 0 },
}));

vi.mock("../../transport", () => ({ rpc }));
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), info: vi.fn() } }));
vi.mock("../../stores/ideas", () => ({ ideasStore: { hydrate: vi.fn() } }));
vi.mock("../useSessions", () => ({
	useSessions: () => ({
		sessions: () => mockSessions.current,
		loading: () => false,
		refreshing: () => false,
		error: () => null,
		authError: () => false,
		refresh: vi.fn(),
		questionCount: () => 0,
	}),
}));
vi.mock("../useMobileNotifications", () => ({ useMobileNotifications: vi.fn() }));
vi.mock("../useVersionCheck", () => ({
	useVersionCheck: () => ({ updateAvailable: () => false, serverDown: () => false, applyUpdate: vi.fn() }),
}));
vi.mock("../components/OutputView", () => ({
	OutputView: () => {
		onMount(() => outputMounts.count++);
		return <div>Live session output</div>;
	},
}));
vi.mock("../components/TerminalKeybar", () => ({ TerminalKeybar: () => <div /> }));
vi.mock("../screens/MobileChatScreen", () => ({ MobileChatScreen: () => <div /> }));
vi.mock("../components/MobileToastContainer", () => ({ MobileToastContainer: () => <div /> }));
vi.mock("../../components/McpConfirmHost/McpConfirmHost", () => ({ McpConfirmHost: () => <div /> }));

function session(cwd: string | null, worktreePath: string | null = null): SessionInfo {
	return {
		session_id: "session-1",
		cwd,
		worktree_path: worktreePath,
		worktree_branch: null,
		state: { awaiting_input: false, rate_limited: false, last_activity_ms: 1 },
	};
}

beforeAll(async () => {
	// MobileApp loads its screens lazily. Finish every transform before behavior
	// waits, so a cold Vite transform on a loaded machine is not mistaken for a
	// missing screen by waitFor's UI deadline; the Chat route is mocked above.
	await Promise.all([
		import("../screens/FilesScreen"),
		import("../screens/SessionDetailScreen"),
		import("../screens/SettingsScreen"),
	]);
});

beforeEach(() => {
	mockRepos.current = { "/repo": {}, "/repo/nested": {} };
	rpc.mockReset();
	rpc.mockImplementation(async (command: string, args?: { repoPath?: string }) => {
		if (command === "load_repositories") return { repos: mockRepos.current };
		if (command === "list_directory") {
			if (args?.repoPath === "/refused") throw new Error("Permission denied");
			return [{ name: "README.md", path: "README.md", is_dir: false, size: 4 }];
		}
		throw new Error(`Unexpected command: ${command}`);
	});
	outputMounts.count = 0;
});

afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
	history.replaceState(null, "", "/mobile");
});

async function openSessionFiles(view: ReturnType<typeof render>) {
	const actions = await waitFor(() => view.getByRole("button", { name: "More session actions" }));
	await fireEvent.click(actions);
	await fireEvent.click(view.getByRole("button", { name: "Files" }));
}

describe("session Files navigation", () => {
	it("starts on the Sessions tab", () => {
		mockSessions.current = [];
		const view = render(() => <MobileApp />);
		expect(view.getByRole("button", { name: "Sessions" }).getAttribute("aria-current")).toBe("page");
	});
	it("opens Settings from the app bar and returns to Sessions", async () => {
		const unmockedFetch = globalThis.fetch;
		const versionFetch = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
			if (String(input) !== "/api/version") return unmockedFetch(input, init);
			return Promise.resolve({ ok: true, json: async () => ({ version: "2.4.1" }) } as Response);
		});
		vi.stubGlobal("fetch", versionFetch);
		mockSessions.current = [];
		const view = render(() => <MobileApp />);
		await fireEvent.click(view.getByRole("button", { name: "More options" }));
		await fireEvent.click(view.getByRole("menuitem", { name: "Settings" }));
		await waitFor(() => expect(view.getByRole("heading", { name: "CONNECTION" })).toBeTruthy());
		await waitFor(() => expect(view.getByText("2.4.1")).toBeTruthy());
		expect(versionFetch).toHaveBeenCalledWith("/api/version");
		await fireEvent.click(view.getByRole("button", { name: "Sessions" }));
		expect(view.getByRole("button", { name: "Sessions" }).getAttribute("aria-current")).toBe("page");
	});
	// Catches: opening the registered parent instead of the session's worktree, or resetting the detail on return.
	it("opens the worktree root and returns with the live output and draft intact", async () => {
		mockSessions.current = [session("/repo/nested/src", "/worktrees/feature")];
		history.replaceState(null, "", "/mobile/session/session-1");
		const view = render(() => <MobileApp />);
		await waitFor(() => expect(view.getByText("Live session output")).toBeTruthy());
		const composer = view.container.querySelector("textarea")!;
		await fireEvent.input(composer, { target: { value: "unsent draft" } });
		const writesBeforeBrowse = rpc.mock.calls.filter(([command]) => command === "write_pty").length;
		await openSessionFiles(view);
		await waitFor(() =>
			expect(rpc).toHaveBeenCalledWith("list_directory", {
				repoPath: "/worktrees/feature",
				subdir: "",
			}),
		);
		// The listing renders after the rpc resolves, not when it is called.
		await waitFor(() => expect(view.getByRole("button", { name: "README.md" })).toBeTruthy());
		expect(view.queryByRole("button", { name: /repo\/nested/ })).toBeNull();
		expect(rpc.mock.calls.filter(([command]) => command === "write_pty")).toHaveLength(writesBeforeBrowse);
		await fireEvent.click(view.getByRole("button", { name: "Back to session" }));
		expect(view.getByText("Live session output")).toBeTruthy();
		expect(view.container.querySelector("textarea")).toBe(composer);
		expect(composer.value).toBe("unsent draft");
		expect(outputMounts.count).toBe(1);
	});

	// Catches: using the nested cwd or the shorter parent as the file browser root.
	it("opens the deepest registered repository containing cwd", async () => {
		mockSessions.current = [session("/repo/nested/src")];
		history.replaceState(null, "", "/mobile/session/session-1");
		const view = render(() => <MobileApp />);
		await openSessionFiles(view);
		await waitFor(() =>
			expect(rpc).toHaveBeenCalledWith("list_directory", {
				repoPath: "/repo/nested",
				subdir: "",
			}),
		);
	});

	// Catches: applying POSIX separators to Windows session paths.
	it("opens the registered repository containing a Windows cwd", async () => {
		mockRepos.current = { "C:\\repo": {} };
		mockSessions.current = [session("C:\\repo\\src")];
		history.replaceState(null, "", "/mobile/session/session-1");
		const view = render(() => <MobileApp />);
		await openSessionFiles(view);
		await waitFor(() => expect(rpc).toHaveBeenCalledWith("list_directory", { repoPath: "C:\\repo", subdir: "" }));
	});

	// Catches: an empty Files screen or a misleading repository list when the session has no usable path.
	it("shows an error when the session has no repository path", async () => {
		mockSessions.current = [session(null)];
		history.replaceState(null, "", "/mobile/session/session-1");
		const view = render(() => <MobileApp />);
		await openSessionFiles(view);
		await waitFor(() => expect(view.getByRole("alert").textContent).toMatch(/repository path.*unavailable/i));
		expect(rpc).not.toHaveBeenCalledWith("list_directory", expect.anything());
	});

	// Catches: treating an unrelated directory with the same textual prefix as a registered repository.
	it("does not open an unregistered sibling of a repository", async () => {
		mockSessions.current = [session("/repo-other/src")];
		history.replaceState(null, "", "/mobile/session/session-1");
		const view = render(() => <MobileApp />);
		await openSessionFiles(view);
		await waitFor(() => expect(view.getByRole("alert").textContent).toMatch(/no registered repository/i));
		expect(rpc).not.toHaveBeenCalledWith("list_directory", expect.anything());
	});

	// Catches: treating a rejected directory request as an empty directory.
	it("shows a directory error when the session path is refused", async () => {
		mockSessions.current = [session("/repo", "/refused")];
		history.replaceState(null, "", "/mobile/session/session-1");
		const view = render(() => <MobileApp />);
		await openSessionFiles(view);
		await waitFor(() => expect(view.getByRole("alert").textContent).toMatch(/permission denied/i));
		expect(view.queryByText("Empty directory")).toBeNull();
	});

	// Catches: changing the original bottom tab to jump into a prior session.
	it("keeps the Files tab on the repository picker", async () => {
		mockSessions.current = [session("/repo/nested/src")];
		const view = render(() => <MobileApp />);
		await fireEvent.click(view.getByRole("button", { name: /files/i }));
		await waitFor(() => expect(view.getByRole("button", { name: /repo\/nested/ })).toBeTruthy());
		expect(rpc).not.toHaveBeenCalledWith("list_directory", expect.anything());
	});
});
