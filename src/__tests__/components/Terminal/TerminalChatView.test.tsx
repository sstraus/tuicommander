import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import claudeNativeQuestion from "./fixtures/native-claude-question-2079.json";

const { mockOpenFile } = vi.hoisted(() => ({ mockOpenFile: vi.fn() }));

vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async (command: string, args?: { cwd?: string; candidate?: string }) => {
		if (command === "resolve_terminal_path") {
			return args?.candidate === "reports/x.md"
				? { absolute_path: `${args.cwd}/reports/x.md`, is_directory: false }
				: null;
		}
		return { epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 };
	}),
	listen: vi.fn(async () => () => {}),
}));
vi.mock("../../../utils/filePreview", () => ({ openTerminalFilePath: mockOpenFile }));

import { TerminalChatView, ViewModeToggle } from "../../../components/Terminal/TerminalChatView";
import { applySessionStateEvent } from "../../../hooks/useAgentPolling";
import { useTerminalLifecycle } from "../../../hooks/useTerminalLifecycle";
import { invoke } from "../../../invoke";
import { acpTranscript } from "../../../stores/acpTranscript";
import { chatViewKey, chatViewStore } from "../../../stores/chatView";
import { terminalsStore } from "../../../stores/terminals";

function addTerminal(agentType: "claude" | "ego" | null, agentSessionId: string | null) {
	return terminalsStore.add({
		sessionId: "sess-1",
		fontSize: 14,
		name: "t",
		cwd: "/repo",
		awaitingInput: null,
		agentType,
		agentSessionId,
	});
}

beforeEach(() => {
	chatViewStore.reset();
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
});

describe("TerminalChatView", () => {
	// Catches: Chat stays at the app font size instead of following its session's CLI zoom.
	it("chat_follows_session_zoom_actions", () => {
		const id = addTerminal("claude", "uuid");
		const other = terminalsStore.add({
			sessionId: "sess-2",
			fontSize: 20,
			name: "other",
			cwd: "/repo",
			awaitingInput: null,
		});
		terminalsStore.setActive(id);
		let zoom!: ReturnType<typeof useTerminalLifecycle>;
		const { getByTestId } = render(() => {
			zoom = useTerminalLifecycle({
				pty: { canSpawn: async () => true, close: async () => {} },
				dialogs: {
					confirmCloseTerminal: async () => false,
					confirmSaveChanges: async () => "cancel",
					confirm: async () => false,
				},
				setStatusInfo: () => {},
				getDefaultFontSize: () => 14,
			});
			zoom.zoomIn();
			return <TerminalChatView terminalId={id} sessionId="sess-1" />;
		});
		const size = () => getByTestId("terminal-chat-view").style.getPropertyValue("--font-base");
		expect(size()).toBe("16px");
		zoom.zoomIn();
		expect(size()).toBe("18px");
		zoom.zoomOut();
		expect(size()).toBe("16px");
		zoom.zoomReset();
		expect(size()).toBe("14px");
		expect(terminalsStore.get(other)?.fontSize).toBe(20);
		zoom.zoomInAll();
		expect(size()).toBe("16px");
		expect(terminalsStore.get(other)?.fontSize).toBe(22);
		zoom.zoomOutAll();
		expect(size()).toBe("14px");
		zoom.zoomResetAll();
		expect(size()).toBe("14px");
		expect(terminalsStore.get(other)?.fontSize).toBe(14);
	});
	// Catches: a blank panel after the agent dies (grid hidden, nothing in its place).
	it("view_falls_back_to_cli_when_agent_exits", async () => {
		const id = addTerminal("claude", "uuid");
		terminalsStore.setViewMode(id, "chat");
		render(() => <TerminalChatView terminalId={id} sessionId="sess-1" />);
		expect(terminalsStore.get(id)?.viewMode).toBe("chat");

		terminalsStore.update(id, { agentType: null });
		await Promise.resolve();

		expect(terminalsStore.get(id)?.viewMode).toBe("cli");
		expect(chatViewStore.unavailableReason("sess-1")).toBeTruthy();
	});
});

// Catches: confidently awaiting exported choices disappear from desktop PTY Chat.
it("desktop_terminal_chat_renders_active_native_choices", () => {
	const id = addTerminal("claude", "uuid");
	terminalsStore.setViewMode(id, "chat");
	applySessionStateEvent({
		session_id: "sess-1",
		state: {
			agent_type: "claude",
			awaiting_input: true,
			question_confident: true,
			question_text: "Pick a colour?",
			choice_prompt: {
				title: "Pick a colour?",
				options: [
					{ key: "1", label: "Red", highlighted: true, destructive: false },
					{ key: "2", label: "Green", highlighted: false, destructive: false },
					{ key: "3", label: "Blue", highlighted: false, destructive: false },
				],
			},
		},
	});
	const view = render(() => <TerminalChatView terminalId={id} sessionId="sess-1" />);
	expect(view.getByText("Pick a colour?")).toBeTruthy();
	expect(view.getByRole("button", { name: /Blue/ })).toBeTruthy();
});

describe("TerminalChatView file links", () => {
	const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
	async function renderAnswer(text: string) {
		const id = addTerminal("claude", "uuid");
		terminalsStore.setViewMode(id, "chat");
		const view = render(() => <TerminalChatView terminalId={id} sessionId="sess-1" />);
		await settle();
		acpTranscript.restore(chatViewKey("sess-1"), [{ id: "a1", kind: "agent", text }]);
		await settle();
		return view;
	}

	// Catches: Chat view Transcript wired without onOpenFile, and a relative path not resolved against the terminal cwd.
	it("opens the absolute path of a backticked relative path on click", async () => {
		const { container } = await renderAnswer("Report in `reports/x.md` now.");
		const link = container.querySelector<HTMLAnchorElement>("a[data-tuic-href]");
		expect(link?.dataset.tuicHref).toBe("reports/x.md");
		link?.click();
		await settle();
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/reports/x.md");
	});

	// Catches: a click on a missing file opening something or throwing.
	it("opens nothing when the path does not exist", async () => {
		mockOpenFile.mockClear();
		const { container } = await renderAnswer("Gone: `reports/missing.md`.");
		container.querySelector<HTMLAnchorElement>("a[data-tuic-href]")?.click();
		await settle();
		expect(mockOpenFile).not.toHaveBeenCalled();
	});

	// Catches: a nonexistent target remaining a clickable dead link after resolution fails.
	it("removes the dead link when a clicked file no longer exists", async () => {
		const { container } = await renderAnswer("Gone: `reports/missing.md`.");
		const link = container.querySelector<HTMLAnchorElement>("a[data-tuic-href]");
		expect(link).not.toBeNull();
		link?.click();
		await settle();
		expect(container.textContent).toContain("reports/missing.md");
		expect(container.querySelector("a[data-tuic-href='reports/missing.md']")).toBeNull();
	});
});

describe("ViewModeToggle", () => {
	it("is absent for a plain shell tab", () => {
		const id = addTerminal(null, null);
		const { queryByRole } = render(() => <ViewModeToggle terminalId={id} />);
		expect(queryByRole("tablist", { name: "Terminal view" })).toBeNull();
	});

	it("lets the backend decide Claude binding when the discovered id is missing", () => {
		const id = addTerminal("claude", null);
		const { getByText } = render(() => <ViewModeToggle terminalId={id} />);
		const chat = getByText("Chat") as HTMLButtonElement;
		expect(chat.disabled).toBe(false);
	});

	it("switches the terminal to chat when bound", () => {
		const id = addTerminal("claude", "uuid");
		const { getByText } = render(() => <ViewModeToggle terminalId={id} />);
		(getByText("Chat") as HTMLButtonElement).click();
		expect(terminalsStore.get(id)?.viewMode).toBe("chat");
		(getByText("CLI") as HTMLButtonElement).click();
		expect(terminalsStore.get(id)?.viewMode).toBe("cli");
	});
});

function nativeQuestion(id: string, agent: "claude" | "ego" = "claude", dismiss = false) {
	terminalsStore.setViewMode(id, "chat");
	applySessionStateEvent({
		session_id: "sess-1",
		state: {
			agent_type: agent,
			awaiting_input: true,
			question_confident: true,
			choice_prompt: {
				title: "Approve command?",
				...(dismiss ? { dismiss_key: "esc" } : {}),
				options: [
					{ key: "1", label: "Approve once", highlighted: true, destructive: false },
					{ key: "2", label: "Refuse", highlighted: false, destructive: true },
				],
			},
		},
	});
	return render(() => <TerminalChatView terminalId={id} sessionId="sess-1" />);
}

// Catches: native approval/refusal routes through Compose, or adds Enter to an immediate-answer dialog.
it.each(["Approve once", "Refuse"])("sends only the native key for %s without submitting a draft", async (label) => {
	const id = addTerminal("ego", "uuid");
	const view = nativeQuestion(id, "ego", true);
	vi.mocked(invoke).mockClear();
	view.getByRole("button", { name: new RegExp(label) }).click();
	await Promise.resolve();
	await Promise.resolve();
	const writes = vi.mocked(invoke).mock.calls.filter(([command]) => command === "write_pty");
	expect(writes).toEqual([["write_pty", { sessionId: "sess-1", data: label === "Refuse" ? "2" : "1" }]]);
});

// Catches: an old native dialog stays actionable after terminal replacement or unavailable transcript.
it.each(["replacement", "unavailable"])("hides native answers after %s", async (reason) => {
	const id = addTerminal("claude", "uuid");
	const view = nativeQuestion(id);
	if (reason === "replacement") terminalsStore.update(id, { sessionId: "replacement" });
	else chatViewStore.markUnavailable("sess-1", "Transcript unavailable");
	await Promise.resolve();
	expect(view.queryByRole("button", { name: /Approve once/ })).toBeNull();
});

// Catches: snapshot omission retains the answered native choices in desktop Chat.
it("retracts native controls when the authoritative question clears", async () => {
	const id = addTerminal("claude", "uuid");
	const view = nativeQuestion(id);
	applySessionStateEvent({ session_id: "sess-1", state: { agent_type: "claude", awaiting_input: false } });
	await Promise.resolve();
	expect(view.queryByRole("button", { name: /Approve once/ })).toBeNull();
	expect(terminalsStore.get(id)?.choicePrompt).toBeNull();
});

// Catches: an in-flight multi-key answer reaches a replacement PTY or a composer after the dialog clears.
it.each(["replacement", "retraction", "unavailable"])("stops native answer continuation after %s", async (reason) => {
	const id = addTerminal("claude", "uuid");
	const view = nativeQuestion(id);
	await new Promise((resolve) => setTimeout(resolve, 0));
	let release!: () => void;
	const pending = new Promise<void>((resolve) => {
		release = resolve;
	});
	const original = vi.mocked(invoke).getMockImplementation()!;
	const warning = vi.spyOn((await import("../../../stores/appLogger")).appLogger, "warn").mockImplementation(() => {});
	vi.mocked(invoke).mockImplementation((command, args) =>
		command === "write_pty" ? pending : original(command, args),
	);
	vi.mocked(invoke).mockClear();
	try {
		view.getByRole("button", { name: /Approve once/ }).click();
		if (reason === "replacement") terminalsStore.update(id, { sessionId: "replacement" });
		else if (reason === "unavailable") chatViewStore.markUnavailable("sess-1", "Transcript unavailable");
		else applySessionStateEvent({ session_id: "sess-1", state: { agent_type: "claude", awaiting_input: false } });
		release();
		await waitFor(() => expect(warning).toHaveBeenCalled());
		expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "write_pty")).toEqual([
			["write_pty", { sessionId: "sess-1", data: "1" }],
		]);
	} finally {
		vi.mocked(invoke).mockImplementation(original);
		warning.mockRestore();
	}
});

// Catches: desktop sends numeric shortcuts instead of Claude's exported navigation/Enter protocol.
it("answers a recorded native Claude question with navigation keys on the same PTY", async () => {
	const id = addTerminal("claude", "uuid");
	terminalsStore.setViewMode(id, "chat");
	applySessionStateEvent({ session_id: "sess-1", state: claudeNativeQuestion });
	const view = render(() => <TerminalChatView terminalId={id} sessionId="sess-1" />);
	await new Promise((resolve) => setTimeout(resolve, 0));
	vi.mocked(invoke).mockClear();
	view.getByRole("button", { name: /Blue/ }).click();
	await waitFor(() =>
		expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "write_pty")).toHaveLength(3),
	);
	expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "write_pty")).toEqual([
		["write_pty", { sessionId: "sess-1", data: "\x1b[B" }],
		["write_pty", { sessionId: "sess-1", data: "\x1b[B" }],
		["write_pty", { sessionId: "sess-1", data: "\r" }],
	]);
});

// Catches: adopting the shared view switch without forwarding keyboard selection to the terminal store.
it("selects terminal Chat with the shared switch arrow key", () => {
	const id = addTerminal("claude", "uuid");
	const view = render(() => <ViewModeToggle terminalId={id} />);
	fireEvent.keyDown(view.getByRole("tab", { name: "CLI" }), { key: "ArrowRight" });
	expect(terminalsStore.get(id)?.viewMode).toBe("chat");
	expect(view.getByRole("tab", { name: "Chat" }).getAttribute("aria-selected")).toBe("true");
});
