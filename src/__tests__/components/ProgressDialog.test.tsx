import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProgressDialog } from "../../components/ProgressDialog";
import { progressStore } from "../../stores/progress";

const { close, deleteEntries, open, selectPty, setBlockedOnly, setView, viewState, flowState, projectState, defaults } =
	vi.hoisted(() => {
		const entry = (id: number, type: string, text: string, createdAtMs: number) => ({
			id,
			project: "/repo",
			createdAtMs,
			type,
			text,
			agentName: "Worker",
		});
		const defaults = () => ({
			entries: [
				entry(3, "blocked", "Cannot reach the registry.", 300),
				entry(2, "intent", "Rewriting the dialog.", 200),
				entry(1, "done", "Shipped the store.", 100),
			],
			ptyIds: ["pty-a", "pty-b"],
			// Everything at or below this timestamp was on screen last visit.
			dividerMs: 200,
			loading: false,
			error: "one line for the whole failure",
		});
		return {
			close: vi.fn(),
			deleteEntries: vi.fn(),
			open: vi.fn(),
			selectPty: vi.fn(),
			setBlockedOnly: vi.fn(),
			setView: vi.fn(),
			viewState: { current: "list" as "list" | "flow" },
			flowState: {} as Record<string, unknown>,
			projectState: defaults() as ReturnType<typeof defaults>,
			defaults,
		};
	});

vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));

vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { get: () => ({ displayName: "Example" }) },
}));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		state: {
			terminals: {
				a: { sessionId: "pty-a", repoPath: "/repo", name: "Agent A" },
				b: { sessionId: "pty-b", repoPath: "/repo", name: "Agent B" },
			},
		},
	},
}));

vi.mock("../../stores/progress", () => ({
	progressStore: {
		requestedProject: () => "/repo",
		selectedPtyId: () => "pty-a",
		selectPty,
		blockedOnly: () => false,
		setBlockedOnly,
		open,
		close,
		deleteEntries,
		view: () => viewState.current,
		setView,
		fetchFlowDetail: vi.fn(),
		state: { projects: { "/repo": projectState }, flows: flowState },
	},
}));

beforeEach(() => {
	Object.assign(projectState, defaults());
	viewState.current = "list";
	for (const key of Object.keys(flowState)) delete flowState[key];
	vi.clearAllMocks();
});

describe("ProgressDialog", () => {
	it("lets the reader switch PTYs and see the whole repository", () => {
		render(() => <ProgressDialog />);
		// Removing the mobile subtitle must not remove desktop's only project label.
		expect(screen.getAllByText("Example")).toHaveLength(1);
		expect(screen.queryByLabelText("Progress project")).toBeNull();
		const selector = screen.getByLabelText("Progress terminal") as HTMLSelectElement;
		expect(selector.value).toBe("pty-a");
		expect(Array.from(selector.options).map((option) => option.text)).toEqual(["All repo", "Agent A", "Agent B"]);
		fireEvent.change(selector, { target: { value: "pty-b" } });
		expect(selectPty).toHaveBeenCalledWith("pty-b");
		fireEvent.change(selector, { target: { value: "" } });
		expect(selectPty).toHaveBeenCalledWith(null);
	});
	it("renders one newest-first list with the last-visit divider above the first old entry", () => {
		render(() => <ProgressDialog />);

		const texts = screen.getAllByText(/registry|dialog|store/).map((node) => node.textContent);
		expect(texts).toEqual(["Cannot reach the registry.", "Rewriting the dialog.", "Shipped the store."]);

		// dividerMs is 200, so entry 2 is the first one already seen.
		const rows = Array.from(document.querySelectorAll("[data-kind], [class*='divider']"));
		expect(rows.map((row) => row.getAttribute("data-kind") ?? "divider")).toEqual([
			"blocked",
			"divider",
			"intent",
			"done",
		]);
	});

	it("labels an intent as an intention rather than an outcome", () => {
		render(() => <ProgressDialog />);

		const intent = document.querySelector('[data-kind="intent"]');
		expect(intent?.textContent).toContain("set out to");
		expect(document.querySelector('[data-kind="done"]')?.textContent).not.toContain("set out to");
	});

	it("reads an unreadable journal as unavailable, never as an empty or finished project", () => {
		projectState.entries = [];

		render(() => <ProgressDialog />);

		expect(screen.getByText("This project's journal is unavailable.")).toBeTruthy();
		expect(screen.queryByText(/No progress recorded/)).toBeNull();
		expect(screen.queryByText(/Nothing is blocked/)).toBeNull();
	});

	it("says nothing was recorded for the selected terminal only when the read succeeded", () => {
		projectState.entries = [];
		projectState.error = null as unknown as string;

		render(() => <ProgressDialog />);

		expect(screen.getByText("No progress recorded for this terminal yet.")).toBeTruthy();
	});

	it("shows one aggregated failure line and keeps the entries visible", () => {
		render(() => <ProgressDialog />);

		expect(screen.getAllByText("one line for the whole failure")).toHaveLength(1);
		expect(screen.getAllByText("Shipped the store.")).toHaveLength(1);
	});

	it("deletes the entry the button belongs to", () => {
		render(() => <ProgressDialog />);

		fireEvent.click(screen.getAllByLabelText("Delete entry")[1]);
		expect(deleteEntries).toHaveBeenCalledWith("/repo", [2]);
	});

	it("marks the visit as ended when the backdrop is clicked", () => {
		const { container } = render(() => <ProgressDialog />);

		// The overlay is the dialog's parent; the dialog itself stops the event.
		fireEvent.click(container.firstElementChild as HTMLElement);
		expect(close).toHaveBeenCalled();
	});

	it("embedded drops the overlay and asks the store to open the active project", () => {
		close.mockClear();
		const { container } = render(() => <ProgressDialog embedded />);

		expect(container.querySelector("[role='region']")).not.toBeNull();
		expect(screen.queryByLabelText("Close")).toBeNull();
		fireEvent.click(container.firstElementChild as HTMLElement);
		expect(close).not.toHaveBeenCalled();
	});

	it("offers exactly one blocked-only toggle", () => {
		render(() => <ProgressDialog />);

		const toggles = document.querySelectorAll("input[type='checkbox']");
		expect(toggles).toHaveLength(1);
		fireEvent.click(toggles[0]);
		expect(setBlockedOnly).toHaveBeenCalledWith(true);
	});
});

describe("ProgressDialog wiring", () => {
	it("uses the store the app exports", () => {
		expect(progressStore.requestedProject()).toBe("/repo");
	});

	it("switches between the List and the Flow view", () => {
		render(() => <ProgressDialog />);
		fireEvent.click(screen.getByRole("tab", { name: "Flow" }));
		expect(setView).toHaveBeenCalledWith("flow");
		expect(screen.getByRole("tab", { name: "List" }).getAttribute("aria-selected")).toBe("true");
	});

	it("draws the Flow view from the backend's sequence and hides the list filter", () => {
		viewState.current = "flow";
		flowState["/repo"] = {
			loading: false,
			error: null,
			data: {
				project: "/repo",
				truncated: false,
				participants: [
					{ id: "pty-a", kind: "terminal", title: "Lead", state: "busy", toolCalls: 0, ptyId: "pty-a" },
					{
						id: "pty-b",
						kind: "terminal",
						title: "Worker",
						state: "closed",
						parent: "pty-a",
						toolCalls: 0,
						ptyId: "pty-b",
					},
				],
				events: [{ kind: "delegated", from: "pty-a", to: "pty-b", summary: "Write the lexer", atMs: 1 }],
			},
		};
		render(() => <ProgressDialog />);
		expect(screen.queryByText("Blocked only")).toBeNull();
		expect(screen.getByText("Lead")).toBeTruthy();
		expect(screen.getByText("Write the lexer")).toBeTruthy();
	});

	it("names the target of a hand-off in the List view", () => {
		projectState.entries = [
			{
				id: 9,
				project: "/repo",
				createdAtMs: 400,
				type: "delegated",
				text: "Write the lexer",
				agentName: "Lead",
				targetPtyId: "pty-b",
				targetName: "lexer",
			},
		] as unknown as typeof projectState.entries;
		render(() => <ProgressDialog />);
		expect(screen.getByText(/delegated to\s+Agent B/)).toBeTruthy();
	});
});
