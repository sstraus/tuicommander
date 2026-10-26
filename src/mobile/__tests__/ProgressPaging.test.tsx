import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ProgressDialog } from "../../components/ProgressDialog";
import { progressStore } from "../../stores/progress";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke }));

const history = Array.from({ length: 1000 }, (_, index) => ({
	id: 1000 - index,
	project: "/paging",
	createdAtMs: 1000,
	type: "done",
	text: `Paging result ${1000 - index}`,
}));

beforeEach(() => {
	progressStore.resetForTests();
	invoke.mockReset();
	invoke.mockImplementation(async (command, args) => {
		if (command !== "progress_list") return {};
		const candidates = history.filter((entry) => args.input.cursor == null || entry.id < args.input.cursor);
		const entries = candidates.slice(0, args.input.limit);
		return {
			project: args.project,
			entries,
			total: history.length,
			nextCursor: candidates.length > entries.length ? entries.at(-1)?.id : null,
			ptyIds: [],
		};
	});
});
afterEach(() => {
	cleanup();
	progressStore.resetForTests();
});

// Catches: draining every backend page before painting the mobile list.
it("opens a thousand-record journal with at most 100 fetched and rendered records", async () => {
	const start = performance.now();
	render(() => <ProgressDialog embedded projects={["/paging"]} />);
	await waitFor(() => expect(progressStore.state.projects["/paging"]?.loading).toBe(false));
	const rows = document.querySelectorAll("[data-kind]").length;
	console.info(
		`Progress fixture open: ${(performance.now() - start).toFixed(1)}ms, ${invoke.mock.calls.length} reads, ${rows} rows`,
	);
	expect(invoke).toHaveBeenCalledTimes(1);
	expect(rows).toBe(100);
	expect(screen.queryByText("Paging result 900")).toBeNull();
});

// Catches: an older-page action re-fetching the newest page or appending twice.
it("appends the exclusive older page once even when Load older is pressed twice", async () => {
	render(() => <ProgressDialog embedded projects={["/paging"]} />);
	await waitFor(() => expect(progressStore.state.projects["/paging"]?.loading).toBe(false));
	const load = screen.getByRole("button", { name: "Load older" });
	fireEvent.click(load);
	fireEvent.click(load);
	await waitFor(() => expect(document.querySelectorAll("[data-kind]")).toHaveLength(200));
	expect(invoke).toHaveBeenCalledTimes(2);
	expect(invoke).toHaveBeenLastCalledWith("progress_list", {
		project: "/paging",
		input: { blockedOnly: false, limit: 100, cursor: 901 },
	});
	const texts = Array.from(document.querySelectorAll("[data-kind] p"), (node) => node.textContent);
	expect(texts).toEqual(history.slice(0, 200).map((entry) => entry.text));
	expect(new Set(texts).size).toBe(200);
});
