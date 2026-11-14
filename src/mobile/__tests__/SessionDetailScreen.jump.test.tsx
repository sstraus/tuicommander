import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { transcriptPreferences } from "../../stores/transcriptPreferences";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { SessionInfo } from "../useSessions";

type Entry = { id: string; kind: "agent"; text: string };
const [entries, setEntries] = createSignal<Entry[]>([]);

vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	rpc: vi.fn(async () => undefined),
	subscribeEvents: vi.fn(async () => () => {}),
}));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div>Terminal output</div> }));
// Real Transcript, fed by the test: the screen, hook and jump control under test stay real.
vi.mock("../../components/Terminal/TerminalChatTranscript", () => ({
	TerminalChatTranscript: (props: Record<string, unknown>) => (
		<Transcript
			mobile
			entries={entries}
			busy={() => false}
			emptyMessage=""
			onAtBottomChange={props.onAtBottomChange as (v: boolean) => void}
			onNewTextBelowChange={props.onNewTextBelowChange as (v: boolean) => void}
			onRegisterScrollToBottom={props.onRegisterScrollToBottom as (fn: (() => void) | undefined) => void}
		/>
	),
}));
vi.mock("../useMobileVoice", () => ({
	useMobileVoice: () => ({ phase: () => null, available: () => false, armed: () => false, recording: () => false }),
}));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));

const session: SessionInfo = {
	session_id: "jump-session",
	cwd: "/repo",
	worktree_path: null,
	worktree_branch: null,
	state: { agent_type: "claude", shell_state: "idle", awaiting_input: false, rate_limited: false, last_activity_ms: 0 },
};

beforeEach(() => {
	localStorage.clear();
	transcriptPreferences.setEnabled(true);
	setEntries([{ id: "a", kind: "agent", text: "Latest answer" }]);
});
afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

async function openChatScrolledUp() {
	const view = render(() => (
		<SessionDetailScreen
			session={session}
			sessionExists
			subscribeChatWake={() => () => {}}
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	const viewport = (await view.findByLabelText("Chat transcript")) as HTMLDivElement;
	let top = 0;
	Object.defineProperties(viewport, {
		clientHeight: { value: 400 },
		scrollHeight: { value: 2000 },
		scrollTop: {
			get: () => top,
			set: (value: number) => {
				top = Math.max(0, Math.min(1600, value));
			},
		},
	});
	const scroll = (value: number) => {
		viewport.scrollTop = value;
		fireEvent.scroll(viewport);
	};
	await Promise.resolve();
	scroll(1600);
	scroll(800);
	return view;
}
const arrow = (view: ReturnType<typeof render>) => view.queryByRole("button", { name: "Scroll to bottom" });

// Catches: the jump arrow staying visible while the New text below badge shows.
it("mobile_chat_shows_one_jump_control", async () => {
	const view = await openChatScrolledUp();
	expect(arrow(view)).not.toBeNull();
	expect(view.queryByText("New text below")).toBeNull();

	setEntries([{ id: "a", kind: "agent", text: "Latest answer, now longer" }]);
	await waitFor(() => expect(view.getByText("New text below")).toBeTruthy());
	expect(arrow(view)).toBeNull();

	fireEvent.click(view.getByText("New text below"));
	await waitFor(() => expect(view.queryByText("New text below")).toBeNull());
	expect(arrow(view)).toBeNull(); // back at the bottom: neither control
});

// Catches: the badge preference off leaving the user with no jump control at all.
it("keeps the arrow when the New text below badge is disabled", async () => {
	transcriptPreferences.setEnabled(false);
	const view = await openChatScrolledUp();
	setEntries([{ id: "a", kind: "agent", text: "Latest answer, now longer" }]);
	await Promise.resolve();
	expect(view.queryByText("New text below")).toBeNull();
	expect(arrow(view)).not.toBeNull();
});
