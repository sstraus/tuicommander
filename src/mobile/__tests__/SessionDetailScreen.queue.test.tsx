import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { QueuedCommand } from "../../hooks/usePty";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo } from "../useSessions";

vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	rpc: vi.fn(),
	subscribeEvents: vi.fn(async () => () => {}),
}));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div>Terminal output</div> }));
vi.mock("../../components/Terminal/TerminalChatTranscript", () => ({
	TerminalChatTranscript: () => <div>Conversation</div>,
}));
vi.mock("../useMobileVoice", () => ({
	useMobileVoice: () => ({ phase: () => null, available: () => false, armed: () => false, recording: () => false }),
}));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));

import { HttpRpcError, rpc } from "../../transport";

let queue: QueuedCommand[];
let nextId = 1;
let warnings: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
	queue = [];
	warnings = vi.spyOn(console, "warn").mockImplementation(() => {});
	vi.mocked(rpc).mockImplementation(async (command, args) => {
		if (command === "list_queued_agent_commands") return [...queue];
		if (command === "enqueue_agent_command") {
			queue.push({ id: nextId++, text: args!.text as string, kind: "user_command" });
			return { accepted: true, typed: false };
		}
		if (command === "remove_queued_agent_command") {
			queue = queue.filter((item) => item.id !== args!.commandId);
			return true;
		}
		if (command === "clear_queued_agent_commands") {
			queue = [];
			return 1;
		}
		return undefined;
	});
});
afterEach(() => {
	cleanup();
	for (const args of warnings.mock.calls)
		expect(args).toEqual(["computations created outside a `createRoot` or `render` will never be disposed"]);
	warnings.mockRestore();
	vi.clearAllMocks();
});
function mount() {
	const [session, setSession] = createSignal<SessionInfo>({
		session_id: `queue-${nextId++}`,
		cwd: "/repo",
		worktree_path: null,
		worktree_branch: null,
		state: {
			agent_type: "claude",
			shell_state: "busy",
			awaiting_input: false,
			rate_limited: false,
			last_activity_ms: 0,
		},
	});
	const view = render(() => (
		<SessionDetailScreen
			session={session()}
			sessionExists
			subscribeChatWake={() => () => {}}
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
	return {
		...view,
		session,
		setSession,
		updateCount: (count: number) => setSession((s) => ({ ...s, state: { ...s.state!, queued_commands: count } })),
	};
}
async function enqueue(view: ReturnType<typeof mount>, text: string) {
	fireEvent.input(view.getByRole("textbox"), { target: { value: text } });
	fireEvent.click(view.getByRole("button", { name: "More composer actions" }));
	fireEvent.click(view.getByRole("menuitem", { name: "Queue" }));
	await waitFor(() => expect((view.getByRole("textbox") as HTMLTextAreaElement).value).toBe(""));
}

// Catches: successful queue clears the input but leaves Chat with only a transient toast.
it("shows queued text immediately before the lifecycle count arrives, then clears on delivery", async () => {
	const view = mount();
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	await enqueue(view, "Review the migration");
	const region = await view.findByRole("region", { name: "Queued prompts" });
	expect(within(region).getByRole("status").textContent).toBe("Queued · 1");
	expect(within(region).getByText("Review the migration")).toBeTruthy();
	expect(vi.mocked(rpc).mock.calls.some(([cmd, args]) => cmd === "write_pty" && args?.data === "\r")).toBe(false);
	view.updateCount(1);
	queue = [];
	view.updateCount(0);
	await waitFor(() => expect(view.queryByRole("region", { name: "Queued prompts" })).toBeNull());
});

// Catches: Hide leaves queue rows visible or removes the only remaining queue count.
it("hides queued rows but keeps the live count and reopens the queue", async () => {
	const view = mount();
	await enqueue(view, "Hidden task");
	fireEvent.click(view.getByRole("button", { name: "Hide queued prompts" }));
	expect(view.queryByText("Hidden task")).toBeNull();
	expect(view.queryByRole("button", { name: "Clear all" })).toBeNull();
	expect(view.getByRole("status").textContent).toBe("Queued · 1");
	queue.push({ id: nextId++, text: "Another task", kind: "user_command" });
	view.updateCount(2);
	await waitFor(() => expect(view.getByRole("status").textContent).toBe("Queued · 2"));
	expect(view.queryByText("Another task")).toBeNull();
	fireEvent.click(view.getByRole("button", { name: "Show queued prompts" }));
	expect(view.getByText("Hidden task")).toBeTruthy();
	expect(view.getByText("Another task")).toBeTruthy();
});

// Catches: a hidden empty queue leaves the next accepted prompt invisible.
it("opens new queued work after the hidden queue empties", async () => {
	const view = mount();
	await enqueue(view, "Delivered task");
	view.updateCount(1);
	fireEvent.click(view.getByRole("button", { name: "Hide queued prompts" }));
	queue = [];
	view.updateCount(0);
	await waitFor(() => expect(view.queryByRole("region", { name: "Queued prompts" })).toBeNull());
	await enqueue(view, "New task");
	expect(await view.findByText("New task")).toBeTruthy();
	expect(view.getByRole("button", { name: "Hide queued prompts" })).toBeTruthy();
});

// Catches: local disclosure state leaks from one selected session into another.
it("opens another session's queued work after hiding the previous session", async () => {
	const view = mount();
	await enqueue(view, "Previous task");
	fireEvent.click(view.getByRole("button", { name: "Hide queued prompts" }));
	queue = [{ id: 97, text: "Other session task", kind: "user_command" }];
	view.setSession((s) => ({ ...s, session_id: "other-session" }));
	expect(await view.findByText("Other session task")).toBeTruthy();
});

// Catches: server mail wakes and spawned tasks leak into the user's mobile queue/count.
it("excludes agent mail by origin without hiding user text that resembles a mail wake", async () => {
	const view = mount();
	const userText = "[TUIC] message available — read it with: agent action=inbox";
	queue = [
		{ id: 101, text: "Server mail wake", kind: "notice" },
		{ id: 102, text: "Spawned task", kind: "initial_prompt" },
		{ id: 103, text: userText, kind: "user_command" },
	];
	view.updateCount(3);
	expect(await view.findByText(userText)).toBeTruthy();
	expect(view.queryByText("Server mail wake")).toBeNull();
	expect(view.queryByText("Spawned task")).toBeNull();
	expect(view.getByRole("status").textContent).toBe("Queued · 1");
	fireEvent.click(view.getByRole("button", { name: "Hide queued prompts" }));
	expect(view.getByRole("status").textContent).toBe("Queued · 1");
	fireEvent.click(view.getByRole("button", { name: "Show queued prompts" }));
	fireEvent.click(view.getByRole("button", { name: "Clear all" }));
	await waitFor(() => expect(view.queryByRole("region", { name: "Queued prompts" })).toBeNull());
	expect(queue.map((item) => item.id)).toEqual([101, 102]);
});

// Catches: a raw FIFO count resurrects a badge for server-only work.
it("has no mobile queue for background-only work and reopens new user work", async () => {
	const view = mount();
	await enqueue(view, "User work");
	fireEvent.click(view.getByRole("button", { name: "Hide queued prompts" }));
	queue = [{ id: 104, text: "Background only", kind: "notice" }];
	view.updateCount(1);
	await waitFor(() => expect(view.queryByRole("region", { name: "Queued prompts" })).toBeNull());
	await enqueue(view, "Next user work");
	expect(await view.findByText("Next user work")).toBeTruthy();
	expect(view.getByRole("status").textContent).toBe("Queued · 1");
});

// Catches: Chat invents its own empty queue and loses entries added in CLI or desktop.
it("keeps queued CLI text on switching to Chat and adopts desktop queue count changes", async () => {
	const view = mount();
	await enqueue(view, "Queued in CLI");
	await view.findByText("Queued in CLI");
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	expect(view.getByText("Queued in CLI")).toBeTruthy();
	queue.push({ id: nextId++, kind: "user_command", text: "Queued on desktop" });
	view.updateCount(2);
	await view.findByText("Queued on desktop");
	expect(view.getByRole("status").textContent).toBe("Queued · 2");
});

// Catches: cancellation removes by display position, or a successful clear leaves stale text.
it("cancels stable queue ids and clears remaining entries from the server snapshot", async () => {
	const view = mount();
	queue = [
		{ id: 41, text: "First task", kind: "user_command" },
		{ id: 85, text: "Second task", kind: "user_command" },
	];
	view.updateCount(2);
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	await view.findByRole("button", { name: "Cancel queued prompt First task" });
	fireEvent.click(view.getByRole("button", { name: "Hide queued prompts" }));
	fireEvent.click(view.getByRole("button", { name: "Show queued prompts" }));
	fireEvent.click(view.getByRole("button", { name: "Cancel queued prompt First task" }));
	await waitFor(() => expect(view.queryByText("First task")).toBeNull());
	expect(rpc).toHaveBeenCalledWith("remove_queued_agent_command", {
		sessionId: view.session().session_id,
		commandId: 41,
	});
	expect(view.getByText("Second task")).toBeTruthy();
	fireEvent.click(view.getByRole("button", { name: "Clear all" }));
	await waitFor(() => expect(view.queryByRole("region", { name: "Queued prompts" })).toBeNull());
});

// Catches: a slow queue read resurrects prompts from a previously selected terminal.
it("does not render a departed terminal's late queue snapshot", async () => {
	const view = mount();
	let resolveOld!: (items: QueuedCommand[]) => void;
	vi.mocked(rpc).mockImplementation(async (command, args) => {
		if (command !== "list_queued_agent_commands") return undefined;
		if (args?.sessionId === "next-queue") return [];
		return new Promise<QueuedCommand[]>((resolve) => {
			resolveOld = resolve;
		});
	});
	view.updateCount(1);
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	await waitFor(() => expect(resolveOld).toBeTypeOf("function"));
	view.setSession((s) => ({ ...s, session_id: "next-queue" }));
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	resolveOld([{ id: 23, text: "Wrong terminal task", kind: "user_command" }]);
	await Promise.resolve();
	expect(view.queryByText("Wrong terminal task")).toBeNull();
});

// Catches: a failed cancel hides an undelivered prompt or leaks a rejected promise.
it("keeps queued text when cancellation fails and explains the failure", async () => {
	const view = mount();
	queue = [{ id: 91, text: "Keep this task", kind: "user_command" }];
	view.updateCount(1);
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	const cancel = await view.findByRole("button", { name: "Cancel queued prompt Keep this task" });
	vi.mocked(rpc).mockImplementation(async (command) => {
		if (command === "remove_queued_agent_command") throw new Error("offline");
		return [...queue];
	});
	fireEvent.click(cancel);
	await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Could not remove queued prompts"));
	expect(view.getByText("Keep this task")).toBeTruthy();
});

// Catches: an older desktop's missing queue endpoint adds a permanent alert to every empty session.
it("does not alert for an unsupported queue endpoint without pending queue evidence", async () => {
	vi.mocked(rpc).mockRejectedValue(new HttpRpcError("list_queued_agent_commands", 404, "no such endpoint"));
	const view = mount();
	await waitFor(() =>
		expect(vi.mocked(rpc).mock.settledResults.some((result) => result.type === "rejected")).toBe(true),
	);
	expect(view.queryByRole("region", { name: "Queued prompts" })).toBeNull();
	expect(view.queryByRole("alert")).toBeNull();
});

// Catches: a non-array host reply crashes the queue computation instead of leaving the session usable.
it.each([undefined, null, {}, "unavailable"])(
	"ignores non-array queue replies (%j) and recovers on a valid snapshot",
	async (reply) => {
		vi.mocked(rpc).mockImplementation(async () => reply);
		const view = mount();
		await waitFor(() =>
			expect(vi.mocked(rpc).mock.settledResults.some((result) => result.type === "fulfilled")).toBe(true),
		);
		expect(view.queryByRole("region", { name: "Queued prompts" })).toBeNull();
		expect(view.queryByRole("alert")).toBeNull();
		expect(view.getByRole("textbox")).toBeTruthy();

		vi.mocked(rpc).mockResolvedValue([{ id: 121, text: "Recovered task", kind: "user_command" }]);
		view.updateCount(1);
		expect(await view.findByText("Recovered task")).toBeTruthy();
		expect(view.getByRole("status").textContent).toBe("Queued · 1");
	},
);

// Catches: treating authentication, server and network failures as unsupported queue capability.
it.each([401, 500, "offline"] as const)("keeps queue read failure %s visible and retryable", async (failure) => {
	vi.mocked(rpc).mockRejectedValue(
		typeof failure === "number"
			? new HttpRpcError("list_queued_agent_commands", failure, "request failed")
			: new Error(failure),
	);
	const view = mount();
	expect((await view.findByRole("alert")).textContent).toContain("Could not load queued prompts.");
	vi.mocked(rpc).mockResolvedValue([]);
	fireEvent.click(view.getByRole("button", { name: "Retry" }));
	await waitFor(() => expect(view.queryByRole("alert")).toBeNull());
});

// Catches: hiding a missing queue read endpoint even though the host reports undelivered work.
it("keeps a missing endpoint visible when the session reports pending prompts", async () => {
	vi.mocked(rpc).mockRejectedValue(new HttpRpcError("list_queued_agent_commands", 404, "no such endpoint"));
	const view = mount();
	view.updateCount(1);
	expect((await view.findByRole("alert")).textContent).toContain("Could not load queued prompts.");
	expect(view.getByRole("status").textContent).toBe("Queued");
});

// Catches: hiding a queued prompt after local acceptance but before the lifecycle count arrives.
it("keeps a missing endpoint visible after a successful local enqueue", async () => {
	vi.mocked(rpc).mockImplementation(async (command) => {
		if (command === "list_queued_agent_commands") throw new HttpRpcError(command, 404, "no such endpoint");
		if (command === "enqueue_agent_command") return { accepted: true, typed: false };
		return undefined;
	});
	const view = mount();
	await enqueue(view, "Do not lose this task");
	expect((await view.findByRole("alert")).textContent).toContain("Could not load queued prompts.");
});

// Catches: a partly failed Clear all leaves removed rows stale or deletes unseen mail.
it("refreshes user prompts after a partial clear failure while preserving background work", async () => {
	const view = mount();
	queue = [
		{ id: 111, text: "First visible", kind: "user_command" },
		{ id: 112, text: "Second visible", kind: "user_command" },
		{ id: 113, text: "Hidden mail", kind: "notice" },
	];
	view.updateCount(3);
	await view.findByText("Second visible");
	const handler = vi.mocked(rpc).getMockImplementation()!;
	vi.mocked(rpc).mockImplementation(async (command, args, ...rest) => {
		if (command === "remove_queued_agent_command" && args?.commandId === 112) throw new Error("offline");
		return handler(command, args, ...rest);
	});
	fireEvent.click(view.getByRole("button", { name: "Clear all" }));
	await waitFor(() => expect(view.queryByText("First visible")).toBeNull());
	expect(view.getByText("Second visible")).toBeTruthy();
	expect(view.getByRole("alert").textContent).toContain("Could not remove queued prompts");
	expect(view.getByRole("status").textContent).toBe("Queued · 1");
	expect(queue.map((item) => item.id)).toEqual([112, 113]);
});
