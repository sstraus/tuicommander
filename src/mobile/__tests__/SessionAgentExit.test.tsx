import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal, Show } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { waitForAgentEnterGap } from "../../utils/sendCommand";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo, SessionState } from "../useSessions";

const { rpc, stream } = vi.hoisted(() => ({
	rpc: vi.fn(),
	stream: { push: (_state: Record<string, unknown>) => {} },
}));
vi.mock("../../transport", () => ({ rpc, subscribeEvents: vi.fn(async () => () => {}) }));
vi.mock("../../invoke", () => ({ invoke: rpc }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() } }));
// Exercise the screen's real snapshot consumer and composer, without opening a PTY.
vi.mock("../components/OutputView", () => ({
	OutputView: (props: { onStateChange: typeof stream.push; visible: boolean }) => {
		stream.push = (state) => props.onStateChange(state);
		return (
			<Show when={props.visible}>
				<div>Shell output</div>
			</Show>
		);
	},
}));

const shell: SessionState = {
	awaiting_input: false,
	rate_limited: false,
	last_activity_ms: 2,
	shell_state: "idle",
};
const waiting: SessionState = {
	...shell,
	agent_type: "ego",
	agent_state: "awaiting_input",
	awaiting_input: true,
	question_confident: true,
	question_text: "Continue?",
};

function mount(agent = "ego") {
	const initial: SessionInfo = {
		session_id: `exit-${agent}`,
		cwd: "/repo",
		worktree_path: null,
		worktree_branch: null,
		state: { ...waiting, agent_type: agent },
	};
	const [session, setSession] = createSignal(initial);
	const [exists, setExists] = createSignal(true);
	const back = vi.fn();
	const view = render(() => (
		<SessionDetailScreen
			session={session()}
			sessionExists={exists()}
			subscribeChatWake={() => () => {}}
			onBack={back}
			onOpenFiles={() => {}}
		/>
	));
	return {
		...view,
		poll: (state: SessionState) => setSession({ ...initial, state }),
		close: () => setExists(false),
		back,
	};
}

beforeEach(() => {
	rpc.mockImplementation(async (method: string) => {
		if (method === "get_hands_free_status") throw new Error("404");
		if (method === "chat_view_snapshot") return { epoch: 1, nextSeq: 0, reset: true, updates: [] };
		return {};
	});
});
afterEach(() => {
	cleanup();
	rpc.mockReset();
});

describe("mobile viewed agent exit", () => {
	// Catches: a retained WS question overrides every fresh shell poll after exit.
	it("fresh_poll_releases_stale_websocket_question_after_agent_exit", async () => {
		const view = mount();
		stream.push(waiting as unknown as Record<string, unknown>);
		await fireEvent.click(view.getByRole("button", { name: "Open ego question" }));
		view.poll(shell);
		expect(view.queryByRole("button", { name: "Open ego question" })).toBeNull();
		expect(view.getByLabelText("Terminal logo")).toBeTruthy();
		const input = view.container.querySelector("textarea")!;
		expect(input.disabled).toBe(false);
		await fireEvent.input(input, { target: { value: "pwd" } });
		await fireEvent.click(view.getByRole("button", { name: "Send" }));
		await waitFor(() => expect(rpc).toHaveBeenCalledWith("write_pty", { sessionId: "exit-ego", data: "pwd" }));
		await waitFor(() => expect(rpc).toHaveBeenCalledWith("write_pty", { sessionId: "exit-ego", data: "\r" }));
		await waitForAgentEnterGap(null);
		await waitFor(() => expect((view.getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled).toBe(false));
		expect(rpc.mock.calls.some(([method]) => method === "submit_agent_reply")).toBe(false);
	});

	// Catches: a delayed pre-exit GET resurrects question controls after the WS has released the agent.
	it("late_poll_does_not_restore_question_after_websocket_agent_exit", () => {
		const view = mount();
		// GET /sessions captures the question, then the socket delivers agent exit
		// before that independent HTTP response reaches the phone.
		const preExitSnapshot = { ...waiting, last_activity_ms: 1 };
		stream.push({ ...shell, last_activity_ms: 3 });
		expect(view.queryByRole("button", { name: "Open ego question" })).toBeNull();
		view.poll(preExitSnapshot);
		expect(view.queryByRole("button", { name: "Open ego question" })).toBeNull();
		expect(view.getByLabelText("Terminal logo")).toBeTruthy();
	});

	// Catches: merging a complete sparse WS snapshot resurrects omitted agent fields from an old poll.
	it("websocket_shell_snapshot_does_not_restore_omitted_agent_fields", () => {
		const view = mount();
		stream.push(shell as unknown as Record<string, unknown>);
		expect(view.getByLabelText("Terminal logo")).toBeTruthy();
		expect(view.queryByRole("button", { name: "Open ego question" })).toBeNull();
	});

	// Catches: a stale WS identity keeps the Chat pane active after a reconnect catch-up poll.
	it("fresh_poll_returns_chat_to_cli_after_agent_exit", async () => {
		const view = mount("claude");
		stream.push({ ...waiting, agent_type: "claude" });
		await fireEvent.click(view.getByRole("tab", { name: /^Chat$/ }));
		expect(view.queryByText("Shell output")).toBeNull();
		view.poll(shell);
		expect(view.getByText("Shell output")).toBeTruthy();
		expect(view.queryByRole("tablist", { name: "Terminal view" })).toBeNull();
	});

	// Catches: PTY closure leaves the user trapped in agent controls instead of offering Back.
	it("pty_closure_disables_input_and_keeps_back_available", async () => {
		const view = mount();
		stream.push(waiting as unknown as Record<string, unknown>);
		view.close();
		expect(view.getByText("Session ended")).toBeTruthy();
		expect(view.container.querySelector("textarea")!.disabled).toBe(true);
		await fireEvent.click(view.getByRole("button", { name: /^Back$/ }));
		expect(view.back).toHaveBeenCalledOnce();
	});
});
