import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProgressDialog } from "../../components/ProgressDialog";
import { progressStore } from "../../stores/progress";

const { invoke } = vi.hoisted(() => ({
	invoke: vi.fn(async (command: string, args?: Record<string, unknown>) => {
		if (command !== "progress_list") return {};
		const project = args?.project;
		const text =
			project === "/long"
				? Array.from({ length: 12 }, (_, n) => `Result line ${n + 1}`).join("\n")
				: project === "/newer"
					? "Later work is blocked."
					: "Earlier work shipped.";
		return {
			project,
			entries: [
				{
					id: project === "/newer" ? 2 : 1,
					project,
					createdAtMs: 100,
					type: project === "/newer" ? "blocked" : "done",
					text,
				},
			],
			ptyIds: [],
		};
	}),
}));
vi.mock("../../invoke", () => ({ invoke }));

beforeEach(() => {
	progressStore.resetForTests();
	invoke.mockClear();
});
afterEach(() => {
	cleanup();
	progressStore.resetForTests();
});

describe("mobile Progress", () => {
	it.each([{ projects: ["/newer"] }, { projects: ["/newer", "/older"] }, { projects: [] }, { projects: undefined }])(
		"keeps the selected project name once with or without a mobile selector ($projects)",
		async ({ projects }) => {
			// Catch both a duplicate subtitle and a lost label while project discovery is unavailable.
			progressStore.open("/newer", null);
			render(() => <ProgressDialog embedded projects={projects} />);
			await waitFor(() => expect(screen.getByText("Later work is blocked.")).toBeTruthy());
			expect(screen.queryByLabelText("Progress project") !== null).toBe(!!projects?.length);
			expect(screen.getAllByText("newer")).toHaveLength(1);
		},
	);

	// Catches losing access to operational events or filtering them only after paging in the UI.
	it("defaults to backend human-progress filtering and exposes event and terminal filters", async () => {
		render(() => <ProgressDialog embedded projects={["/newer"]} />);
		await waitFor(() => expect(screen.getByText("Later work is blocked.")).toBeTruthy());
		expect(invoke).toHaveBeenLastCalledWith("progress_list", {
			project: "/newer",
			input: { progressOnly: true, blockedOnly: false, limit: 100 },
		});
		expect(screen.queryByLabelText("Progress terminal")).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Filters" }));
		expect(screen.getByLabelText("Progress terminal")).toBeTruthy();
		fireEvent.click(screen.getByRole("switch", { name: "Messages and delegations" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenLastCalledWith("progress_list", {
				project: "/newer",
				input: { blockedOnly: false, limit: 100 },
			}),
		);
		fireEvent.click(screen.getByRole("button", { name: "Entry actions" }));
		expect(screen.getByRole("button", { name: "Delete entry" })).toBeTruthy();
		expect(screen.getByText("Blocked")).toBeTruthy();
	});

	it("opens the newest populated project and lets Boss read another project's entries", async () => {
		render(() => <ProgressDialog embedded projects={["/newer", "/older"]} />);
		await waitFor(() => expect(screen.getByText("Later work is blocked.")).toBeTruthy());
		const picker = screen.getByLabelText("Progress project") as HTMLSelectElement;
		expect(picker.value).toBe("/newer");
		fireEvent.change(picker, { target: { value: "/older" } });
		await waitFor(() => expect(screen.getByText("Earlier work shipped.")).toBeTruthy());
		expect(screen.queryByText("Later work is blocked.")).toBeNull();
	});

	it("explains the empty project list instead of claiming the journal is empty", () => {
		render(() => <ProgressDialog embedded projects={[]} />);
		expect(screen.getByText("No project with Progress entries is available.")).toBeTruthy();
		expect(screen.queryByText("No progress recorded for this project yet.")).toBeNull();
	});

	it("does not claim an empty journal while the project list is loading", () => {
		render(() => <ProgressDialog embedded projects={undefined} />);
		expect(screen.getByText("Loading Progress projects…")).toBeTruthy();
		expect(screen.queryByText("No progress recorded for this project yet.")).toBeNull();
	});

	it("shows project discovery failure instead of a false empty journal", () => {
		render(() => <ProgressDialog embedded projects={undefined} projectsError="Connection lost" />);
		expect(screen.getByText("Progress projects are unavailable.")).toBeTruthy();
		expect(screen.queryByText("No progress recorded for this project yet.")).toBeNull();
	});

	it("collapses a long mobile progress message until More is pressed", async () => {
		render(() => <ProgressDialog embedded projects={["/long"]} />);
		await waitFor(() => expect(screen.getByText(/Result line 12/)).toBeTruthy());
		const more = screen.getByRole("button", { name: "More" });
		expect(more.getAttribute("aria-expanded")).toBe("false");
		fireEvent.click(more);
		expect(screen.getByRole("button", { name: "Less" }).getAttribute("aria-expanded")).toBe("true");
	});
});
