import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { workflowRunSignals } from "../../stores/workflowRunSignals";
import { HttpRpcError } from "../../transport";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const plan = { id: "p1", project: "/repo", title: "Plan A", source: "plans/a.md" };
const story = {
	id: "s1",
	planId: "p1",
	title: "Implement API",
	criteria: ["API works"],
	checked: [false],
	dependencies: [],
	priority: 1,
	origin: "native",
	fileScope: ["src/api.rs"],
	status: "ready",
	revision: 1,
	claimSession: null,
	abandoned: false,
};

const graph = {
	id: "root",
	targetId: "s1",
	completed: false,
	definition: {
		id: "flow-1",
		revision: 2,
		graph: {
			nodes: [{ id: "pause", kind: { type: "pause" } }],
			edges: [{ from: "judge", to: "pause", outcome: "uncertain" }],
		},
	},
	activations: [{ id: "a3", nodeId: "pause", state: "paused", edgeIndex: null }],
	decisions: [
		{
			activationId: "a2",
			edgeIndex: 0,
			evidence: { actor: "reviewer", reason: "Conflicting evidence", references: ["report:r2"] },
		},
	],
	loops: [{ nodeId: "repair", repeats: 3 }],
	pauses: [
		{ activationId: "a3", resumeTo: "implement", evidence: { reason: "Operator decision needed" }, resolution: null },
	],
};

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		if (_command === "story_capabilities") return true;
		const action = (args as { action: { action: string } }).action;
		if (action.action === "transition_history") return { type: "transitions", value: [] };
		if (action.action === "list_notes") return { type: "notes", value: [] };
		if (action.action === "list_plans") return { type: "plans", value: [plan] };
		if (action.action === "plan_view")
			return { type: "plan_view", value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false } };
		if (action.action === "transition")
			return { type: "story", value: { ...story, status: "in_progress", revision: 2 } };
		throw new Error(`unexpected action ${action.action}`);
	});
});

describe("StoriesDialog", () => {
	it("offers repository plans first and adds one without a typed title", async () => {
		let created = false;
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string; source?: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: created ? [plan] : [] };
			if (action.action === "list_plan_sources")
				return { type: "plan_sources", value: [{ title: "Plan A", source: "plans/a.md" }] };
			if (action.action === "add_plan_source") {
				created = true;
				return { type: "plan", value: plan };
			}
			if (action.action === "plan_view")
				return { type: "plan_view", value: { stories: [], state: "draft", wontFixCount: 0, allCancelled: false } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		fireEvent.click(await screen.findByRole("button", { name: /Plan A/ }));
		await waitFor(() => expect(screen.getByRole("button", { name: "Plan A" })).toBeTruthy());
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "add_plan_source", source: "plans/a.md" },
		});
	});

	it("shows discovered plans immediately when no plan is registered", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [] };
			if (action.action === "list_plan_sources")
				return { type: "plan_sources", value: [{ title: "Agent plan", source: "plans/agent.md" }] };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		expect(await screen.findByRole("button", { name: /Agent plan/ })).toBeTruthy();
	});

	it("rejects an invalid response when adding a discovered plan", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [] };
			if (action.action === "list_plan_sources")
				return { type: "plan_sources", value: [{ title: "Agent plan", source: "plans/agent.md" }] };
			if (action.action === "add_plan_source") return { type: "plans", value: [] };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		fireEvent.click(await screen.findByRole("button", { name: /Agent plan/ }));
		await screen.findByText("Invalid plan response");
	});
	it("moves initial focus into the dialog", async () => {
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		await waitFor(() =>
			expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close Plans and Stories" })),
		);
	});

	it("loads plan and story detail, then starts manual work through the backend", async () => {
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		expect(screen.getByText("API works")).toBeTruthy();
		expect(screen.getByText("src/api.rs")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Start work" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "transition", story_id: "s1", expected_revision: 1, command: "start_manual" },
			}),
		);
	});

	it("shows a recoverable error when loading fails", async () => {
		vi.mocked(invoke).mockResolvedValueOnce(true).mockRejectedValueOnce(new Error("offline"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/offline/);
		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		await screen.findByRole("heading", { name: "Implement API" });
	});

	it("explains how to load the missing native stories backend", async () => {
		vi.stubGlobal("__TAURI_INTERNALS__", {});
		try {
			vi.mocked(invoke).mockRejectedValueOnce("Command story_capabilities not found");
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByText(/Restart TUICommander/);
			expect(invoke).toHaveBeenCalledWith("story_capabilities");
		} finally {
			vi.unstubAllGlobals();
		}
	});

	it("preserves an HTTP domain error even when it says a story was not found", async () => {
		vi.mocked(invoke)
			.mockResolvedValueOnce(true)
			.mockRejectedValueOnce(new HttpRpcError("story_action_command", 500, '{"error":"story not found: s1"}'));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/story not found: s1/);
		expect(screen.queryByText(/Restart TUICommander/)).toBeNull();
	});

	// Catches: the dialog showing "RPC story_action_command failed: 422 {...}" instead of the plain message.
	it("shows only the plain message of a rejected user action", async () => {
		vi.mocked(invoke)
			.mockResolvedValueOnce(true)
			.mockRejectedValueOnce(
				new HttpRpcError("story_action_command", 422, '{"error":"criteria are incomplete: unchecked indexes 1"}'),
			);
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		const alert = await screen.findByText(/criteria are incomplete/);
		expect(alert.textContent).not.toMatch(/RPC|422|\{/);
	});

	it("classifies a missing HTTP capability route by its status", async () => {
		vi.mocked(invoke).mockRejectedValueOnce(new HttpRpcError("story_capabilities", 404, "Not Found"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/Restart TUICommander/);
	});

	it("recognizes an older browser backend serving its HTML fallback", async () => {
		vi.mocked(invoke).mockResolvedValueOnce("<!doctype html><html><body>TUICommander</body></html>");
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/Restart TUICommander/);
		expect(screen.queryByText("Invalid plan response")).toBeNull();
	});

	it("creates a story with criteria and relative file scope", async () => {
		const created = { ...story, id: "s2", title: "Review API" };
		let rows = [story];
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string; input?: unknown } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return { type: "plan_view", value: { stories: rows, state: "active", wontFixCount: 0, allCancelled: false } };
			if (action.action === "create_story") {
				rows = [story, created];
				return { type: "story", value: created };
			}
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "New story" }));
		fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Review API" } });
		fireEvent.input(screen.getByLabelText("Acceptance criteria, one per line"), {
			target: { value: "Review passes\nNo regressions" },
		});
		fireEvent.input(screen.getByLabelText("File scope, one relative path per line"), {
			target: { value: "src/api.rs" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Create story" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: {
					action: "create_story",
					input: {
						planId: "p1",
						title: "Review API",
						criteria: ["Review passes", "No regressions"],
						priority: 2,
						origin: { type: "native" },
						fileScope: ["src/api.rs"],
					},
				},
			}),
		);
	});

	it("sends indexed criterion changes in the backend enum shape", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: {
						stories: [{ ...story, status: "in_progress" }],
						state: "active",
						wontFixCount: 0,
						allCancelled: false,
					},
				};
			if (action.action === "transition") return { type: "story", value: { ...story, status: "in_progress" } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("checkbox", { name: "API works" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "transition", story_id: "s1", expected_revision: 1, command: { check_criterion: 0 } },
			}),
		);
	});

	it("creates a plan and adds an eligible story dependency", async () => {
		let created = false;
		const dependent = { ...story, id: "s2", title: "Review API" };
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: created ? [plan] : [] };
			if (action.action === "list_plan_sources") return { type: "plan_sources", value: [] };
			if (action.action === "add_plan_source") {
				created = true;
				return { type: "plan", value: plan };
			}
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: { stories: [story, dependent], state: "active", wontFixCount: 0, allCancelled: false },
				};
			if (action.action === "add_dependency") return { type: "story", value: { ...story, dependencies: ["s2"] } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText("Create a plan to begin.");
		fireEvent.click(screen.getByRole("button", { name: "New plan" }));
		fireEvent.click(screen.getByText("Add from path or link"));
		fireEvent.input(screen.getByLabelText("Plan document or link"), { target: { value: "plans/a.md" } });
		fireEvent.click(screen.getByRole("button", { name: "Create plan" }));
		await screen.findByRole("heading", { name: "Implement API" });
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "add_plan_source", source: "plans/a.md" },
		});
		fireEvent.change(screen.getByRole("combobox", { name: "Add dependency" }), { target: { value: "s2" } });
		fireEvent.click(screen.getByRole("button", { name: "Add dependency" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "add_dependency", story_id: "s1", dependency_id: "s2", expected_revision: 1 },
			}),
		);
	});

	it("drops a pending dependency choice when another story is selected", async () => {
		const second = { ...story, id: "s2", title: "Review API" };
		const third = { ...story, id: "s3", title: "Ship API" };
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: { stories: [story, second, third], state: "active", wontFixCount: 0, allCancelled: false },
				};
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.change(screen.getByRole("combobox", { name: "Add dependency" }), { target: { value: "s3" } });
		fireEvent.click(screen.getByRole("button", { name: /Review API/ }));
		await screen.findByRole("heading", { name: "Review API" });
		// s3 is also a valid candidate for s2: a leaked choice would stay armed for a story the user never chose it for.
		expect((screen.getByRole("combobox", { name: "Add dependency" }) as HTMLSelectElement).value).toBe("");
		expect((screen.getByRole("button", { name: "Add dependency" }) as HTMLButtonElement).disabled).toBe(true);
	});

	it("logs a failed story action as well as showing it", async () => {
		vi.mocked(invoke).mockResolvedValueOnce(true).mockRejectedValueOnce(new Error("offline"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/offline/);
		expect(appLogger.warn).toHaveBeenCalledWith("store", expect.stringContaining("Stories"), expect.anything());
	});

	it("shows cancelled dependencies and waives only the direct cancelled edge", async () => {
		const cancelled = { ...story, id: "s2", title: "Cancelled API", status: "wont_fix", abandoned: true };
		const intermediate = {
			...story,
			id: "s3",
			title: "Intermediate API",
			status: "backlog",
			dependencies: ["s2"],
			abandoned: true,
		};
		const dependent = {
			...story,
			id: "s4",
			title: "Ship API",
			status: "backlog",
			dependencies: ["s2", "s3"],
			abandoned: true,
		};
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: {
						stories: [cancelled, intermediate, dependent],
						state: "active",
						wontFixCount: 1,
						allCancelled: false,
					},
				};
			if (action.action === "remove_dependency") return { type: "story", value: dependent };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Cancelled API" });
		expect(screen.getByText("1 won't fix")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: /Ship API/ }));
		await screen.findByRole("heading", { name: "Ship API" });
		expect(screen.getAllByText(/abandoned/i)).toHaveLength(2);
		const remove = screen.getByRole("button", { name: /Remove.*Cancelled API/ });
		fireEvent.click(remove);
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "remove_dependency", story_id: "s4", dependency_id: "s2", expected_revision: 1 },
			}),
		);
		expect(screen.queryByRole("button", { name: /Remove.*Intermediate API/ })).toBeNull();
	});

	it("renders the service cancellation summary without recounting story statuses", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return { type: "plan_view", value: { stories: [story], state: "done", wontFixCount: 1, allCancelled: true } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText("All cancelled");
	});

	it("offers browser approval and sends the selected transition", async () => {
		const environment = globalThis as Record<string, unknown>;
		const priorShim = environment.__TAURI_SHIM__;
		environment.__TAURI_SHIM__ = true;
		try {
			vi.mocked(invoke).mockImplementation(async (_command, args) => {
				if (_command === "story_capabilities") return true;
				const action = (args as { action: { action: string } }).action;
				if (action.action === "transition_history") return { type: "transitions", value: [] };
				if (action.action === "list_notes") return { type: "notes", value: [] };
				if (action.action === "list_plans") return { type: "plans", value: [plan] };
				if (action.action === "plan_view")
					return {
						type: "plan_view",
						value: { stories: [{ ...story, status: "review" }], state: "active", wontFixCount: 0, allCancelled: false },
					};
				if (action.action === "transition") return { type: "story", value: { ...story, status: "done" } };
				throw new Error(`unexpected action ${action.action}`);
			});
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByRole("heading", { name: "Implement API" });
			fireEvent.click(screen.getByRole("button", { name: "Approve" }));
			await waitFor(() =>
				expect(invoke).toHaveBeenCalledWith("story_action_command", {
					project: "/repo",
					action: { action: "transition", story_id: "s1", expected_revision: 1, command: "approve" },
				}),
			);
		} finally {
			environment.__TAURI_SHIM__ = priorShim;
		}
	});

	it("offers desktop review actions and sends the selected transition", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: { stories: [{ ...story, status: "review" }], state: "active", wontFixCount: 0, allCancelled: false },
				};
			if (action.action === "transition") return { type: "story", value: { ...story, status: "review" } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Approve" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "transition", story_id: "s1", expected_revision: 1, command: "approve" },
			}),
		);
		await waitFor(() =>
			expect((screen.getByRole("button", { name: "Request changes" }) as HTMLButtonElement).disabled).toBe(false),
		);
		fireEvent.click(screen.getByRole("button", { name: "Request changes" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "transition", story_id: "s1", expected_revision: 1, command: "reject_review" },
			}),
		);
	});

	it("opens the persisted run timeline for the selected plan", async () => {
		const run = {
			id: "r1",
			planId: "p1",
			status: "paused",
			sequence: 4,
			startedMs: 1_000,
			stories: [{ storyId: "s1", accepted: false }],
			attempts: [],
		};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "story_capabilities") return true;
			const action = (args as { action: { action: string; after_sequence?: number } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (command === "workflow_run_action") {
				if (action.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action.action === "get") return { type: "snapshot", value: run };
				if (action.action === "events")
					return {
						type: "events",
						value:
							action.after_sequence === 4
								? []
								: action.after_sequence === 3
									? [{ sequence: 4, atMs: 1_200, kind: { type: "resumed" } }]
									: [
											{ sequence: 1, atMs: 1_000, kind: { type: "started" } },
											{ sequence: 3, atMs: 1_100, kind: { type: "paused" } },
										],
					};
			}
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false },
				};
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo/worktree" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Run history" }));
		await screen.findByRole("region", { name: "Run timeline" });
		await screen.findByText("started");
		expect(screen.getAllByText("paused").length).toBeGreaterThan(0);
		expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
			project: "/repo/worktree",
			action: { action: "list_plan_runs", plan_id: "p1", limit: 20 },
		});
		fireEvent.click(screen.getByRole("button", { name: "Load more events" }));
		await screen.findByText("resumed");
		expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
			project: "/repo/worktree",
			action: { action: "events", run_id: "r1", after_sequence: 3, limit: 100 },
		});
		const reads = vi
			.mocked(invoke)
			.mock.calls.filter(
				([command, args]) =>
					command === "workflow_run_action" && (args as { action: { action: string } }).action.action === "get",
			).length;
		workflowRunSignals.accept({ repo_path: "/repo", payload: { runId: "r1", sequence: 5 } });
		await waitFor(() =>
			expect(
				vi
					.mocked(invoke)
					.mock.calls.filter(
						([command, args]) =>
							command === "workflow_run_action" && (args as { action: { action: string } }).action.action === "get",
					).length,
			).toBeGreaterThan(reads),
		);
	});

	it("records a human answer to a paused workflow request", async () => {
		const run = {
			id: "r1",
			planId: "p1",
			status: "paused",
			sequence: 4,
			startedMs: 1_000,
			stories: [{ storyId: "s1", accepted: false }],
			attempts: [
				{
					id: "a1",
					storyId: "s1",
					nodeId: "implement",
					state: "reported",
					outcome: "needs_input",
					inputAnswer: null,
					report: { inputRequest: { question: "Use A or B?", options: ["A", "B"] } },
				},
			],
		};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (command === "workflow_run_action") {
				if (action.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action.action === "get") return { type: "snapshot", value: run };
				if (action.action === "events") return { type: "events", value: [] };
				if (action.action === "command") return { type: "receipt", value: { snapshot: run } };
			}
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false },
				};
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Run history" }));
		await screen.findByText("Use A or B?");
		fireEvent.input(screen.getByRole("textbox", { name: "Answer" }), { target: { value: "A" } });
		fireEvent.click(screen.getByRole("button", { name: "Record answer" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
				project: "/repo",
				action: {
					action: "command",
					run_id: "r1",
					expected_sequence: 4,
					command_id: expect.any(String),
					command: { action: "answer_input", attempt_id: "a1", answer: "A" },
				},
			}),
		);
	});

	it("resumes a graph pause with an explicit activation and resolution after input is answered", async () => {
		// Catches: status-only Resume bypassing the graph pause target and resolution.
		const run = {
			graphExecutions: [graph],
			id: "r1",
			planId: "p1",
			status: "paused",
			sequence: 5,
			startedMs: 1_000,
			stories: [{ storyId: "s1", accepted: false }],
			attempts: [
				{
					id: "a1",
					storyId: "s1",
					nodeId: "implement",
					state: "reported",
					outcome: "needs_input",
					inputAnswer: "A",
					report: { inputRequest: { question: "Use A or B?", options: [] } },
				},
			],
		};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (command === "workflow_run_action") {
				if (action.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action.action === "get") return { type: "snapshot", value: run };
				if (action.action === "events") return { type: "events", value: [] };
				if (action.action === "command")
					return { type: "receipt", value: { snapshot: { ...run, status: "running", sequence: 6 } } };
			}
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false },
				};
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Run history" }));
		const resume = await screen.findByRole("button", { name: "Resume run" });
		expect((resume as HTMLButtonElement).disabled).toBe(true);
		fireEvent.change(screen.getByRole("combobox", { name: "Execution" }), { target: { value: "root" } });
		fireEvent.change(screen.getByRole("combobox", { name: "Resume activation" }), { target: { value: "a3" } });
		fireEvent.input(screen.getByRole("textbox", { name: "Resolution" }), { target: { value: "Use A" } });
		fireEvent.click(resume);
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
				project: "/repo",
				action: {
					action: "command",
					run_id: "r1",
					expected_sequence: 5,
					command_id: expect.any(String),
					command: { action: "resume_graph", execution_id: "root", activation_id: "a3", resolution: "Use A" },
				},
			}),
		);
	});

	it("starts only the selected published story revision and offers no plan start button", async () => {
		// Catches: using an editable draft or manual story transition instead of daemon graph start.
		const fallback = vi.mocked(invoke).getMockImplementation();
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			const action = (args as { action?: { action: string } } | undefined)?.action;
			if (action?.action === "transition_history") return { type: "transitions", value: [] };
			if (command === "workflow_definition_action")
				return {
					type: "drafts",
					value: [
						{ id: "flow-1", name: "Published delivery", kind: "story", latestPublishedRevision: 2 },
						{ id: "unpublished", name: "Draft only", kind: "story", latestPublishedRevision: null },
					],
				};
			if (command === "workflow_run_action") {
				if (action?.action === "start_graph" || action?.action === "get")
					return {
						type: "snapshot",
						value: {
							id: "new-run",
							planId: "p1",
							status: "running",
							sequence: 4,
							startedMs: 1000,
							stories: [],
							attempts: [],
							graphExecutions: [],
						},
					};
				if (action?.action === "events") return { type: "events", value: [] };
				if (action?.action === "start_readiness") return { type: "readiness", value: [] };
			}
			return fallback?.(command, args);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		expect(screen.queryByText(/Start plan workflow/)).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Start story workflow" }));
		await screen.findByRole("button", { name: "Start published workflow" });
		expect(screen.queryByRole("option", { name: /Draft only/ })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Start published workflow" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
				project: "/repo",
				action: {
					action: "start_graph",
					target: { type: "story", id: "s1" },
					expected_revision: 1,
					definition_id: "flow-1",
					definition_revision: 2,
					request_id: expect.any(String),
				},
			}),
		);
		await screen.findByRole("region", { name: "Run timeline" });
	});

	describe("story workflow run on a default install (2138)", () => {
		const runBase = {
			id: "r1",
			planId: "p1",
			status: "running",
			sequence: 3,
			startedMs: 1_000,
			stories: [{ storyId: "s1", accepted: false }],
			attempts: [],
		};
		const mockRuns = (getRun: () => Record<string, unknown>) => {
			const fallback = vi.mocked(invoke).getMockImplementation();
			vi.mocked(invoke).mockImplementation(async (command, args) => {
				const action = (args as { action?: { action: string } } | undefined)?.action;
				if (action?.action === "transition_history") return { type: "transitions", value: [] };
				if (command === "workflow_run_action") {
					if (action?.action === "list_plan_runs") return { type: "runs", value: [{ ...runBase }] };
					if (action?.action === "get") return { type: "snapshot", value: getRun() };
					if (action?.action === "events") return { type: "events", value: [] };
				}
				return fallback?.(command, args);
			});
		};

		it("says before start what stops the first agent step and refuses to start", async () => {
			// Catches: a Start button that creates a run which pauses at once on a default install.
			const fallback = vi.mocked(invoke).getMockImplementation();
			vi.mocked(invoke).mockImplementation(async (command, args) => {
				const action = (args as { action?: { action: string } } | undefined)?.action;
				if (action?.action === "transition_history") return { type: "transitions", value: [] };
				if (command === "workflow_definition_action")
					return {
						type: "drafts",
						value: [{ id: "flow-1", name: "Story delivery", kind: "story", latestPublishedRevision: 2 }],
					};
				if (command === "workflow_run_action" && action?.action === "start_readiness")
					return { type: "readiness", value: ["no run configuration is saved; save one in Settings > Agents"] };
				return fallback?.(command, args);
			});
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByRole("heading", { name: "Implement API" });
			fireEvent.click(screen.getByRole("button", { name: "Start story workflow" }));
			expect((await screen.findByRole("alert")).textContent).toContain("Settings > Agents");
			expect((screen.getByRole("button", { name: "Start published workflow" }) as HTMLButtonElement).disabled).toBe(
				true,
			);
		});

		it("keeps Start disabled while readiness is pending, also after switching definition", async () => {
			// Catches: Start enabled during the readiness request, so a click creates the run that pauses at once.
			const fallback = vi.mocked(invoke).getMockImplementation();
			const pending: ((problems: string[]) => void)[] = [];
			vi.mocked(invoke).mockImplementation(async (command, args) => {
				const action = (args as { action?: { action: string } } | undefined)?.action;
				if (action?.action === "transition_history") return { type: "transitions", value: [] };
				if (command === "workflow_definition_action")
					return {
						type: "drafts",
						value: [
							{ id: "flow-1", name: "Story delivery", kind: "story", latestPublishedRevision: 2 },
							{ id: "flow-2", name: "Other delivery", kind: "story", latestPublishedRevision: 1 },
						],
					};
				if (command === "workflow_run_action" && action?.action === "start_readiness")
					return new Promise((resolve) => pending.push((value) => resolve({ type: "readiness", value })));
				return fallback?.(command, args);
			});
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByRole("heading", { name: "Implement API" });
			fireEvent.click(screen.getByRole("button", { name: "Start story workflow" }));
			const start = (await screen.findByRole("button", { name: "Start published workflow" })) as HTMLButtonElement;
			await waitFor(() => expect(pending.length).toBe(1));
			expect(start.disabled).toBe(true);
			pending[0]([]);
			await waitFor(() => expect(start.disabled).toBe(false));
			fireEvent.change(screen.getByRole("combobox", { name: "Published workflow" }), { target: { value: "flow-2" } });
			await waitFor(() => expect(pending.length).toBe(2));
			expect(start.disabled).toBe(true);
			pending[1]([]);
			await waitFor(() => expect(start.disabled).toBe(false));
		});

		it("updates the run list row when the run changes, without Refresh", async () => {
			// Catches: the list keeping the first fetched status while only the detail pane reloads.
			let current: Record<string, unknown> = { ...runBase };
			mockRuns(() => current);
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByRole("heading", { name: "Implement API" });
			fireEvent.click(screen.getByRole("button", { name: "Run history" }));
			await screen.findByText(/running · 1 story · #3/);
			current = { ...runBase, status: "paused", sequence: 5 };
			workflowRunSignals.accept({ repo_path: "/repo", payload: { runId: "r1", sequence: 5 } });
			await screen.findByText(/paused · 1 story · #5/);
		});

		it("shows the pause cause and preselects the only pending activation", async () => {
			// Catches: a paused run with a known cause whose Resume form cannot be used from the UI.
			const paused = {
				...runBase,
				status: "paused",
				pauseCause: "workflow has no run profile for the Implementer role",
				graphExecutions: [
					{
						...graph,
						pauses: [],
						activations: [{ id: "a1", nodeId: "implement", state: "running", edgeIndex: null }],
					},
				],
			};
			mockRuns(() => paused);
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByRole("heading", { name: "Implement API" });
			fireEvent.click(screen.getByRole("button", { name: "Run history" }));
			await screen.findByText(/no run profile for the Implementer role/);
			expect((screen.getByRole("combobox", { name: "Execution" }) as HTMLSelectElement).value).toBe("root");
			expect((screen.getByRole("combobox", { name: "Resume activation" }) as HTMLSelectElement).value).toBe("a1");
		});

		it("does not show a cancelled run's open activation as running", async () => {
			// Catches: the graph listing `running` for work that a cancelled run will never do.
			mockRuns(() => ({
				...runBase,
				status: "cancelled",
				graphExecutions: [
					{ ...graph, pauses: [], activations: [{ id: "a1", nodeId: "implement", state: "running", edgeIndex: null }] },
				],
			}));
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByRole("heading", { name: "Implement API" });
			fireEvent.click(screen.getByRole("button", { name: "Run history" }));
			await screen.findByText(/a1 · implement · cancelled/);
		});
	});

	it("history renders replayed decisions and waits and preserves later-page payloads", async () => {
		// Catches: showing only 'graph' labels while hiding node, decision and pause evidence.
		const run = {
			id: "r1",
			planId: "p1",
			status: "paused",
			sequence: 102,
			startedMs: 1000,
			stories: [],
			attempts: [],
			graphExecutions: [graph],
		};
		const fallback = vi.mocked(invoke).getMockImplementation();
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			const action = (args as { action?: { action: string; after_sequence?: number } } | undefined)?.action;
			if (action?.action === "transition_history") return { type: "transitions", value: [] };
			if (command === "workflow_run_action") {
				if (action?.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action?.action === "get") return { type: "snapshot", value: run };
				if (action?.action === "events")
					return {
						type: "events",
						value:
							action.after_sequence === 100
								? [
										{
											sequence: 102,
											atMs: 1100,
											kind: {
												type: "graph",
												event: {
													type: "transition",
													transition: {
														action: "complete",
														activationId: "a2",
														outcome: "uncertain",
														evidence: { actor: "reviewer", reason: "Later page evidence", references: ["report:r2"] },
													},
												},
											},
										},
									]
								: [{ sequence: 100, atMs: 1000, kind: { type: "paused" } }],
					};
				if (action?.action === "command")
					return { type: "receipt", value: { snapshot: { ...run, status: "cancelled" } } };
			}
			return fallback?.(command, args);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Run history" }));
		await screen.findByText(/Decision · a2 · uncertain · reviewer · Conflicting evidence/);
		expect(screen.getByText(/Operator decision needed · resume to implement/)).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Load more events" }));
		await screen.findByText(/Later page evidence/);
		expect(screen.getAllByText("Event details")).toHaveLength(2);
		fireEvent.click(screen.getByRole("button", { name: "Cancel run" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
				project: "/repo",
				action: {
					action: "command",
					run_id: "r1",
					command_id: expect.any(String),
					expected_sequence: 102,
					command: { action: "cancel" },
				},
			}),
		);
	});

	it("legacy history offers cancellation without graph or status-only resume", async () => {
		// Catches: accidentally reviving pre-graph runs from history controls.
		const run = {
			id: "legacy",
			planId: "p1",
			status: "paused",
			sequence: 2,
			startedMs: 1000,
			stories: [],
			attempts: [],
		};
		const fallback = vi.mocked(invoke).getMockImplementation();
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			const action = (args as { action?: { action: string } } | undefined)?.action;
			if (action?.action === "transition_history") return { type: "transitions", value: [] };
			if (command === "workflow_run_action") {
				if (action?.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action?.action === "get") return { type: "snapshot", value: run };
				if (action?.action === "events") return { type: "events", value: [] };
			}
			return fallback?.(command, args);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Run history" }));
		await screen.findByText("Legacy run · inspect or cancel only.");
		expect(screen.queryByRole("button", { name: "Resume run" })).toBeNull();
		expect(screen.getByRole("button", { name: "Cancel run" })).toBeTruthy();
	});

	it("opens the project workflow designer", async () => {
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
			if (command === "workflow_definition_action" && action.action === "list_drafts")
				return {
					type: "drafts",
					value: [
						{
							id: "f1",
							project: "/repo",
							name: "Story delivery",
							kind: "story",
							graph: {
								nodes: [
									{ id: "start", kind: { type: "start" } },
									{ id: "end", kind: { type: "end" } },
								],
								edges: [],
							},
							draftRevision: 1,
							latestPublishedRevision: 1,
							builtinKey: "story_delivery",
						},
					],
				};
			if (action.action === "list_notes") return { type: "notes", value: [] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false },
				};
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Designer" }));
		await screen.findByRole("heading", { name: "Workflow designer" });
		expect(invoke).toHaveBeenCalledWith("workflow_definition_action", {
			project: "/repo",
			action: { action: "list_drafts" },
		});
	});
});
