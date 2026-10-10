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

describe("mobile history editing", () => {
	it.each([
		["↑", "\x1b[A"],
		["↓", "\x1b[B"],
	])(
		"keeps explicitly recalled %s history editable instead of appending typing to an invisible command",
		async (key, sequence) => {
			const view = mountSession();
			fireEvent.click(view.getByRole("button", { name: key }));
			await Promise.resolve();
			receiveLine!("echo old");
			const composer = view.getByRole("textbox") as HTMLTextAreaElement;
			expect(composer.value).toBe("echo old");
			fireEvent.input(composer, { target: { value: "echo new" } });
			expect(composer.value).toBe("echo new");
			expect(writes()).toEqual([sequence, "\x7f\x7f\x7fnew"]);
			// A repeated history snapshot must still be handled when explicitly
			// requested after local edits (the transport text itself is unchanged).
			fireEvent.click(view.getByRole("button", { name: key }));
			await Promise.resolve();
			receiveLine!("echo old");
			expect(composer.value).toBe("echo old");
		},
	);

	it("accepts an explicitly requested empty history line and resets the delta baseline", async () => {
		const view = mountSession();
		const composer = view.getByRole("textbox") as HTMLTextAreaElement;
		fireEvent.input(composer, { target: { value: "echo draft" } });
		fireEvent.click(view.getByRole("button", { name: "↓" }));
		await Promise.resolve();
		receiveLine!("");
		expect(composer.value).toBe("");
		fireEvent.input(composer, { target: { value: "pwd" } });
		expect(writes()).toEqual(["echo draft", "\x1b[B", "pwd"]);
	});

	it("does not recreate sent history from a delayed input_line after the send guard expires", async () => {
		const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
		const view = mountSession();
		fireEvent.click(view.getByRole("button", { name: "↑" }));
		await Promise.resolve();
		receiveLine!("echo old");
		const composer = view.getByRole("textbox") as HTMLTextAreaElement;
		fireEvent.click(view.getByRole("button", { name: "Send" }));
		expect(composer.value).toBe("");
		receiveLine!(null);
		now.mockReturnValue(2_000);
		receiveLine!("echo old");
		expect(composer.value).toBe("");
		await waitFor(() => expect(writes()).toEqual(["\x1b[A", "\r"]));
	});
});
