import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/usePty", () => ({
	usePty: () => ({
		createSession: vi.fn().mockResolvedValue("sess-toggle"),
		resize: vi.fn(),
		close: vi.fn(),
		getKittyFlags: vi.fn().mockResolvedValue(0),
	}),
}));
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn().mockResolvedValue(undefined),
	subscribePty: () => Promise.resolve(() => {}),
}));
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn(async (cmd: string) => (cmd === "get_session_foreground_process" ? "claude" : undefined)),
}));
vi.mock("../../../stores/agentConfigs", () => ({
	ensureAgentConfigsForRepo: vi.fn().mockResolvedValue({ getEnvFlags: () => ({}) }),
	agentConfigsForRepo: () => ({ getEnvFlags: () => ({}) }),
}));
vi.mock("../../../components/Terminal/glyphCache", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../components/Terminal/glyphCache")>()),
	getSharedMetrics: () => ({ cellWidth: 8, cellHeight: 16 }),
}));
const canvas = vi.hoisted(() => ({
	focus: vi.fn(),
	refresh: vi.fn(),
	searchClear: vi.fn(),
	searchFind: vi.fn().mockResolvedValue({ index: 0, count: 1 }),
	searchNext: vi.fn().mockReturnValue({ index: 0, count: 1 }),
	searchPrev: vi.fn().mockReturnValue({ index: 0, count: 1 }),
}));
vi.mock("../../../components/Terminal/CanvasTerminal", () => ({
	default: (props: { onRef: (ref: unknown) => void }) => {
		props.onRef(canvas);
		return <div data-testid="canvas" />;
	},
}));
// Compose's editor is outside the Find contract; keep its focus timers out of this boundary test.
vi.mock("../../../components/ComposePanel/ComposePanel", () => ({
	ComposePanel: () => <textarea aria-label="Compose" />,
}));
vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async (command: string) => {
		if (command === "get_session_foreground_process") return "claude";
		if (command === "discover_agent_session") return { sessionId: "uuid", launchCommand: null };
		if (command === "chat_view_snapshot")
			return {
				epoch: 0,
				nextSeq: 0,
				reset: true,
				updates: [],
				unknownRows: 0,
				malformedRows: 0,
			};
		return null;
	}),
	listen: vi.fn(async () => () => {}),
}));

import { Terminal } from "../../../components/Terminal/Terminal";
import { acpTranscript } from "../../../stores/acpTranscript";
import { chatViewKey, chatViewStore } from "../../../stores/chatView";
import { terminalsStore } from "../../../stores/terminals";

afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});

describe("visible terminal view search", () => {
	// Catches: Cmd+F searches the hidden canvas instead of the visible transcript.
	it("searches chat text, highlights and scrolls next/previous, and clears on mode switch", async () => {
		vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
		vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
		const id = terminalsStore.add({
			sessionId: "live-search",
			cwd: "/repo",
			name: "claude",
			fontSize: 13,
			awaitingInput: null,
			agentType: "claude",
			agentSessionId: "uuid",
		});
		await import("../../../components/ComposePanel/ComposePanel");
		const view = render(() => <Terminal id={id} cwd="/repo" alwaysVisible />);
		terminalsStore.setViewMode(id, "chat");
		await view.findByLabelText("Chat transcript");
		await waitFor(() => expect(chatViewStore.state.cursors["live-search"]?.epoch).toBe(0));
		acpTranscript.restore(chatViewKey("live-search"), [
			{ id: "first", kind: "user", text: "first needle" },
			{ id: "second", kind: "user", text: "second needle" },
		]);
		terminalsStore.get(id)?.ref?.openSearch();
		const input = await view.findByRole("textbox", { name: "Find in chat" });
		fireEvent.input(input, { target: { value: "needle" } });
		const scroll = vi.spyOn(HTMLElement.prototype, "scrollIntoView").mockImplementation(() => {});
		fireEvent.keyDown(input, { key: "Enter" });
		expect(window.getSelection()?.toString()).toBe("needle");
		expect(window.getSelection()?.anchorNode?.parentElement?.textContent).toContain("first needle");
		expect(scroll).toHaveBeenCalled();
		expect(canvas.searchFind).not.toHaveBeenCalled();
		fireEvent.keyDown(input, { key: "Enter" });
		expect(window.getSelection()?.anchorNode?.parentElement?.textContent).toContain("second needle");
		fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
		expect(window.getSelection()?.anchorNode?.parentElement?.textContent).toContain("first needle");
		terminalsStore.setViewMode(id, "cli");
		await waitFor(() => expect(view.queryByRole("textbox", { name: "Find in chat" })).toBeNull());
		expect(window.getSelection()?.toString()).toBe("");
		terminalsStore.get(id)?.ref?.openSearch();
		const cliInput = view.getByPlaceholderText("Find…");
		fireEvent.input(cliInput, { target: { value: "cli needle" } });
		await view.findByText("1 of 1");
		expect(canvas.searchFind).toHaveBeenCalledWith("cli needle", false);
		fireEvent.keyDown(cliInput, { key: "Enter" });
		expect(canvas.searchNext).toHaveBeenCalledOnce();
		fireEvent.keyDown(cliInput, { key: "Enter", shiftKey: true });
		expect(canvas.searchPrev).toHaveBeenCalledOnce();
		const cleared = canvas.searchClear.mock.calls.length;
		terminalsStore.setViewMode(id, "chat");
		expect(view.queryByPlaceholderText("Find…")).toBeNull();
		expect(canvas.searchClear.mock.calls.length).toBeGreaterThan(cleared);
		await view.findByLabelText("Chat transcript");
		await waitFor(() => expect(chatViewStore.state.cursors["live-search"]?.epoch).toBe(0));
		acpTranscript.restore(chatViewKey("live-search"), [
			{ id: "first", kind: "user", text: "first needle" },
			{ id: "second", kind: "user", text: "second needle" },
		]);
		terminalsStore.get(id)?.ref?.openSearch();
		const reopened = view.getByRole("textbox", { name: "Find in chat" });
		fireEvent.input(reopened, { target: { value: "needle" } });
		fireEvent.keyDown(reopened, { key: "Enter", shiftKey: true });
		expect(window.getSelection()?.anchorNode?.parentElement?.textContent).toContain("second needle");
		fireEvent.input(reopened, { target: { value: "absent" } });
		expect(window.getSelection()?.toString()).toBe("");
		fireEvent.keyDown(reopened, { key: "Escape" });
		expect(view.queryByRole("textbox", { name: "Find in chat" })).toBeNull();
	});
});
