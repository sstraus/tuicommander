// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const { invoke, rpc } = vi.hoisted(() => ({ invoke: vi.fn(), rpc: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke }));
vi.mock("../../transport", async (original) => ({ ...(await original<typeof import("../../transport")>()), rpc }));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn(),
	emitTo: vi.fn(),
}));

import { MarkdownTab } from "../../components/MarkdownTab/MarkdownTab";
import { appLogger } from "../../stores/appLogger";
import { type FileTab, mdTabsStore } from "../../stores/mdTabs";
import { terminalsStore } from "../../stores/terminals";
import { setToastBellMirrorResolver, toastsStore } from "../../stores/toasts";

const initial = "# Review\n\n- [ ] First\n\nKeep this paragraph.\n\n- [ ] Second\n";
let disk: string;
let expectedBaseline: string;
beforeEach(() => {
	setToastBellMirrorResolver(() => false);
	disk = initial;
	expectedBaseline = initial;
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
	invoke.mockReset();
	rpc.mockReset().mockResolvedValue({ typed: false, queued: 1 });
	invoke.mockImplementation(async (command, args) => {
		if (command === "read_file" || command === "read_external_file") return disk;
		if (command === "write_file") {
			disk = args.content;
			return;
		}
		if (command === "markdown_review_changed_lines") {
			return args.baseline === expectedBaseline && args.expected === disk ? "3, 7" : null;
		}
		throw new Error(`Unexpected command ${command}`);
	});
	const id = terminalsStore.add({
		name: "Reviewer",
		sessionId: "reviewer",
		fontSize: 14,
		cwd: "/repo",
		awaitingInput: null,
	});
	terminalsStore.update(id, { agentType: "ego", repoPath: "/repo" });
});
afterEach(() => {
	cleanup();
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	mdTabsStore.clearAll();
	vi.restoreAllMocks();
	setToastBellMirrorResolver(() => true);
});
async function open() {
	const id = mdTabsStore.add("/repo", "review.md");
	const view = render(() => <MarkdownTab tab={mdTabsStore.get(id) as FileTab} />);
	await waitFor(() => expect(view.getAllByRole("checkbox")).toHaveLength(2));
	return view;
}
const prompts = () =>
	rpc.mock.calls.filter(([command]) => command === "enqueue_agent_command").map(([, args]) => args.text);

// Catches: the second save replaces the baseline and drops the first checkbox edit.
it("retains the desktop baseline across multiple saves and a failed send", async () => {
	const view = await open();
	await fireEvent.click(view.getAllByRole("checkbox")[0]);
	await waitFor(() => expect(view.getAllByRole("checkbox")[0]).toBeChecked());
	await fireEvent.click(view.getAllByRole("checkbox")[1]);
	await waitFor(() => expect(view.getAllByRole("checkbox")[1]).toBeChecked());
	const error = vi.spyOn(appLogger, "error").mockImplementation(() => {});
	rpc.mockRejectedValueOnce(new Error("offline"));
	await fireEvent.click(view.getByRole("button", { name: "Send changes to agent" }));
	await waitFor(() => expect(error).toHaveBeenCalled());
	await fireEvent.click(view.getByRole("button", { name: "Send changes to agent" }));
	await waitFor(() => expect(prompts()).toHaveLength(2));
	expect(prompts()).toEqual([
		expect.stringContaining("read changed lines 3, 7 with their enclosing sections"),
		expect.stringContaining("read changed lines 3, 7 with their enclosing sections"),
	]);
	await waitFor(() => expect(view.queryByRole("button", { name: "Send changes to agent" })).toBeNull());
});

// Catches: a disk reload after a local save leaves stale selective-read coordinates.
it("invalidates desktop ranges after an external reload", async () => {
	const [reload, setReload] = (await import("solid-js")).createSignal(0);
	const id = mdTabsStore.add("/repo", "review.md");
	const view = render(() => <MarkdownTab tab={mdTabsStore.get(id) as FileTab} reloadToken={reload} />);
	await fireEvent.click(await waitFor(() => view.getAllByRole("checkbox")[0]));
	await waitFor(() => expect(view.getAllByRole("checkbox")[0]).toBeChecked());
	disk = "# External replacement\n\n" + disk;
	setReload(1);
	await waitFor(() => expect(view.getByText("External replacement")).toBeTruthy());
	await fireEvent.click(view.getByRole("button", { name: "Send changes to agent" }));
	await waitFor(() => expect(prompts()).toHaveLength(1));
	expect(prompts()[0]).toContain("re-read the whole file");
	await waitFor(() => expect(view.queryByRole("button", { name: "Send changes to agent" })).toBeNull());
});
