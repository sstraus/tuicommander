// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import MobileApp from "../MobileApp";

const { rpc } = vi.hoisted(() => ({ rpc: vi.fn() }));

vi.mock("../../transport", () => ({ rpc }));
vi.mock("../../invoke", () => ({
	invoke: vi.fn(),
	listen: vi.fn(async () => () => {}),
}));
vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn() } }));
vi.mock("../../stores/settings", () => ({
	settingsStore: { hydrate: vi.fn(async () => {}), state: { chatMuteIntermediateText: true } },
}));
vi.mock("../../stores/ideas", () => ({ ideasStore: { hydrate: vi.fn() } }));
vi.mock("../../components/AIChatPanel/useAcpChat", () => ({
	createAcpChat: () => ({
		phase: () => "live",
		root: () => "/home/boss/Gits",
		connectionId: () => "connection-1",
		sessionId: () => "chat-1",
		entries: () => [{ id: "answer", kind: "agent", text: "" }],
		busy: () => false,
		queuedPrompts: () => [],
		held: () => false,
		gap: () => null,
		error: () => null,
		isStreaming: () => true,
		interactions: () => [],
		sessions: () => [{ sessionId: "chat-1", title: "Current" }],
		capabilities: () => ({ list: true, load: true }),
		configOptions: () => [],
		answerPermission: vi.fn(),
		selectSession: vi.fn(),
		answerElicitation: vi.fn(),
		cancelPermission: vi.fn(),
		startSession: vi.fn(),
		ensureStarted: vi.fn(async () => {}),
		send: vi.fn(),
		cancel: vi.fn(),
		cancelQueued: vi.fn(),
		recallQueued: vi.fn().mockResolvedValue(null),
		recover: vi.fn(),
	}),
}));
vi.mock("../useSessions", () => ({
	useSessions: () => ({
		aiChatAvailable: () => true,
		sessions: () => [],
		loading: () => false,
		refreshing: () => false,
		error: () => null,
		authError: () => false,
		refresh: vi.fn(),
		markSeen: vi.fn(),
	}),
}));
vi.mock("../useMobileNotifications", () => ({ useMobileNotifications: vi.fn() }));
vi.mock("../useVersionCheck", () => ({
	useVersionCheck: () => ({ updateAvailable: () => true, serverDown: () => false, applyUpdate: vi.fn() }),
}));
vi.mock("../../components/McpConfirmHost/McpConfirmHost", () => ({ McpConfirmHost: () => <div /> }));

beforeEach(async () => {
	await Promise.all([
		import("../screens/MobileChatScreen"),
		import("../screens/FilesScreen"),
		import("../mobileTheme"),
	]);
	rpc.mockReset().mockImplementation(async (command: string) => {
		if (command === "load_repositories") return { repos: {} };
		if (command === "list_directory") return [];
		throw new Error(`Unexpected command: ${command}`);
	});
	history.replaceState(null, "", "/mobile");
});

afterEach(() => {
	cleanup();
	history.replaceState(null, "", "/mobile");
});

describe("mobile update banner", () => {
	// Catches: the update strip touching the translucent iOS status edge above the header.
	it("places the update strip after the app header", () => {
		const view = render(() => <MobileApp />);
		const header = view.getByText("TUICommander");
		const banner = view.getByText("New version available");
		expect(header.compareDocumentPosition(banner) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
	});

	// Catches: the "New version available" top bar showing on every tab instead of only Sessions.
	it("shows on the Sessions screen and not on other screens", async () => {
		const view = render(() => <MobileApp />);
		expect(view.queryByText("New version available")).not.toBeNull();
		fireEvent.click(view.getByRole("button", { name: "Chat" }));
		await waitFor(() => expect(view.queryByText("New version available")).toBeNull());
		fireEvent.click(view.getByRole("button", { name: "Sessions" }));
		await waitFor(() => expect(view.queryByText("New version available")).not.toBeNull());
	});
});
