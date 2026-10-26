// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal, onMount } from "solid-js";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { subscribeEvents } from "../../transport";
import { OpenMarkdownMenu } from "../components/OpenMarkdownMenu";
import MobileApp from "../MobileApp";
import type { SessionInfo } from "../useSessions";

const { rpc, mockSessions, outputMounts, events } = vi.hoisted(() => ({
	rpc: vi.fn(),
	events: { handlers: {} as Record<string, (payload: unknown) => void>, resync: () => {} },
	mockSessions: { current: [] as SessionInfo[] },
	outputMounts: { count: 0 },
}));

vi.mock("../../transport", () => ({
	rpc,
	isTauri: () => false,
	subscribeEvents: vi.fn(
		async (handlers: Record<string, (payload: unknown) => void>, options: { onResync: () => void }) => {
			events.handlers = handlers;
			events.resync = options.onResync;
			return () => {};
		},
	),
}));
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../mobileTheme", () => ({ loadMobileTheme: vi.fn(async () => "commander") }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), info: vi.fn(), error: vi.fn() } }));
vi.mock("../../stores/ideas", () => ({ ideasStore: { hydrate: vi.fn() } }));
vi.mock("../useSessions", () => ({
	useSessions: () => ({
		aiChatAvailable: () => true,
		sessions: () => mockSessions.current,
		loading: () => false,
		refreshing: () => false,
		error: () => null,
		authError: () => false,
		refresh: vi.fn(),
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

const tab = {
	id: "md-1",
	title: "guide.md",
	relative_path: "docs/guide.md",
	absolute_path: "/repo/docs/guide.md",
	fs_root: "/repo",
};
let tabs: (typeof tab & { opener_tuic_session?: string })[] = [tab];
const session: SessionInfo = {
	session_id: "session-1",
	cwd: "/repo/src",
	worktree_path: null,
	worktree_branch: null,
	state: { awaiting_input: false, rate_limited: false, last_activity_ms: 1 },
};
beforeAll(async () => {
	await Promise.all([
		import("../screens/FilesScreen"),
		import("../screens/SessionDetailScreen"),
		import("../mobileTheme"),
	]);
});
beforeEach(() => {
	tabs = [tab];
	vi.stubGlobal("innerWidth", 390);
	mockSessions.current = [session];
	events.handlers = {};
	rpc.mockReset();
	rpc.mockImplementation(async (command: string) => {
		if (command === "get_hands_free_status") throw new Error("Voice unavailable in the headless test backend");
		if (command === "list_open_markdown_tabs") return tabs;
		if (command === "load_repositories") return { repos: { "/repo": {}, "/other": {} } };
		if (command === "resolve_terminal_path") return { absolute_path: "/repo/docs/guide.md", is_directory: false };
		if (command === "stat_path") return { exists: true, is_dir: false, size: 40 };
		if (command === "fs_read_file") return "# Desktop guide\n\nVisible on mobile.";
		return [];
	});
	history.replaceState(null, "", "/mobile/session/session-1");
});
afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
	history.replaceState(null, "", "/mobile");
});

// Catches: a desktop close between the initial read and the first SSE connection leaving a stale entry.
it("refreshes tabs closed before the first SSE connection opens", async () => {
	vi.stubGlobal("__TAURI_SHIM__", true);
	const streams: Array<{ onopen: (() => void) | null }> = [];
	vi.stubGlobal(
		"EventSource",
		class {
			onopen: (() => void) | null = null;
			onerror: (() => void) | null = null;
			constructor() {
				streams.push(this);
			}
			addEventListener() {}
			close() {}
		},
	);
	const transport = await vi.importActual<typeof import("../../transport/ipc")>("../../transport/ipc");
	vi.mocked(subscribeEvents).mockImplementationOnce(transport.subscribeEvents);
	const view = render(() => <OpenMarkdownMenu cwd="/repo" onOpen={() => {}} />);
	await waitFor(() => expect(view.getByRole("button", { name: "guide.md docs/guide.md" })).toBeTruthy());
	// No SSE subscriber existed when the desktop closed this tab.
	tabs = [];
	await waitFor(() => expect(streams).toHaveLength(1));
	streams[0].onopen?.();
	await waitFor(() => expect(view.getByText("No open Markdown tabs")).toBeTruthy());
});
async function openMenu(view: ReturnType<typeof render>) {
	await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "More session actions" })));
}
// Catches: missing desktop Markdown section or labels without the relative file path.
it("lists the session repository desktop Markdown titles and paths", async () => {
	const view = render(() => <MobileApp />);
	await openMenu(view);
	await waitFor(() => expect(view.getByRole("region", { name: "Markdown" })).toBeTruthy());
	expect(view.getByRole("button", { name: "guide.md docs/guide.md" })).toBeTruthy();
});
// Catches: a listed tab that opens nothing or bypasses the existing Markdown renderer.
it("opens a desktop entry in the existing mobile Markdown viewer", async () => {
	const view = render(() => <MobileApp />);
	await openMenu(view);
	await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "guide.md docs/guide.md" })));
	await waitFor(() => expect(view.container.querySelector("#markdown-content h1")?.textContent).toBe("Desktop guide"));
	await fireEvent.click(view.getByRole("button", { name: "Back to session" }));
	expect(view.getByRole("button", { name: "More session actions" })).toBeTruthy();
});
// Catches: a first-load snapshot that never refreshes after desktop open or close.
it("updates the rendered menu after desktop tab open and close events", async () => {
	const view = render(() => <MobileApp />);
	await openMenu(view);
	await waitFor(() => expect(view.getByRole("button", { name: "guide.md docs/guide.md" })).toBeTruthy());
	tabs = [
		...tabs,
		{ ...tab, id: "md-2", title: "notes.md", relative_path: "notes.md", absolute_path: "/repo/notes.md" },
	];
	events.handlers["open-markdown-tabs-changed"]?.({});
	await waitFor(() => expect(view.getByRole("button", { name: "notes.md notes.md" })).toBeTruthy());
	tabs = [];
	events.handlers["open-markdown-tabs-changed"]?.({});
	await waitFor(() => expect(view.queryByRole("button", { name: "guide.md docs/guide.md" })).toBeNull());
	expect(view.getByText("No open Markdown tabs")).toBeTruthy();
});

// Catches: a tab from another checkout of the same repo resolving against the session checkout.
it("opens a same-repository worktree tab from its original checkout", async () => {
	tabs = [{ ...tab, absolute_path: "/worktrees/feature/docs/guide.md", fs_root: "/worktrees/feature" }];
	const original = rpc.getMockImplementation()!;
	rpc.mockImplementation(async (command: string, args?: Record<string, string>) => {
		if (command === "resolve_terminal_path") return { absolute_path: args?.candidate, is_directory: false };
		if (command === "fs_read_file") {
			if (args?.repoPath !== "/worktrees/feature" || args?.file !== "docs/guide.md") throw new Error("Wrong checkout");
			return "# Worktree guide";
		}
		return original(command);
	});
	const view = render(() => <MobileApp />);
	await openMenu(view);
	await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "guide.md docs/guide.md" })));
	await waitFor(() => expect(view.container.querySelector("#markdown-content h1")?.textContent).toBe("Worktree guide"));
});

// Catches: a reconnect retaining tabs closed while the phone was disconnected.
it("refetches the rendered entries after SSE resynchronization", async () => {
	const view = render(() => <OpenMarkdownMenu cwd="/repo" onOpen={() => {}} />);
	await waitFor(() => expect(view.getByRole("button", { name: "guide.md docs/guide.md" })).toBeTruthy());
	tabs = [];
	events.resync();
	await waitFor(() => expect(view.getByText("No open Markdown tabs")).toBeTruthy());
});

// Catches: a slow older request resurrecting closed desktop tabs after a newer invalidation.
it("ignores a stale read completed after the desktop close response", async () => {
	const view = render(() => <OpenMarkdownMenu cwd="/repo" onOpen={() => {}} />);
	await waitFor(() => expect(view.getByRole("button", { name: "guide.md docs/guide.md" })).toBeTruthy());
	let resolveOld: ((value: typeof tabs) => void) | undefined;
	rpc.mockImplementationOnce(
		() =>
			new Promise<typeof tabs>((resolve) => {
				resolveOld = resolve;
			}),
	);
	events.handlers["open-markdown-tabs-changed"]({});
	tabs = [];
	events.handlers["open-markdown-tabs-changed"]({});
	await waitFor(() => expect(view.getByText("No open Markdown tabs")).toBeTruthy());
	resolveOld?.([tab]);
	await Promise.resolve();
	expect(view.queryByRole("button", { name: "guide.md docs/guide.md" })).toBeNull();
});

// Catches: request failures presenting a stale or misleading empty list as successful.
it("reports a failed metadata read and recovers on the next desktop event", async () => {
	rpc.mockRejectedValueOnce(new Error("network down"));
	const view = render(() => <OpenMarkdownMenu cwd="/repo" onOpen={() => {}} />);
	await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Could not load"));
	events.handlers["open-markdown-tabs-changed"]({});
	await waitFor(() => expect(view.getByRole("button", { name: "guide.md docs/guide.md" })).toBeTruthy());
	expect(view.queryByRole("alert")).toBeNull();
});

// Catches: unchanged-cwd session-state polls resetting the mounted list to Loading.
it("keeps the open list mounted across two unchanged-cwd session polls", async () => {
	const [snapshot, setSnapshot] = createSignal(session);
	const view = render(() => <OpenMarkdownMenu cwd={snapshot().cwd} onOpen={() => {}} />);
	const entry = await waitFor(() => view.getByRole("button", { name: "guide.md docs/guide.md" }));
	const reads = rpc.mock.calls.filter(([command]) => command === "list_open_markdown_tabs").length;
	for (const tick of [2, 3]) {
		setSnapshot({ ...session, state: { ...session.state!, last_activity_ms: tick } });
		expect(view.queryByText("Loading…")).toBeNull();
		expect(view.getByRole("button", { name: "guide.md docs/guide.md" })).toBe(entry);
		await Promise.resolve();
	}
	expect(rpc.mock.calls.filter(([command]) => command === "list_open_markdown_tabs")).toHaveLength(reads);
});

// Catches: real cwd updates blanking the old list while the replacement is in flight.
it("replaces changed cwd results in place without an empty frame", async () => {
	const [cwd, setCwd] = createSignal("/repo");
	const view = render(() => <OpenMarkdownMenu cwd={cwd()} onOpen={() => {}} />);
	await waitFor(() => view.getByRole("button", { name: "guide.md docs/guide.md" }));
	let finish: ((value: typeof tabs) => void) | undefined;
	rpc.mockImplementationOnce(
		() =>
			new Promise<typeof tabs>((resolve) => {
				finish = resolve;
			}),
	);
	setCwd("/other");
	try {
		expect(view.queryByText("Loading…")).toBeNull();
		expect(view.getByRole("button", { name: "guide.md docs/guide.md" })).toBeTruthy();
	} finally {
		finish?.([{ ...tab, title: "other.md" }]);
	}
	await waitFor(() => view.getByRole("button", { name: "other.md docs/guide.md" }));
	expect(view.queryByRole("button", { name: "guide.md docs/guide.md" })).toBeNull();
});

// Catches: both or neither Markdown entry point being shown at a viewport boundary.
it("shows the top-bar dropdown only at wide widths and keeps one shared list", async () => {
	vi.stubGlobal("innerWidth", 1024);
	const view = render(() => <MobileApp />);
	const dropdown = await waitFor(() => view.getByRole("button", { name: "Markdown" }));
	const subscriptions = vi.mocked(subscribeEvents).mock.calls.length;
	await openMenu(view);
	expect(view.queryByRole("region", { name: "Markdown" })).toBeNull();
	await fireEvent.click(dropdown);
	await waitFor(() => view.getByRole("button", { name: "guide.md docs/guide.md" }));
	expect(view.getAllByRole("region", { name: "Markdown" })).toHaveLength(1);
	vi.stubGlobal("innerWidth", 767);
	window.dispatchEvent(new Event("resize"));
	expect(view.queryByRole("button", { name: "Markdown" })).toBeNull();
	await openMenu(view);
	expect(view.getAllByRole("region", { name: "Markdown" })).toHaveLength(1);
	vi.stubGlobal("innerWidth", 390);
	window.dispatchEvent(new Event("resize"));
	expect(view.getAllByRole("region", { name: "Markdown" })).toHaveLength(1);
	expect(vi.mocked(subscribeEvents).mock.calls.length).toBe(subscriptions);
	await fireEvent.click(view.getByRole("button", { name: "guide.md docs/guide.md" }));
	await waitFor(() => expect(view.container.querySelector("#markdown-content h1")?.textContent).toBe("Desktop guide"));
});

// Catches: the top-bar entry not using the existing Markdown opening path.
it("opens a wide-screen dropdown entry in the mobile Markdown viewer", async () => {
	vi.stubGlobal("innerWidth", 768);
	const view = render(() => <MobileApp />);
	await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "Markdown" })));
	await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "guide.md docs/guide.md" })));
	await waitFor(() => expect(view.container.querySelector("#markdown-content h1")?.textContent).toBe("Desktop guide"));
});

// Catches: mobile uses the session viewing the menu instead of the agent that opened the document.
it("preselects the desktop tab opener when opening its entry from another session", async () => {
	tabs = [{ ...tab, opener_tuic_session: "tuic-b" }];
	mockSessions.current = [
		{ ...session, state: { ...session.state!, agent_type: "ego" } },
		{ ...session, session_id: "pty-b", tuic_session: "tuic-b", state: { ...session.state!, agent_type: "ego" } },
	];
	const original = rpc.getMockImplementation()!;
	rpc.mockImplementation(async (command: string) =>
		command === "fs_read_file"
			? "<!--tweak:block:c1 @2026-10-25T12:00:00.000Z\nReview-->\n# Desktop guide"
			: original(command),
	);
	const view = render(() => <MobileApp />);
	await openMenu(view);
	await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "guide.md docs/guide.md" })));
	const select = await waitFor(() => view.getByRole("combobox", { name: "Review agent" }));
	expect(select).toHaveValue("pty-b");
	fireEvent.change(select, { target: { value: "session-1" } });
	expect(select).toHaveValue("session-1");
});
