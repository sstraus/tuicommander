import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { rpc } from "../../transport";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";

vi.mock("../../transport", () => ({
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	rpc: vi.fn().mockResolvedValue(undefined),
	HttpRpcError: class extends Error {},
}));
vi.mock("../useMobileVoice", () => ({
	useMobileVoice: () => ({
		available: () => false,
		armed: () => false,
		phase: () => undefined,
		toggle: async () => {},
	}),
}));

// Supply only the transport's public input-line callback. The session screen,
// keybar, composer, request wiring and delta calculation are all production code.
let receiveLine: ((text: string | null) => void) | undefined;
vi.mock("../components/OutputView", () => ({
	OutputView: (props: { onInputLine?: (text: string | null) => void }) => {
		receiveLine = props.onInputLine;
		return <div>Terminal output</div>;
	},
}));

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
	vi.restoreAllMocks();
	receiveLine = undefined;
});

function mountSession() {
	return render(() => (
		<SessionDetailScreen
			session={{
				session_id: "history-session",
				cwd: "/repo",
				worktree_path: null,
				worktree_branch: null,
				state: { agent_type: "claude", awaiting_input: false, rate_limited: false, last_activity_ms: 1 },
			}}
			sessionExists={true}
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
}

const writes = () =>
	vi
		.mocked(rpc)
		.mock.calls.filter(([command]) => command === "write_pty")
		.map(([, args]) => args?.data);

describe("history response ordering", () => {
	it("does not adopt a delayed voice echo as history while the Up write is still pending", async () => {
		let finishHistoryWrite: (() => void) | undefined;
		vi.mocked(rpc).mockImplementation((command, args) => {
			if (command === "write_pty" && args?.data === "\x1b[A") {
				return new Promise<void>((resolve) => {
					finishHistoryWrite = resolve;
				});
			}
			return Promise.resolve(undefined);
		});
		const view = mountSession();
		const composer = view.getByRole("textbox") as HTMLTextAreaElement;
		fireEvent.click(view.getByRole("button", { name: "↑" }));
		// Already in flight before Up: the hands-free payload is not its response.
		receiveLine!("Computer, approva tu quelle");
		const valueBeforeHistoryWrite = composer.value;
		finishHistoryWrite!();
		await Promise.resolve();
		expect(valueBeforeHistoryWrite).toBe("");
		receiveLine!("echo old");
		expect(composer.value).toBe("echo old");
		fireEvent.input(composer, { target: { value: "echo new" } });
		fireEvent.click(view.getByRole("button", { name: "Send" }));
		await waitFor(() => expect(writes()).toEqual(["\x1b[A", "\x7f\x7f\x7fnew", "\r"]));
	});
});
