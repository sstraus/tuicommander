// @vitest-environment jsdom

import { cleanup, configure, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toastsStore } from "../../stores/toasts";
import MobileApp from "../MobileApp";
import type { SessionInfo } from "../useSessions";

const { rpc, subscribePty, output } = vi.hoisted(() => ({
	rpc: vi.fn(),
	subscribePty: vi.fn(async () => () => {}),
	output: { text: "" },
}));

vi.mock("../../transport", () => ({
	rpc,
	subscribePty,
	subscribeEvents: vi.fn(async () => () => {}),
	isTauri: () => false,
}));
vi.mock("../../invoke", () => ({
	invoke: vi.fn(),
	listen: vi.fn(async () => () => {}),
}));
vi.mock("../../stores/appLogger", () => ({ appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() } }));
vi.mock("../../stores/ideas", () => ({ ideasStore: { hydrate: vi.fn() } }));
vi.mock("../useSessions", () => ({
	useSessions: () => ({
		aiChatAvailable: () => true,
		sessions: () => [session],
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
vi.mock("../components/TerminalKeybar", () => ({ TerminalKeybar: () => <div /> }));
vi.mock("../../components/McpConfirmHost/McpConfirmHost", () => ({ McpConfirmHost: () => <div /> }));

const session: SessionInfo = {
	session_id: "session-1",
	cwd: "/repo",
	worktree_path: null,
	worktree_branch: "main",
	state: { awaiting_input: false, rate_limited: false, last_activity_ms: 1, agent_type: "codex" },
};

// Every wait below polls for its condition; the deadlines only bound a hang.
// The 1s waitFor and 5s test defaults expire when a loaded full-suite run
// starves this file of CPU, and the test deadline must outlast the wait.
configure({ asyncUtilTimeout: 20_000 });
vi.setConfig({ testTimeout: 30_000, hookTimeout: 30_000 });

const guide = "# Guide\n\nThird line of the guide.\n";

beforeEach(async () => {
	// Prime the lazy screens before rendering so a cold Vite transform is not
	// mistaken for a missing Files editor by waitFor's UI deadline.
	await Promise.all([import("../screens/FilesScreen"), import("../screens/MobileChatScreen")]);
	rpc.mockReset();
	subscribePty.mockClear();
	rpc.mockImplementation(async (command: string, args?: Record<string, string>) => {
		if (command === "resolve_terminal_path") {
			const candidate = args?.candidate ?? "";
			if (candidate.includes("secret")) return { absolute_path: "/secret/private.md", is_directory: false };
			if (candidate.includes("shortcut")) return { absolute_path: "/repo-other/private.md", is_directory: false };
			if (args?.cwd !== "/repo") return null;
			return { absolute_path: "/repo/docs/guide.md", is_directory: false };
		}
		if (command === "list_queued_agent_commands") return [];
		if (command === "load_repositories") return { repos: { "/repo": {} } };
		if (command === "stat_path") return { exists: true, is_dir: false, size: guide.length };
		if (command === "fs_read_file") {
			if (args?.repoPath === "/repo" && args?.file === "docs/guide.md") return guide;
			throw new Error("Read outside the expected repository");
		}
		if (command === "list_directory" || command === "list_open_markdown_tabs") return [];
		throw new Error(`Unexpected command: ${command}`);
	});
	vi.stubGlobal(
		"fetch",
		vi.fn(async () => ({
			ok: true,
			json: async () => ({ lines: [{ spans: [{ text: output.text }] }], total_lines: 1 }),
		})),
	);
	history.replaceState(null, "", "/mobile/session/session-1");
});

afterEach(() => {
	cleanup();
	for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	vi.unstubAllGlobals();
	history.replaceState(null, "", "/mobile");
});

describe("mobile session output links", () => {
	it("opens absolute, relative, file URL and tuic Markdown references in the session Files editor", async () => {
		output.text =
			"Read /repo/docs/guide.md, docs/guide.md, file:///repo/docs/guide.md, and tuic://open//repo/docs/guide.md";
		const view = render(() => <MobileApp />);
		for (const label of [
			"/repo/docs/guide.md",
			"docs/guide.md",
			"file:///repo/docs/guide.md",
			"tuic://open//repo/docs/guide.md",
		]) {
			const link = await waitFor(() => view.getByRole("button", { name: label }));
			await fireEvent.click(link);
			await waitFor(() => expect(view.container.querySelector("#markdown-content h1")?.textContent).toBe("Guide"));
			await fireEvent.click(view.getByRole("button", { name: "Back to session" }));
		}
		expect(rpc).toHaveBeenCalledWith("resolve_terminal_path", expect.objectContaining({ cwd: "/repo" }));
	});

	it("positions the Markdown editor at a line and refuses a file outside the allowed roots", async () => {
		output.text = "docs/guide.md:3 and /secret/private.md";
		const view = render(() => <MobileApp />);
		await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "docs/guide.md:3" })));
		const editor = (await waitFor(() => view.getByRole("textbox", { name: "File content" }))) as HTMLTextAreaElement;
		await waitFor(() => {
			expect(editor.value).toBe(guide);
			expect(editor.selectionStart).toBe(guide.indexOf("Third line"));
		});
		await fireEvent.click(view.getByRole("button", { name: "Back to session" }));
		// Back remounts the output, which refetches before the links render again.
		await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "/secret/private.md" })));
		await waitFor(() => expect(view.getByRole("alert").textContent).toMatch(/outside.*registered repository/i));
		expect(view.getByText("/secret/private.md", { selector: "[class*='message']" })).toBeTruthy();
		expect(rpc).not.toHaveBeenCalledWith("fs_read_file", expect.objectContaining({ file: "/secret/private.md" }));
	});

	it("opens web URLs outside the PWA and leaves non-Markdown paths as text", async () => {
		output.text =
			"See https://example.com/guide?q=1 and http://example.org/notes. Ignore /repo/src/main.rs and version.mdish";
		const view = render(() => <MobileApp />);
		const link = await waitFor(() => view.getByRole("link", { name: "https://example.com/guide?q=1" }));
		expect(link.getAttribute("href")).toBe("https://example.com/guide?q=1");
		expect(link.getAttribute("target")).toBe("_blank");
		expect(link.getAttribute("rel")).toContain("noopener");
		const plainHttp = view.getByRole("link", { name: "http://example.org/notes" });
		expect(plainHttp.getAttribute("target")).toBe("_blank");
		expect(plainHttp.getAttribute("rel")).toContain("noopener");
		expect(view.queryByRole("button", { name: "/repo/src/main.rs" })).toBeNull();
		expect(view.queryByRole("button", { name: "version.mdish" })).toBeNull();
	});

	it("refuses a symlink that the resolver canonicalizes into a sibling repository", async () => {
		output.text = "/repo/shortcut.md";
		const view = render(() => <MobileApp />);
		await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "/repo/shortcut.md" })));
		await waitFor(() =>
			expect(view.getByText("/repo-other/private.md", { selector: "[class*='message']" })).toBeTruthy(),
		);
		expect(rpc).not.toHaveBeenCalledWith("fs_read_file", expect.anything());
	});
});
