// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { aiChatDraft } from "../../components/AIChatPanel/draft";
import MobileApp from "../MobileApp";

const { rpc, chatText } = vi.hoisted(() => ({
	rpc: vi.fn(),
	chatText: { value: "" },
}));

vi.mock("../../transport", () => ({ rpc, isTauri: () => false }));
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
		entries: () => [{ id: "answer", kind: "agent", text: chatText.value }],
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
	useVersionCheck: () => ({ updateAvailable: () => false, serverDown: () => false, applyUpdate: vi.fn() }),
}));
vi.mock("../../components/McpConfirmHost/McpConfirmHost", () => ({ McpConfirmHost: () => <div /> }));

beforeEach(async () => {
	await Promise.all([
		import("../screens/MobileChatScreen"),
		import("../screens/FilesScreen"),
		import("../mobileTheme"),
	]);
	aiChatDraft.reset();
	chatText.value = "";
	rpc.mockReset().mockImplementation(async (command: string, args?: Record<string, string>) => {
		if (command === "resolve_terminal_path") {
			if (args?.candidate === "project/missing.md") return null;
			if (args?.candidate === "project/docs/")
				return { absolute_path: "/home/boss/Gits/project/docs", is_directory: true };
			return { absolute_path: "/home/boss/Gits/project/docs/guide.md", is_directory: false };
		}
		if (command === "load_repositories") return { repos: {} };
		if (command === "stat_path") return { exists: true, is_dir: false, size: 8 };
		if (command === "fs_read_file") return "# Guide\n";
		if (command === "list_directory") return [];
		throw new Error(`Unexpected command: ${command}`);
	});
	history.replaceState(null, "", "/mobile");
});

afterEach(() => {
	cleanup();
	history.replaceState(null, "", "/mobile");
});

describe("mobile AI Chat file links", () => {
	it("opens a linked file in Files using the backend-resolved path", async () => {
		chatText.value = "[guide](project/docs/guide.md)";
		const view = render(() => <MobileApp />);
		fireEvent.click(view.getByRole("button", { name: "Chat" }));
		fireEvent.click(await waitFor(() => view.getByRole("link", { name: "guide" })));
		await waitFor(() => expect(view.container.querySelector("#markdown-content h1")?.textContent).toBe("Guide"));
		expect(rpc).toHaveBeenCalledWith("resolve_terminal_path", {
			cwd: "/home/boss/Gits",
			candidate: "project/docs/guide.md",
		});
		expect(rpc).toHaveBeenCalledWith("fs_read_file", {
			repoPath: "/home/boss/Gits",
			file: "project/docs/guide.md",
		});
	});

	it("opens a linked directory as the Files root", async () => {
		chatText.value = "[folder](project/docs/)";
		const view = render(() => <MobileApp />);
		fireEvent.click(view.getByRole("button", { name: "Chat" }));
		fireEvent.click(await waitFor(() => view.getByRole("link", { name: "folder" })));
		await waitFor(() =>
			expect(rpc).toHaveBeenCalledWith("list_directory", {
				repoPath: "/home/boss/Gits/project/docs",
				subdir: "",
			}),
		);
		expect(rpc).not.toHaveBeenCalledWith("stat_path", expect.anything());
	});

	it("keeps an unresolved link in Files without reading a guessed path", async () => {
		chatText.value = "[missing](project/missing.md)";
		const view = render(() => <MobileApp />);
		fireEvent.click(view.getByRole("button", { name: "Chat" }));
		fireEvent.click(await waitFor(() => view.getByRole("link", { name: "missing" })));
		await waitFor(() => expect(view.getByRole("alert").textContent).toContain("unavailable"));
		expect(rpc).not.toHaveBeenCalledWith("fs_read_file", expect.anything());
	});
});
