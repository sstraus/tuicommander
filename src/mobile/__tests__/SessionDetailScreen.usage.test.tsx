import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import question from "../__fixtures__/egoQuestion.json";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import type { ChoicePrompt, SessionInfo } from "../useSessions";

vi.mock("../../transport", () => ({
	subscribeEvents: vi.fn(async () => () => {}),
	rpc: vi.fn().mockResolvedValue(undefined),
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	HttpRpcError: class extends Error {},
}));
vi.mock("../components/OutputView", () => ({ OutputView: () => <div>Terminal output</div> }));
vi.mock("../useMobileVoice", () => ({
	useMobileVoice: () => ({
		phase: () => null,
		available: () => false,
		armed: () => false,
		recording: () => false,
	}),
}));

import { rpc } from "../../transport";

// Solid's development build warns when this isolated screen's lazy updates
// run after the event owner. Capture and validate this known fixture warning;
// unexpected warnings still fail rather than leaking raw stderr.
let warnings: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
	warnings = vi.spyOn(console, "warn").mockImplementation(() => {});
});
afterEach(() => {
	cleanup();
	for (const args of warnings.mock.calls) {
		expect(args).toEqual(["computations created outside a `createRoot` or `render` will never be disposed"]);
	}
	warnings.mockRestore();
	vi.clearAllMocks();
});

const session: SessionInfo = {
	session_id: "ego-question",
	cwd: "/repo",
	worktree_path: null,
	worktree_branch: null,
	state: {
		agent_type: "ego",
		shell_state: "idle",
		awaiting_input: true,
		question_confident: true,
		question_text: question.title,
		rate_limited: false,
		last_activity_ms: 0,
		choice_prompt: question as ChoicePrompt,
	},
};

// Catches: the old warning row presents an unknown quota as 0%.
it("unavailable usage never renders the warning percentage as a measured zero", async () => {
	vi.mocked(rpc).mockImplementation(async (command) =>
		command === "get_session_usage"
			? { availability: "unavailable", reason: "unsupported agent", lines: [] }
			: undefined,
	);
	const view = render(() => (
		<SessionDetailScreen
			subscribeChatWake={() => () => {}}
			session={{ ...session, state: { ...session.state!, usage_limit_pct: 0 } }}
			sessionExists
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
	fireEvent.click(view.getByRole("button", { name: "More session actions" }));
	await waitFor(() => expect(view.getByText("Usage not available")).toBeTruthy());
	expect(view.container.textContent).not.toContain("0%");
});

// Catches: a slow response from a previous PTY overwrites the selected session.
it("discards usage responses for a previous session", async () => {
	let resolveFirst!: (value: unknown) => void;
	vi.mocked(rpc).mockImplementation(async (command, args) => {
		if (command !== "get_session_usage") return undefined;
		if (args?.sessionId === session.session_id)
			return new Promise((resolve) => {
				resolveFirst = resolve;
			});
		return {
			availability: "available",
			model: "current/model",
			lines: [{ label: "Session usage", value: "24 output" }],
		};
	});
	const [current, setCurrent] = createSignal(session);
	const view = render(() => (
		<SessionDetailScreen
			subscribeChatWake={() => () => {}}
			session={current()}
			sessionExists
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
	fireEvent.click(view.getByRole("button", { name: "More session actions" }));
	await waitFor(() => expect(resolveFirst).toBeTypeOf("function"));
	setCurrent({ ...session, session_id: "next-session" });
	await waitFor(() => expect(view.getByText("Model: current/model")).toBeTruthy());
	resolveFirst({
		availability: "available",
		model: "sibling/model",
		lines: [{ label: "Session usage", value: "999 output" }],
	});
	await Promise.resolve();
	expect(view.container.textContent).not.toContain("sibling/model");
	expect(view.container.textContent).not.toContain("999 output");
});
