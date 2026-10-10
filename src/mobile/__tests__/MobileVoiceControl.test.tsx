import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo } from "../useSessions";

const { rpc, toastAdd, voice } = vi.hoisted(() => {
	const voice = {
		state: { handsFree: null as unknown, handsFreeError: null as string | null },
		setSpokenReplies: vi.fn(),
		armHandsFree: vi.fn(),
		disarmHandsFree: vi.fn(),
		setStatus: (_status: unknown) => {},
		setError: (_message: string | null) => {},
	};
	return { rpc: vi.fn(), toastAdd: vi.fn(), voice };
});

vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc,
}));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: toastAdd } }));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div /> }));
vi.mock("../components/TerminalKeybar", () => ({ TerminalKeybar: () => <div /> }));
vi.mock("../../stores/dictation", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({
		handsFree: null as unknown,
		handsFreeError: null as string | null,
		spokenReplies: true,
	});
	voice.state = state as typeof voice.state;
	voice.setStatus = (status) => setState("handsFree", status);
	voice.setError = (message) => setState("handsFreeError", message);
	return {
		browserAudioOwner: "browser-test",
		dictationStore: {
			state,
			refreshConfig: vi.fn().mockResolvedValue(undefined),
			setSpokenReplies: voice.setSpokenReplies,
			armHandsFree: voice.armHandsFree,
			disarmHandsFree: voice.disarmHandsFree,
		},
	};
});

const session: SessionInfo = {
	session_id: "s1",
	cwd: "/repo",
	worktree_path: null,
	worktree_branch: null,
	state: { agent_type: "claude", awaiting_input: false, rate_limited: false, last_activity_ms: 1 },
};

const armedStatus = { armed: true, phase: "waiting", sessionId: "s1", owner: "browser-test" };

function mount() {
	return render(() => (
		<SessionDetailScreen session={session} sessionExists={true} onBack={() => {}} onOpenFiles={() => {}} />
	));
}

beforeEach(() => {
	rpc.mockResolvedValue({ armed: false });
	voice.armHandsFree.mockImplementation(async () => {
		voice.setStatus(armedStatus);
		return true;
	});
	voice.disarmHandsFree.mockImplementation(async () => {
		voice.setStatus(null);
	});
});

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
	voice.setStatus(null);
});

it("hides the voice control when the server has no hands-free route", async () => {
	// catches: a dead mic button on headless tuic-remote, which answers 404.
	rpc.mockRejectedValue(new Error("RPC get_hands_free_status failed: 404"));
	mount();
	await waitFor(() => expect(rpc).toHaveBeenCalledWith("get_hands_free_status"));
	await Promise.resolve();
	expect(screen.queryByRole("button", { name: /voice conversation/ })).toBeNull();
});

it("arms synchronously inside the tap and disarms on the second tap", async () => {
	// catches: arming after an await, which spends the iOS user gesture before the AudioContext can resume.
	mount();
	const button = await screen.findByRole("button", { name: "Start voice conversation" });

	fireEvent.click(button);
	expect(voice.armHandsFree).toHaveBeenCalledWith("s1");

	const stop = await screen.findByRole("button", { name: "Stop voice conversation" });
	fireEvent.click(stop);
	await waitFor(() => expect(voice.disarmHandsFree).toHaveBeenCalledTimes(1));
	await screen.findByRole("button", { name: "Start voice conversation" });
});

it("shows a refused start as a readable toast without the Error prefix", async () => {
	// catches: the raw "Error: ..." string, or silence, when the mic is refused.
	voice.armHandsFree.mockImplementation(async () => {
		voice.setError("Error: Voice needs a secure page");
		return false;
	});
	mount();
	fireEvent.click(await screen.findByRole("button", { name: "Start voice conversation" }));
	await waitFor(() =>
		expect(toastAdd).toHaveBeenCalledWith("Voice not started", "Voice needs a secure page", "error", true),
	);
});

it("ends the conversation when the session screen is left", async () => {
	// catches: the microphone staying open on a screen the user has navigated away from.
	const { unmount } = mount();
	fireEvent.click(await screen.findByRole("button", { name: "Start voice conversation" }));
	await screen.findByRole("button", { name: "Stop voice conversation" });

	unmount();

	expect(voice.disarmHandsFree).toHaveBeenCalledTimes(1);
});

it("keeps Spoken replies out of the session screen when voice is available", async () => {
	// catches: the redundant Spoken replies row returning beneath the session header.
	mount();
	await screen.findByRole("button", { name: "Start voice conversation" });
	expect(screen.queryByRole("checkbox", { name: "Spoken replies" })).toBeNull();
	expect(screen.queryByText("Spoken replies")).toBeNull();
});
