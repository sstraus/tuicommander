import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";
import { appLogger } from "../../stores/appLogger";
import { storiesUi } from "../../stores/storiesUi";
import { workflowsUi } from "../../stores/workflowsUi";
import { HttpRpcError } from "../../transport";
import { nativeBackendStatusReply } from "../fixtures/nativeBackendStatus";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

// Settle the app-lifetime hydration promise without mounting the application shell.
beforeAll(async () => {
	vi.mocked(invoke).mockResolvedValueOnce(null);
	await activityStore.hydrate();
});

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

beforeEach(() => {
	storiesUi.setVisibility("all");
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		if (_command === "story_capabilities") return { backend: "native", operations: {} };
		if (_command === "workflow_neutral_action") return { type: "delivery_runs", value: [] };
		if (
			_command === "story_action_command" &&
			(args as { action?: { action?: string } })?.action?.action === "backend_status"
		)
			return nativeBackendStatusReply;
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
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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

	it("catches a failed read being presented as an empty project and supports retry", async () => {
		const fallback = vi.mocked(invoke).getMockImplementation();
		vi.mocked(invoke)
			.mockImplementation(async (command, args) => fallback?.(command, args))
			.mockResolvedValueOnce(nativeBackendStatusReply)
			.mockResolvedValueOnce({ backend: "native", operations: {} })
			.mockRejectedValueOnce(new Error("offline"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/offline/);
		expect(screen.queryByText("Create a plan to begin.")).toBeNull();
		expect(screen.queryByText("Select or create a plan.")).toBeNull();
		expect(screen.queryByText("This plan has no stories.")).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		await screen.findByRole("heading", { name: "Implement API" });
	});

	it("catches action failures hiding previously loaded stories as if their read failed", async () => {
		const original = vi.mocked(invoke).getMockImplementation()!;
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if ((args as { action?: { action: string } })?.action?.action === "transition")
				throw new Error("criteria are incomplete");
			return original(command, args);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Start work" }));
		await screen.findByText("criteria are incomplete");
		expect(screen.getByRole("heading", { name: "Implement API" })).toBeVisible();
		expect(screen.getByText("API works")).toBeVisible();
	});

	it("catches an untyped project error labeling its path as a folder", async () => {
		vi.mocked(invoke)
			.mockResolvedValueOnce(nativeBackendStatusReply)
			.mockResolvedValueOnce({ backend: "native", operations: {} })
			.mockRejectedValueOnce(
				new HttpRpcError(
					"story_action_command",
					422,
					JSON.stringify({ error: "project_unavailable: project root '/repo' is not a directory" }),
				),
			);
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		const alert = await screen.findByRole("alert");
		expect(alert.textContent).toContain("Cannot read /repo");
		expect(alert.textContent).not.toMatch(/folder|plan file/);
		expect(alert.textContent).not.toContain("project_unavailable");
		expect(screen.getByRole("button", { name: "Retry" })).toBeVisible();
	});

	it("explains how to load the missing native stories backend", async () => {
		vi.stubGlobal("__TAURI_INTERNALS__", {});
		try {
			vi.mocked(invoke)
				.mockResolvedValueOnce(nativeBackendStatusReply)
				.mockRejectedValueOnce("Command story_capabilities not found");
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByText(/Restart TUICommander/);
			expect(invoke).toHaveBeenCalledWith("story_capabilities", { project: "/repo" });
		} finally {
			vi.unstubAllGlobals();
		}
	});

	// Catches: a desktop project whose external story adapter is down being told to restart
	// TUICommander (which ends every agent session) instead of seeing that its adapter is unavailable.
	it("shows an unavailable external story adapter instead of asking for a restart", async () => {
		vi.stubGlobal("__TAURI_INTERNALS__", {});
		try {
			vi.mocked(invoke).mockRejectedValueOnce("story backend unavailable: cannot start node: No such file");
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByText(/story backend unavailable/);
			expect(screen.queryByText(/Restart TUICommander/)).toBeNull();
		} finally {
			vi.unstubAllGlobals();
		}
	});

	it("preserves an HTTP domain error even when it says a story was not found", async () => {
		vi.mocked(invoke)
			.mockResolvedValueOnce(nativeBackendStatusReply)
			.mockResolvedValueOnce({ backend: "native", operations: {} })
			.mockRejectedValueOnce(new HttpRpcError("story_action_command", 500, '{"error":"story not found: s1"}'));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/story not found: s1/);
		expect(screen.queryByText(/Restart TUICommander/)).toBeNull();
	});

	// Catches: the dialog showing "RPC story_action_command failed: 422 {...}" instead of the plain message.
	it("shows only the plain message of a rejected user action", async () => {
		vi.mocked(invoke)
			.mockResolvedValueOnce(nativeBackendStatusReply)
			.mockResolvedValueOnce({ backend: "native", operations: {} })
			.mockRejectedValueOnce(
				new HttpRpcError("story_action_command", 422, '{"error":"criteria are incomplete: unchecked indexes 1"}'),
			);
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		const alert = await screen.findByText(/criteria are incomplete/);
		expect(alert.textContent).not.toMatch(/RPC|422|\{/);
	});

	it("classifies a missing HTTP capability route by its status", async () => {
		vi.mocked(invoke)
			.mockResolvedValueOnce(nativeBackendStatusReply)
			.mockRejectedValueOnce(new HttpRpcError("story_capabilities", 404, "Not Found"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/Restart TUICommander/);
	});

	// Catches: a backend that predates the story backend port (it answers `true`) passing the probe.
	it("treats a backend that answers the probe with a plain flag as outdated", async () => {
		vi.mocked(invoke).mockResolvedValueOnce(true);
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/Restart TUICommander/);
	});

	// Catches: the dialog offering actions an external story backend did not declare, or probing
	// without the project so every repository reads the native descriptor.
	it("disables the actions the project's story backend does not support", async () => {
		const probe = vi.mocked(invoke).getMockImplementation();
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "story_capabilities")
				return {
					backend: "external:tuic-reference",
					operations: { create_plan: false, create_story: true, transitions: false, notes: true },
				};
			return probe?.(command, args);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		expect(invoke).toHaveBeenCalledWith("story_capabilities", { project: "/repo" });
		expect(screen.getByRole("button", { name: "New plan" })).toBeDisabled();
		expect(screen.getByRole("button", { name: "New story" })).toBeEnabled();
		expect(screen.queryByRole("button", { name: "Start work" })).toBeNull();
	});

	it("recognizes an older browser backend serving its HTML fallback", async () => {
		vi.mocked(invoke).mockResolvedValueOnce("<!doctype html><html><body>TUICommander</body></html>");
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/Restart TUICommander/);
		expect(screen.queryByText("Invalid plan response")).toBeNull();
	});

	it("catches the permanent manual-prefix hint returning in New story", async () => {
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "New story" }));
		expect(screen.getByLabelText("Acceptance criteria, one per line")).toBeVisible();
		expect(screen.queryByText("Prefix human checks with [MANUAL] to track them in QA.")).toBeNull();
	});

	// Catches: removing the hint also stripping manual markers or changing ordinary criterion parsing.
	it("creates a story with criteria and relative file scope", async () => {
		const created = { ...story, id: "s2", title: "Review API" };
		let rows = [story];
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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
			target: { value: "  Review passes  \r\n\r\n  [MANUAL] Inspect the layout  \r\nNo regressions" },
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
						criteria: ["Review passes", "[MANUAL] Inspect the layout", "No regressions"],
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
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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
		vi.mocked(invoke)
			.mockResolvedValueOnce(nativeBackendStatusReply)
			.mockResolvedValueOnce({ backend: "native", operations: {} })
			.mockRejectedValueOnce(new Error("offline"));
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
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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
				if (_command === "story_capabilities") return { backend: "native", operations: {} };
				if (
					_command === "story_action_command" &&
					(args as { action?: { action?: string } })?.action?.action === "backend_status"
				)
					return nativeBackendStatusReply;
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
			if (_command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				_command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
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

	it("opens the Workflows space from planning without losing the story selection", async () => {
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "story_capabilities") return { backend: "native", operations: {} };
			if (
				command === "story_action_command" &&
				(args as { action?: { action?: string } })?.action?.action === "backend_status"
			)
				return nativeBackendStatusReply;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "transition_history") return { type: "transitions", value: [] };
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
		expect(screen.queryByRole("button", { name: "Designer" })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Workflows" }));
		expect(workflowsUi.visible()).toBe(true);
		expect(workflowsUi.project()).toBe("/repo");
		expect(screen.getByRole("heading", { name: "Implement API" })).toBeTruthy();
		workflowsUi.close();
	});
});
