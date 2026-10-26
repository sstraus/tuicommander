// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import { MobileToastContainer } from "../components/MobileToastContainer";
import { FilesScreen } from "../screens/FilesScreen";
import type { SessionInfo } from "../useSessions";

vi.mock("../../transport", () => ({ rpc: vi.fn(), isTauri: () => false }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));
let disk: string;
let enqueue: () => Promise<{ typed: boolean; queued: number }>;
const agent = (id: string, root = "/repo"): SessionInfo => ({
	session_id: id,
	cwd: root,
	worktree_path: root,
	worktree_branch: null,
	display_name: id,
	state: { agent_type: "ego", awaiting_input: false, rate_limited: false, last_activity_ms: 0 },
});
beforeEach(() => {
	disk = "# Review\n\n- [ ] Confirm release\n\nShip on Friday.\n";
	enqueue = async () => ({ typed: true, queued: 0 });
	vi.mocked(rpc).mockReset();
	vi.mocked(rpc).mockImplementation(async (command, args) => {
		if (command === "load_repositories") return { repos: { "/repo": {} } };
		if (command === "resolve_terminal_path") return { absolute_path: "/repo/review.md", is_directory: false };
		if (command === "stat_path") return { exists: true, is_dir: false, size: 100 };
		if (command === "fs_read_file") return disk;
		if (command === "write_file_if_unchanged") {
			if (args?.expected !== disk) return false;
			disk = String(args.content);
			return true;
		}
		if (command === "enqueue_agent_command") return enqueue();
		throw new Error(`Unexpected command ${command}`);
	});
});
afterEach(() => {
	cleanup();
	for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	vi.useRealTimers();
});
async function open(openerTuicSession?: string) {
	const view = render(() => (
		<>
			<MobileToastContainer />
			<FilesScreen
				sessions={[agent("other"), agent("origin"), agent("foreign", "/elsewhere")]}
				initialSessionId="origin"
				openerTuicSession={openerTuicSession}
				initialRepo={{ cwd: "/repo", worktreePath: "/repo" }}
				initialLink={{ candidate: "review.md" }}
			/>
		</>
	));
	await waitFor(() => expect(view.getByRole("checkbox")).toBeTruthy());
	return view;
}
const sends = () => vi.mocked(rpc).mock.calls.filter(([command]) => command === "enqueue_agent_command");

it("keeps the viewing-session fallback when the document opener is unavailable", async () => {
	// Catches: an unavailable opener changes the existing fallback from the viewing session to the first agent.
	const view = await open("missing-opener");
	await fireEvent.click(view.getByRole("checkbox"));
	expect(await waitFor(() => view.getByRole("combobox", { name: "Review agent" }))).toHaveValue("origin");
});

it("sends persisted checkbox and tweak edits with the absolute path and comment count to the originating agent", async () => {
	// Catches: send drops the absolute path, ignores checkbox edits, or targets a foreign repository.
	const view = await open();
	await fireEvent.click(view.getByRole("checkbox"));
	await waitFor(() => expect(disk).toContain("- [x] Confirm release"));
	await waitFor(() => expect(view.getByText("Ship on Friday.").hasAttribute("data-comment-source-start")).toBe(true));
	await fireEvent.click(view.getByText("Ship on Friday."), { clientX: 100, clientY: 100 });
	await fireEvent.click(view.getByRole("button", { name: "Comment" }));
	await fireEvent.input(view.getByRole("textbox", { name: "Comment text" }), { target: { value: "Monday instead" } });
	await fireEvent.click(view.getByRole("button", { name: "Save comment" }));
	await waitFor(() => expect(disk).toContain("Monday instead-->"));
	expect(view.getByRole("combobox", { name: "Review agent" })).toHaveValue("origin");
	expect(view.queryByRole("option", { name: "foreign" })).toBeNull();
	await fireEvent.click(view.getByRole("button", { name: "Send changes to agent" }));
	await waitFor(() => expect(view.getByText("The review request was delivered now.")).toBeTruthy());
	expect(sends()).toEqual([
		[
			"enqueue_agent_command",
			{
				sessionId: "origin",
				text: "Open /repo/review.md, re-read the whole file: apply the 1 embedded tweak review comment, treat every other change since your last write (checkbox toggles, edited text) as the user's answer, remove each resolved tweak marker, and leave unrelated files unchanged.",
			},
		],
	]);
	expect(view.queryByRole("button", { name: "Send changes to agent" })).toBeNull();
});

it("sends checkbox-only changes to a picked peer and reports the busy queue outcome", async () => {
	// Catches: checkbox-only reviews cannot send, or the selected peer and queue result are ignored.
	enqueue = async () => ({ typed: false, queued: 1 });
	const view = await open();
	await fireEvent.click(view.getByRole("checkbox"));
	const select = await waitFor(() => view.getByRole("combobox", { name: "Review agent" }));
	await fireEvent.change(select, { target: { value: "other" } });
	await fireEvent.click(view.getByRole("button", { name: "Send changes to agent" }));
	await waitFor(() =>
		expect(view.getByText("The review request is queued for the agent's next idle window.")).toBeTruthy(),
	);
	expect(sends()).toHaveLength(1);
	expect(sends()[0][1]).toEqual({
		sessionId: "other",
		text: "Open /repo/review.md, re-read the whole file: treat every other change since your last write (checkbox toggles, edited text) as the user's answer, and leave unrelated files unchanged.",
	});
});

it("retains review changes after a rejected send and prevents duplicate in-flight sends", async () => {
	// Catches: optimistic clearing loses retryable edits, or double taps enqueue twice.
	let reject!: (error: Error) => void;
	enqueue = () =>
		new Promise((_, fail) => {
			reject = fail;
		});
	const view = await open();
	await fireEvent.click(view.getByRole("checkbox"));
	const send = await waitFor(() => view.getByRole("button", { name: "Send changes to agent" }));
	await fireEvent.click(send);
	await fireEvent.click(send);
	expect(send).toBeDisabled();
	expect(sends()).toHaveLength(1);
	reject(new Error("offline"));
	await waitFor(() => expect(view.getByRole("alert").textContent).toBe("Couldn't send changes"));
	expect(view.getByRole("button", { name: "Send changes to agent" })).not.toBeDisabled();
	expect(disk).toContain("- [x] Confirm release");
});

it("keeps checkbox edits made during a send available for the next review", async () => {
	// Catches: a successful older request acknowledges a newer checkbox edit.
	let finish!: (result: { typed: boolean; queued: number }) => void;
	enqueue = () =>
		new Promise((resolve) => {
			finish = resolve;
		});
	const view = await open();
	await fireEvent.click(view.getByRole("checkbox"));
	await fireEvent.click(await waitFor(() => view.getByRole("button", { name: "Send changes to agent" })));
	await fireEvent.click(view.getByRole("checkbox"));
	await waitFor(() => expect(disk).toContain("- [~] Confirm release"));
	finish({ typed: false, queued: 1 });
	await waitFor(() => expect(view.getByRole("button", { name: "Send changes to agent" })).not.toBeDisabled());
	expect(sends()).toHaveLength(1);
});

it("includes agents launched from a repository subdirectory but excludes nested repositories", async () => {
	// Catches: cwd-only sessions lose the originating agent, or a nested repository receives the review.
	const origin = { ...agent("origin"), cwd: "/repo/src", worktree_path: null };
	const nested = { ...agent("nested"), cwd: "/repo/nested/src", worktree_path: null };
	const base = vi.mocked(rpc).getMockImplementation()!;
	vi.mocked(rpc).mockImplementation(async (command, args) =>
		command === "load_repositories" ? { repos: { "/repo": {}, "/repo/nested": {} } } : base(command, args),
	);
	const view = render(() => (
		<FilesScreen
			sessions={[origin, nested]}
			initialSessionId="origin"
			initialRepo={{ cwd: "/repo/src", worktreePath: null }}
			initialLink={{ candidate: "../review.md" }}
		/>
	));
	await fireEvent.click(await waitFor(() => view.getByRole("checkbox")));
	expect(await waitFor(() => view.getByRole("combobox", { name: "Review agent" }))).toHaveValue("origin");
	expect(view.queryByRole("option", { name: "nested" })).toBeNull();
});

it("removes successful queue feedback after the toast lifetime instead of leaving stale status", async () => {
	// Catches: a queued review notice remains on screen after transient feedback expires.
	enqueue = async () => ({ typed: false, queued: 1 });
	const view = await open();
	await fireEvent.click(view.getByRole("checkbox"));
	const send = await waitFor(() => view.getByRole("button", { name: "Send changes to agent" }));
	vi.useFakeTimers();
	await fireEvent.click(send);
	await vi.advanceTimersByTimeAsync(0);
	const message = "The review request is queued for the agent's next idle window.";
	expect(view.getByText(message)).toBeTruthy();
	await vi.advanceTimersByTimeAsync(20_000);
	expect(view.queryByText(message)).toBeNull();
	expect(view.queryByText("Sent to agent")).toBeNull();
	expect(view.getByText("Ship on Friday.")).toBeTruthy();
});
