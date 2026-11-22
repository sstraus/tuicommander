import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";
import { registerModal } from "../../stores/modalStack";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

// Settle the app-lifetime hydration promise without mounting the application shell.
beforeAll(async () => {
	vi.mocked(invoke).mockResolvedValueOnce(null);
	await activityStore.hydrate();
});

const native = {
	backend: "native",
	label: "TUIC native",
	provenance: "automatic",
	availability: "available",
	reason: "Plans and stories are stored in TUICommander. Create a plan to begin.",
	installation: null,
	version: null,
	error: null,
};
const wiz = {
	backend: "external:wiz-stories",
	label: "Wiz",
	provenance: "automatic",
	availability: "approval_required",
	reason: "Wiz project markers detected",
	installation: "/installed/wiz/6.0.0",
	version: "6.0.0",
	generation: "6.0.0:displayed-entry-sha256",
	error: null,
};
beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(registerModal).mockClear();
});

it("keeps Wiz provenance visible and refuses native reads when the connector is unavailable", async () => {
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		if (!args || _command !== "story_action_command") return null;
		if ((args as { action?: { action: string } }).action?.action === "backend_status")
			return { type: "backend_status", value: { ...wiz, availability: "unavailable", error: "Wiz connector missing" } };
		throw new Error("A failed connector must not reach capabilities or native content");
	});
	render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
	await screen.findByRole("alert");
	expect(screen.getByLabelText("Story backend").textContent).toContain("Wiz");
	expect(screen.getByRole("button", { name: "Story backend details" })).toHaveAttribute(
		"title",
		"Wiz · automatic · unavailable",
	);
	expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "story_action_command")).toHaveLength(1);
});

it("approves the displayed global installation only after an explicit click", async () => {
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		if (!args || _command !== "story_action_command") return null;
		const action = (args as { action: { action: string } }).action;
		if (action.action === "backend_status" || action.action === "approve_connector")
			return { type: "backend_status", value: wiz };
		throw new Error("Unapproved connector cannot be read");
	});
	render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
	fireEvent.click(await screen.findByRole("button", { name: "Story backend details" }));
	const approve = await screen.findByRole("button", { name: "Approve installed Wiz connector" });
	expect(
		vi
			.mocked(invoke)
			.mock.calls.some(([, args]) => (args as { action?: { action: string } })?.action?.action === "approve_connector"),
	).toBe(false);
	fireEvent.click(approve);
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: {
				action: "approve_connector",
				installation: "/installed/wiz/6.0.0",
				expected_identity: "6.0.0:displayed-entry-sha256",
			},
		}),
	);
});

it("refreshes backend identity and capabilities when the project changes", async () => {
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (!args) return null;
		const request = args as { project: string; action?: { action: string } };
		const external = request.project === "/wiz";
		if (command === "story_capabilities")
			return { backend: external ? "external:wiz-stories" : "native", operations: { create_plan: !external } };
		if (request.action?.action === "backend_status")
			return {
				type: "backend_status",
				value: external ? { ...wiz, availability: "available", provenance: "explicit" } : native,
			};
		if (request.action?.action === "list_plans") return { type: "plans", value: [] };
		if (request.action?.action === "list_plan_sources") return { type: "plan_sources", value: [] };
		throw new Error(`Unexpected action ${request.action?.action}`);
	});
	const [project, setProject] = createSignal("/native");
	render(() => <StoriesDialog project={project()} onClose={() => {}} />);
	await waitFor(() => expect(screen.getByLabelText("Story backend").textContent).toContain("TUIC native"));
	await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_capabilities", { project: "/native" }));
	setProject("/wiz");
	await waitFor(() => expect(screen.getByLabelText("Story backend").textContent).toContain("Wiz"));
	await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_capabilities", { project: "/wiz" }));
	expect(screen.getByRole("button", { name: "New plan" })).toBeDisabled();
});

// Catches: a delayed old repository response overwriting a newer header or operation set.
it.each(["backend_status", "story_capabilities"])("discards stale %s after a project switch", async (stage) => {
	let release!: (value: unknown) => void;
	const delayed = new Promise((resolve) => {
		release = resolve;
	});
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		const request = args as { project?: string; action?: { action: string } } | undefined;
		const old = request?.project === "/old";
		const action = request?.action?.action;
		if (old && ((stage === "backend_status" && action === stage) || command === stage)) return delayed;
		if (command === "story_capabilities") return { backend: "native", operations: { create_plan: true } };
		if (action === "backend_status") return { type: "backend_status", value: native };
		if (action === "list_plans") return { type: "plans", value: [] };
		if (action === "list_plan_sources") return { type: "plan_sources", value: [] };
		return null;
	});
	const [project, setProject] = createSignal("/old");
	render(() => <StoriesDialog project={project()} onClose={() => {}} />);
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith(
			stage === "backend_status" ? "story_action_command" : stage,
			stage === "backend_status" ? { project: "/old", action: { action: stage } } : { project: "/old" },
		),
	);
	setProject("/new");
	await waitFor(() => expect(screen.getByRole("button", { name: "New plan" })).toBeEnabled());
	release(
		stage === "backend_status"
			? { type: "backend_status", value: wiz }
			: { backend: "external:wiz-stories", operations: { create_plan: false } },
	);
	await delayed;
	await Promise.resolve();
	expect(screen.getByLabelText("Story backend").textContent).toContain("TUIC native");
	expect(screen.getByRole("button", { name: "New plan" })).toBeEnabled();
});

// Catches: content resolved after a backend switch rendering under an old provenance header.
it("rejects a backend generation change between status and list responses", async () => {
	let statuses = 0;
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		const action = (args as { action?: { action: string } } | undefined)?.action?.action;
		if (command === "story_capabilities") return { backend: "native", operations: { create_plan: true } };
		if (action === "backend_status")
			return { type: "backend_status", value: ++statuses === 1 ? native : { ...wiz, availability: "available" } };
		if (action === "list_plans") return { type: "plans", value: [] };
		if (action === "list_plan_sources")
			return { type: "plan_sources", value: [{ source: "foreign-plan.md", title: "Foreign plan" }] };
		return null;
	});
	render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
	await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Story backend changed during refresh"));
	expect(screen.getByLabelText("Story backend").textContent).toContain("Wiz");
	expect(screen.queryByText("Foreign plan")).not.toBeInTheDocument();
	expect(statuses).toBe(2); // One bounded confirmation, no broad retry loop.
});

// Catches: native override being machine-edit-only rather than a shared human action.
it("offers a native override through the shared story action", async () => {
	let selected = false;
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		const action = (args as { action?: { action: string; backend?: string } } | undefined)?.action;
		if (action?.action === "set_backend") {
			selected = action.backend === "native";
			return { type: "backend_status", value: native };
		}
		if (action?.action === "backend_status") return { type: "backend_status", value: selected ? native : wiz };
		if (command === "story_capabilities") return { backend: "native", operations: { create_plan: true } };
		if (action?.action === "list_plans") return { type: "plans", value: [] };
		if (action?.action === "list_plan_sources") return { type: "plan_sources", value: [] };
		return null;
	});
	render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
	fireEvent.click(await screen.findByRole("button", { name: "Story backend details" }));
	fireEvent.click(screen.getByRole("button", { name: "Use TUIC native" }));
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "set_backend", backend: "native" },
		}),
	);
	await waitFor(() => expect(screen.getByLabelText("Story backend").textContent).toContain("TUIC native"));
});

// Catches: connection details expanding the header or losing keyboard access to backend controls.
it("keeps backend details collapsed beside Workflows and restores focus on Escape", async () => {
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		if ((args as { action?: { action: string } } | undefined)?.action?.action === "backend_status")
			return { type: "backend_status", value: { ...wiz, availability: "unavailable", error: "Connector unavailable" } };
		return null;
	});
	render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
	const toggle = await screen.findByRole("button", { name: "Story backend details" });
	expect(toggle).toHaveAttribute("aria-expanded", "false");
	expect(toggle.parentElement?.parentElement).toContainElement(screen.getByRole("button", { name: "Workflows" }));
	expect(screen.queryByText(wiz.installation)).toBeNull();
	expect(screen.queryByRole("button", { name: "Use automatic selection" })).toBeNull();
	fireEvent.click(toggle);
	const details = screen.getByRole("region", { name: "Story backend details" });
	expect(details.id).toBe(toggle.getAttribute("aria-controls"));
	expect(toggle).toHaveAttribute("aria-expanded", "true");
	expect(details).toHaveTextContent(wiz.installation);
	expect(details).toHaveTextContent(wiz.generation);
	expect(details).toHaveTextContent("Connector unavailable");
	const automatic = screen.getByRole("button", { name: "Use automatic selection" });
	automatic.focus();
	fireEvent.keyDown(automatic, { key: "Escape" });
	expect(screen.queryByRole("region", { name: "Story backend details" })).toBeNull();
	expect(toggle).toHaveAttribute("aria-expanded", "false");
	expect(toggle).toHaveFocus();
});

// Catches: the central modal Escape handler closing the dialog instead of just its disclosure.
it("routes modal Escape through the open disclosure before closing Stories", async () => {
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		if ((args as { action?: { action: string } } | undefined)?.action?.action === "backend_status")
			return { type: "backend_status", value: wiz };
		return null;
	});
	const close = vi.fn();
	render(() => <StoriesDialog project="/repo" onClose={close} />);
	const toggle = await screen.findByRole("button", { name: "Story backend details" });
	fireEvent.click(toggle);
	const modalEscape = vi.mocked(registerModal).mock.calls[0][0];
	modalEscape();
	expect(toggle).toHaveAttribute("aria-expanded", "false");
	expect(toggle).toHaveFocus();
	expect(close).not.toHaveBeenCalled();
	modalEscape();
	expect(close).toHaveBeenCalledOnce();
});
