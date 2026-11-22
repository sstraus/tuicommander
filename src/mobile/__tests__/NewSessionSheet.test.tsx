import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NewSessionSheet } from "../components/NewSessionSheet";

vi.mock("../../transport", () => ({
	rpc: vi.fn().mockResolvedValue({}),
}));
vi.mock("../../stores/toasts", () => ({
	toastsStore: { add: vi.fn() },
}));

import { rpc } from "../../transport";

vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));
const INSTALLED = {
	claude: { path: "/usr/local/bin/claude", version: null },
	codex: { path: "/usr/local/bin/codex", version: null },
};
function mockSpawn(spawn: () => Promise<unknown>) {
	vi.mocked(rpc).mockImplementation(
		(command) =>
			(command === "detect_all_agent_binaries" ? Promise.resolve(INSTALLED) : spawn()) as ReturnType<typeof rpc>,
	);
}
beforeEach(() => {
	vi.mocked(rpc).mockReset();
	mockSpawn(() => Promise.resolve("created-session"));
});
async function ready(view: ReturnType<typeof render>) {
	await waitFor(() => expect(view.getByRole("combobox", { name: "Agent" })).toHaveProperty("disabled", false));
}

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

const REPOS = ["/home/user/project-a", "/home/user/project-b"];

describe("NewSessionSheet", () => {
	it("spawns installed Cursor with its detected CLI path instead of looking up the unrelated cursor binary", async () => {
		// Catches: Cursor is offered from cursor-agent detection but spawn resolves agent_type=cursor as the executable.
		vi.mocked(rpc).mockResolvedValueOnce({ "cursor-agent": { path: "/opt/homebrew/bin/cursor-agent" } });
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={() => {}} />);
		await ready(view);
		expect(view.getByRole("option", { name: "Cursor Agent" })).toBeTruthy();
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(rpc).toHaveBeenCalledWith(
			"spawn_agent",
			expect.objectContaining({
				agent_config: expect.objectContaining({
					agent_type: "cursor",
					binary_path: "/opt/homebrew/bin/cursor-agent",
				}),
			}),
		);
	});
	it("shows detection errors and never spawns from a fallback list", async () => {
		// Catches: a rejected detection silently offers unavailable agents or permits spawning.
		vi.mocked(rpc).mockRejectedValueOnce(new Error("Detection offline"));
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={() => {}} />);
		await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Detection offline"));
		expect(view.queryAllByRole("option")).toHaveLength(0);
		expect(view.getByRole("button", { name: /project-a/ })).toHaveProperty("disabled", true);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(rpc).not.toHaveBeenCalledWith("spawn_agent", expect.anything());
	});

	it("shows an empty installed list without allowing a hardcoded default spawn", async () => {
		// Catches: empty detection leaves Claude selected and spawnable.
		vi.mocked(rpc).mockResolvedValueOnce({});
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={() => {}} />);
		await waitFor(() => expect(view.getByRole("status").textContent).toBe("No installed agents found"));
		expect(view.getByRole("combobox", { name: "Agent" })).toHaveProperty("disabled", true);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(rpc).not.toHaveBeenCalledWith("spawn_agent", expect.anything());
	});

	it("blocks spawning until detection completes then selects the first installed registry agent", async () => {
		// Catches: a slow detection allows a stale Claude spawn, or detection-map order controls the default.
		let finish!: (value: unknown) => void;
		vi.mocked(rpc).mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finish = resolve;
				}),
		);
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={() => {}} />);
		expect(view.getByRole("status").textContent).toContain("Detecting");
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(rpc).not.toHaveBeenCalledWith("spawn_agent", expect.anything());
		finish({ ego: { path: "/bin/ego" }, codex: { path: "/bin/codex" } });
		await ready(view);
		expect(view.getByRole("combobox", { name: "Agent" })).toHaveProperty("value", "codex");
	});

	it("offers installed ego but not absent agy instead of a hardcoded agent list", async () => {
		// Catches: the mobile sheet ignores binary detection and offers only its hardcoded trio.
		vi.mocked(rpc).mockResolvedValueOnce({
			ego: { path: "/usr/local/bin/ego", version: null },
			agy: { path: null, version: null },
			git: { path: "/usr/bin/git", version: null },
			api: { path: "/not-a-cli", version: null },
		});
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={() => {}} />);
		await waitFor(() => expect(view.getByRole("option", { name: "ego" })).toBeTruthy());
		expect(view.queryByRole("option", { name: /agy/i })).toBeNull();
		expect(view.queryByRole("option", { name: "Git" })).toBeNull();
		expect(view.queryByRole("option", { name: "External API" })).toBeNull();
		expect(view.getByRole("combobox", { name: "Agent" })).toHaveProperty("value", "ego");
		await ready(view);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(rpc).toHaveBeenCalledWith(
			"spawn_agent",
			expect.objectContaining({
				agent_config: expect.objectContaining({ agent_type: "ego" }),
			}),
		);
	});
	it("searches repositories, spawns the chosen agent, and opens its session", async () => {
		mockSpawn(() => Promise.resolve("new-session-id"));
		const onCreated = vi.fn();
		const onDismiss = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} onCreated={onCreated} />);
		await ready(view);
		await fireEvent.change(view.getByRole("combobox", { name: "Agent" }), { target: { value: "codex" } });
		await fireEvent.input(view.getByRole("searchbox", { name: "Search repositories" }), {
			target: { value: "project-b" },
		});
		expect(view.queryByRole("button", { name: /project-a/ })).toBeNull();
		await fireEvent.click(view.getByRole("button", { name: /project-b/ }));
		expect(rpc).toHaveBeenCalledWith("spawn_agent", {
			pty_config: { cwd: "/home/user/project-b", rows: 24, cols: 80 },
			agent_config: {
				cwd: "/home/user/project-b",
				agent_type: "codex",
				binary_path: "/usr/local/bin/codex",
				prompt: "",
				print_mode: false,
				args: [],
			},
		});
		expect(onCreated).toHaveBeenCalledWith("new-session-id");
		expect(onDismiss).toHaveBeenCalled();
	});

	it("filters on the directory part of the path, not only the repository name", async () => {
		// Catches: narrowing the filter to the basename, which hides repos the user finds by parent folder.
		const view = render(() => (
			<NewSessionSheet repos={["/work/alpha/api", "/personal/web"]} onDismiss={() => {}} onCreated={() => {}} />
		));
		await fireEvent.input(view.getByRole("searchbox", { name: "Search repositories" }), {
			target: { value: "ALPHA" },
		});
		expect(view.getByRole("button", { name: /api/ })).toBeTruthy();
		expect(view.queryByRole("button", { name: /web/ })).toBeNull();
	});

	it("closes with an explicit X", async () => {
		const onDismiss = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} onCreated={() => {}} />);
		await fireEvent.click(view.getByRole("button", { name: "Close new session" }));
		expect(onDismiss).toHaveBeenCalled();
	});

	it("keeps the sheet open after spawn failure", async () => {
		mockSpawn(() => Promise.reject(new Error("Agent unavailable")));
		const onDismiss = vi.fn();
		const onCreated = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} onCreated={onCreated} />);
		await ready(view);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(view.getByRole("dialog", { name: "New session" })).toBeTruthy();
		expect(onCreated).not.toHaveBeenCalled();
		expect(onDismiss).not.toHaveBeenCalled();
	});

	it("does not start a second agent while a spawn is pending", async () => {
		let finish: ((id: string) => void) | undefined;
		mockSpawn(
			() =>
				new Promise<string>((resolve) => {
					finish = resolve;
				}) as ReturnType<typeof rpc>,
		);
		const onDismiss = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} />);
		await ready(view);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(view.getByRole("button", { name: /project-a/ })).toHaveProperty("disabled", true);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(vi.mocked(rpc).mock.calls.filter(([command]) => command === "spawn_agent")).toHaveLength(1);
		finish?.("created-session");
		await waitFor(() => expect(onDismiss).toHaveBeenCalledOnce());
	});
	it("renders a button for each repo", () => {
		const { container } = render(() => <NewSessionSheet repos={REPOS} onDismiss={() => {}} />);
		const buttons = container.querySelectorAll("button[class*='repoItem']");
		expect(buttons.length).toBe(2);
		expect(buttons[0].textContent).toContain("project-a");
		expect(buttons[1].textContent).toContain("project-b");
	});

	it("starts the selected agent in the selected repository", async () => {
		mockSpawn(() => Promise.resolve("created-session"));
		const onDismiss = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} />);
		await ready(view);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(rpc).toHaveBeenCalledWith("spawn_agent", {
			pty_config: { cwd: "/home/user/project-a", rows: 24, cols: 80 },
			agent_config: {
				cwd: "/home/user/project-a",
				agent_type: "claude",
				binary_path: "/usr/local/bin/claude",
				prompt: "",
				print_mode: false,
				args: [],
			},
		});
		expect(onDismiss).toHaveBeenCalled();
	});

	it("calls onDismiss when backdrop is clicked", async () => {
		const onDismiss = vi.fn();
		const { container } = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} />);
		const backdrop = container.firstElementChild as HTMLElement;
		await fireEvent.click(backdrop);
		expect(onDismiss).toHaveBeenCalled();
	});

	it("shows empty state when no repos", () => {
		const { container } = render(() => <NewSessionSheet repos={[]} onDismiss={() => {}} />);
		expect(container.textContent).toContain("No repositories configured");
	});
});
