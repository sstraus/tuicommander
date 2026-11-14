import { readFileSync } from "node:fs";
import { fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../../../../__tests__/mocks/tauri";

vi.mock("../../../../stores/ui", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({ settingsExpertMode: false });
	return {
		uiStore: {
			state,
			setSettingsExpertMode: (enabled: boolean) => setState("settingsExpertMode", enabled),
		},
	};
});

const remoteRpc = vi.hoisted(() => vi.fn());
vi.mock("../../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../../transport")>()),
	rpc: remoteRpc,
}));

const settingsState = vi.hoisted(() => ({ intentTabTitle: true, suggestFollowups: true, progressTracking: true }));
vi.mock("../../../../stores/settings", () => ({
	settingsStore: {
		state: settingsState,
		isAgentEnabled: () => true,
		toggleAgent: vi.fn(),
		setIntentTabTitle: vi.fn(),
		setSuggestFollowups: vi.fn(),
		setProgressTracking: vi.fn(),
	},
}));

vi.mock("../../../../hooks/useAgentDetection", () => ({
	useAgentDetection: () => ({
		detectAll: vi.fn(),
		detectVersion: vi.fn(),
		isAvailable: () => true,
		getDetection: () => ({ available: true }),
	}),
}));

vi.mock("../../../../plugins", () => ({ setClaudeUsageEnabled: vi.fn() }));
vi.mock("../../../../plugins/pluginLoader", () => ({
	isPluginDisabled: () => false,
	setPluginEnabled: vi.fn(),
}));

import { AGENTS, type AgentsConfig, LIMITED_SUPPORT_LABEL } from "../../../../agents";
import { agentConfigsStore } from "../../../../stores/agentConfigs";
import { appLogger } from "../../../../stores/appLogger";
import { settingsExpertStore } from "../../../../stores/settingsExpert";
import { uiStore } from "../../../../stores/ui";
import { AgentsTab } from "../AgentsTab";

// The serialized `AppConfig`/`AgentSettings` defaults these controls compare against.
// The optional overrides are written as explicit nulls: the Rust side omits them
// (`skip_serializing_if`), and a missing key inside a present domain resolves to
// null — the same comparison either way.
const DEFAULTS = {
	app: { progress_tracking: true },
	notifications: {},
	agent_settings: {
		run_configs: [],
		auto_retry_on_error: false,
		headless_template: null,
		native_status_signals: null,
		prevent_alt_screen: null,
		idle_close_minutes: 15,
		skip_trust_dialog: null,
		hook_instrumentation: null,
		intent_tab_title: null,
		progress_tracking: null,
		suggest_followups: null,
		env_flags: null,
	},
	agents: { headless_agent: null },
};

/** Per-agent override rows: label, the agent whose row carries it, a stored override. */
const OVERRIDES: Array<[string, "claude" | "gemini", Record<string, unknown>]> = [
	["Close idle managed child after", "claude", { idle_close_minutes: 0 }],
	["Native status signals", "claude", { native_status_signals: false }],
	["Prevent alternate screen", "gemini", { prevent_alt_screen: false }],
	["Accept workspace trust for managed spawns", "claude", { skip_trust_dialog: false }],
	["Install hooks globally", "gemini", { hook_instrumentation: true }],
	["Track agent intent", "claude", { intent_tab_title: false }],
	["Collect progress", "claude", { progress_tracking: false }],
	["Show suggested follow-ups", "claude", { suggest_followups: false }],
	["Environment Flags", "claude", { env_flags: { CLAUDE_CODE_SIMPLE: "1" } }],
];

let agentsConfig: Record<string, unknown> = {};

async function setup(agents: Record<string, unknown> = {}) {
	agentsConfig = agents;
	await agentConfigsStore.hydrate();
	await settingsExpertStore.open();
}

/** Render the tab and expand one agent's row, where the per-agent controls live. */
function renderExpanded(agent: "claude" | "gemini" | "codex" | "ego" = "claude") {
	const result = render(() => <AgentsTab />);
	const header = [...result.container.querySelectorAll("[role='button']")].find((el) =>
		el.textContent?.includes(AGENTS[agent].name),
	) as HTMLElement;
	fireEvent.click(header);
	return result;
}

const has = (container: HTMLElement, text: string) => container.textContent?.includes(text) ?? false;

// Hook guidance mentions the setting even while its expert control is hidden.
const hasOverrideControl = (container: HTMLElement, label: string) =>
	label === "Install hooks globally"
		? within(container).queryByRole("checkbox", { name: /^Install hooks globally/ }) !== null
		: has(container, label);

describe("AgentsTab expert controls", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		remoteRpc.mockImplementation((cmd: string) =>
			Promise.resolve(cmd === "get_agent_hook_state" ? "notInstalled" : undefined),
		);
		settingsState.progressTracking = true;
		uiStore.setSettingsExpertMode(false);
		mockInvoke.mockImplementation((cmd: string, payload?: { config: AgentsConfig }) => {
			if (cmd === "list_installed_mcp_integrations") return Promise.resolve([]);
			if (cmd === "get_config_defaults") return Promise.resolve(DEFAULTS);
			if (cmd === "save_agents_config" && payload) {
				agentsConfig = JSON.parse(JSON.stringify(payload.config.agents));
				return Promise.resolve(undefined);
			}
			if (cmd === "load_agents_config") return Promise.resolve({ agents: agentsConfig });
			// The intent/progress/follow-up overrides render only with the bridge installed.
			if (cmd === "get_agent_mcp_status")
				return Promise.resolve({ supported: true, installed: true, config_path: null, shared_settings_file: false });
			return Promise.resolve(undefined);
		});
	});

	afterEach(() => {
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
	});

	// Catches: duplicate-name migration losing the error cue/description or enabling Save.
	it("links duplicate run config validation through native invalid state", async () => {
		await setup({
			claude: { run_configs: [{ name: "Existing", command: "claude", args: [], env: {}, is_default: true }] },
		});
		const view = renderExpanded("claude");
		fireEvent.click(view.getByRole("button", { name: "Add Configuration..." }));
		const name = view.getByRole("textbox", { name: "Configuration name" });
		fireEvent.input(name, { target: { value: "Existing" } });
		expect(name).toHaveAttribute("aria-invalid", "true");
		expect(name).toHaveAccessibleDescription('Name "Existing" already exists');
		expect(view.getByRole("button", { name: "Save" })).toBeDisabled();
		fireEvent.input(name, { target: { value: "Unique" } });
		expect(name).toHaveAttribute("aria-invalid", "false");
		expect(view.getByRole("button", { name: "Save" })).not.toBeDisabled();
	});

	// Catches: shared controls dropping Enter-save/Escape-cancel or persisting a canceled draft.
	it("preserves keyboard save and cancel for run configuration drafts", async () => {
		await setup();
		const view = renderExpanded("claude");
		fireEvent.click(view.getByRole("button", { name: "Add Configuration..." }));
		const name = view.getByRole("textbox", { name: "Configuration name" });
		fireEvent.input(name, { target: { value: "Keyboard saved" } });
		fireEvent.keyDown(name, { key: "Enter" });
		await waitFor(() => expect(view.queryByRole("group", { name: "New run configuration" })).toBeNull());
		expect(agentConfigsStore.getRunConfigs("claude").some((config) => config.name === "Keyboard saved")).toBe(true);
		const saves = mockInvoke.mock.calls.filter(([command]) => command === "save_agents_config").length;
		fireEvent.click(view.getByRole("button", { name: "Add Configuration..." }));
		const canceled = view.getByRole("textbox", { name: "Configuration name" });
		fireEvent.input(canceled, { target: { value: "Canceled draft" } });
		fireEvent.keyDown(canceled, { key: "Escape" });
		expect(view.queryByRole("group", { name: "New run configuration" })).toBeNull();
		expect(mockInvoke.mock.calls.filter(([command]) => command === "save_agents_config")).toHaveLength(saves);
		expect(agentConfigsStore.getRunConfigs("claude").some((config) => config.name === "Canceled draft")).toBe(false);
	});

	// Catches: installed-action migration losing owner state or calling install when pressed.
	it("keeps installed MCP action pressed and routes its click to removal", async () => {
		await setup();
		const view = renderExpanded("claude");
		const remove = await view.findByRole("button", { name: "Remove TUIC MCP" });
		expect(remove).toHaveAttribute("aria-pressed", "true");
		fireEvent.click(remove);
		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("remove_agent_mcp", { agentType: "claude" }));
	});

	// Catches: saved files or a pending native review are labelled operational.
	it("shows Codex native hook review instead of an installed badge", async () => {
		remoteRpc.mockImplementation((cmd: string) =>
			Promise.resolve(cmd === "get_agent_hook_state" ? "reviewRequired" : undefined),
		);
		await setup({ codex: { run_configs: [], hook_instrumentation: true } });
		const view = renderExpanded("codex");
		await waitFor(() => expect(view.getByText("Hooks: review in /hooks")).toBeInTheDocument());
		expect(view.queryByText("Hooks installed")).toBeNull();
		expect(view.getByRole("checkbox", { name: /^Install hooks globally/ })).toBeChecked();
		expect(remoteRpc).not.toHaveBeenCalledWith("set_agent_hook_instrumentation", expect.anything(), expect.anything());
		remoteRpc.mockReset();
	});

	// Catches: a client shows hook actions using its own OS instead of the owning host response.
	it.each([undefined, "windows-owner"])(
		"hides unsupported owning-machine hooks for %s without changing saved preferences",
		async (connectionId) => {
			remoteRpc.mockImplementation((cmd: string) => {
				if (cmd === "get_agent_hook_state") return Promise.resolve("unsupported");
				if (cmd === "load_agents_config")
					return Promise.resolve({ agents: { codex: { run_configs: [], hook_instrumentation: true } } });
				if (cmd === "get_config_defaults") return Promise.resolve(DEFAULTS);
				return Promise.resolve(undefined);
			});
			await setup({ codex: { run_configs: [], hook_instrumentation: true } });
			uiStore.setSettingsExpertMode(true);
			const view = render(() => <AgentsTab connectionId={connectionId} />);
			const header = [...view.container.querySelectorAll<HTMLElement>("[role='button']")].find((el) =>
				el.textContent?.includes(AGENTS.codex.name),
			)!;
			fireEvent.click(header);
			await waitFor(() =>
				expect(remoteRpc).toHaveBeenCalledWith("get_agent_hook_state", { agentType: "codex" }, connectionId),
			);
			expect(view.queryByText("Native status signals")).toBeNull();
			expect(view.queryByRole("checkbox", { name: /^Install hooks globally/ })).toBeNull();
			expect(view.queryByText("Hooks installed")).toBeNull();
			expect(agentConfigsStore.getHookInstrumentation("codex")).toBe(true);
			expect(
				remoteRpc.mock.calls.some(
					([cmd]) => cmd === "set_agent_hook_instrumentation" || cmd === "set_agent_native_status_signals",
				),
			).toBe(false);
		},
	);

	it("labels only amp, cursor and droid as limited support", async () => {
		await setup();
		const { container } = render(() => <AgentsTab />);
		const labelled = [...container.querySelectorAll("[role='button']")]
			.filter((row) => row.textContent?.includes(LIMITED_SUPPORT_LABEL))
			.map((row) => row.textContent ?? "");
		expect(LIMITED_SUPPORT_LABEL).toBe("limited support, need more tests");
		expect(labelled).toHaveLength(3);
		for (const agent of ["amp", "cursor", "droid"] as const) {
			expect(labelled.filter((text) => text.includes(AGENTS[agent].name))).toHaveLength(1);
		}
		for (const agent of ["claude", "codex", "grok", "ego"] as const) {
			expect(labelled.some((text) => text.includes(AGENTS[agent].name))).toBe(false);
		}
	});

	it("quotes the same label in the user guide for the same three agents", () => {
		const rows = readFileSync("docs/user-guide/ai-agents.md", "utf8")
			.split("\n")
			.filter((line) => line.includes(LIMITED_SUPPORT_LABEL));
		expect(rows.map((row) => row.split("|")[1].trim().split(" ")[0])).toEqual(["Amp", "Cursor", "Droid"]);
	});

	it("persists ego permission choices through config transport instead of losing them on reload", async () => {
		await setup();
		const { container } = renderExpanded("ego");
		fireEvent.change(container.querySelector("#ego-permission-mode")!, { target: { value: "edits" } });
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith(
				"save_agents_config",
				expect.objectContaining({
					config: expect.objectContaining({
						agents: expect.objectContaining({ ego: expect.objectContaining({ ego_mode: "edits" }) }),
					}),
				}),
			),
		);
		fireEvent.change(container.querySelector("#ego-permission-sandbox")!, { target: { value: "ro" } });
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith(
				"save_agents_config",
				expect.objectContaining({
					config: expect.objectContaining({
						agents: expect.objectContaining({ ego: expect.objectContaining({ ego_mode: "edits", ego_sandbox: "ro" }) }),
					}),
				}),
			),
		);
		// Hydration must recover the actual IO save, not a hand-set oracle.
		await agentConfigsStore.hydrate();
		expect((container.querySelector("#ego-permission-mode") as HTMLSelectElement).value).toBe("edits");
		expect((container.querySelector("#ego-permission-sandbox") as HTMLSelectElement).value).toBe("ro");
		fireEvent.change(container.querySelector("#ego-permission-mode")!, { target: { value: "" } });
		await waitFor(() => expect(agentConfigsStore.state.agents.ego.ego_mode).toBeUndefined());
		expect(agentConfigsStore.state.agents.ego.ego_sandbox).toBe("ro");
	});

	it.each([
		[["--dangerously-bypass-approvals-and-sandbox", "--search"], true],
		[["--search"], false],
		[["--", "--dangerously-bypass-approvals-and-sandbox"], false],
	] as const)("does not hide active bypass or mistake positional text for bypass: %s", async (args, visible) => {
		await setup({
			codex: { run_configs: [{ name: "Default", command: "codex", args: [...args], env: {}, is_default: true }] },
		});
		const { container } = renderExpanded("codex");
		expect(container.querySelector('[aria-label="Approvals and sandbox bypassed"]') !== null).toBe(visible);
	});

	it("hides the expert controls at their defaults in basic mode and keeps the basic ones", async () => {
		await setup();
		const { container } = renderExpanded();
		expect(has(container, "Collect project progress")).toBe(false);
		expect(has(container, "Auto-retry on server errors")).toBe(false);
		expect(has(container, "Headless Command Template")).toBe(false);

		expect(has(container, "Show agent intent as tab title")).toBe(true);
		expect(has(container, "Show suggested follow-up actions")).toBe(true);
		expect(has(container, "Run Configurations")).toBe(true);
	});

	it("shows Collect project progress in basic mode once it is turned off", async () => {
		settingsState.progressTracking = false;
		await setup();
		const { container } = render(() => <AgentsTab />);
		expect(has(container, "Collect project progress")).toBe(true);
	});

	it("shows Auto-retry in basic mode once it is turned on for the agent", async () => {
		await setup({ claude: { run_configs: [], auto_retry_on_error: true } });
		const { container } = renderExpanded();
		expect(has(container, "Auto-retry on server errors")).toBe(true);
	});

	it("shows the Headless Command Template in basic mode once the agent overrides it", async () => {
		await setup({ claude: { run_configs: [], headless_template: 'claude -p "{prompt}"' } });
		const { container } = renderExpanded();
		expect(has(container, "Headless Command Template")).toBe(true);
	});

	it("keeps the Headless Command Template hidden while only the built-in template applies", async () => {
		// The input shows AGENTS.claude.defaultHeadlessTemplate as its value; that
		// fallback is not an override and must not count as a modification.
		await setup({ claude: { run_configs: [] } });
		const { container } = renderExpanded();
		expect(has(container, "Headless Command Template")).toBe(false);
	});

	it.each(OVERRIDES)("hides %s in basic mode while the agent has no override", async (label, agent) => {
		await setup();
		const { container } = renderExpanded(agent);
		// Wait for the bridge status, so the gated rows would be rendered if visible.
		await waitFor(() => expect(has(container, "MCP bridge installed")).toBe(true));
		expect(hasOverrideControl(container, label)).toBe(false);
		if (label === "Install hooks globally") {
			expect(within(container).getByRole("complementary", { name: "More precise agent status" })).toBeInTheDocument();
		}
	});

	it.each(OVERRIDES)("shows %s in basic mode once the agent stores an override", async (label, agent, override) => {
		await setup({ [agent]: { run_configs: [], ...override } });
		const { container } = renderExpanded(agent);
		await waitFor(() => expect(hasOverrideControl(container, label)).toBe(true));
	});

	it("shows every per-agent override at its default in expert mode", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		for (const [label, agent] of OVERRIDES) {
			const { container, unmount } = renderExpanded(agent);
			await waitFor(() => expect(hasOverrideControl(container, label), label).toBe(true));
			unmount();
		}
	});

	it("shows every expert control at its default in expert mode", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const { container } = renderExpanded();
		await waitFor(() => {
			expect(has(container, "Collect project progress")).toBe(true);
			expect(has(container, "Auto-retry on server errors")).toBe(true);
			expect(has(container, "Headless Command Template")).toBe(true);
		});
	});

	it("shows native scrollback prevention for an agent without a CLI flag", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const { container } = renderExpanded("gemini");
		await waitFor(() => expect(has(container, "Prevent alternate screen")).toBe(true));
		const label = [...container.querySelectorAll("label")].find((el) =>
			el.textContent?.includes("Prevent alternate screen"),
		);
		const checkbox = container.querySelector(`#${label?.htmlFor}`) as HTMLInputElement;
		expect(checkbox.checked).toBe(true);
		fireEvent.click(checkbox);
		expect(agentConfigsStore.getPreventAltScreen("gemini")).toBe(false);
	});

	it("keeps managed trust acceptance on by default and saves an explicit opt-out", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const { container } = renderExpanded("claude");
		const label = [...container.querySelectorAll("label")].find((el) =>
			el.textContent?.includes("Accept workspace trust for managed spawns"),
		);
		const checkbox = container.querySelector(`#${label?.htmlFor}`) as HTMLInputElement;
		expect(checkbox.checked).toBe(true);
		fireEvent.click(checkbox);
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith(
				"save_agents_config",
				expect.objectContaining({
					config: expect.objectContaining({
						agents: expect.objectContaining({ claude: expect.objectContaining({ skip_trust_dialog: false }) }),
					}),
				}),
			),
		);
	});

	// Catches: the trust row gated to claude/codex, so ego users cannot see the
	// setting that the backend honors with --trust-workspace (#2983-c437).
	it("shows managed trust acceptance for ego", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const { container } = renderExpanded("ego");
		await waitFor(() => expect(has(container, "Accept workspace trust for managed spawns")).toBe(true));
	});

	// Catches: a Claude Code 2.1.319/2.1.321 flag missing from the editor (category outside
	// CATEGORY_ORDER, or not in the catalogue), or PLUGIN_DIR_WATCH written as a toggle
	// value other than the documented "0".
	it("lists the Claude Code 2.1.319 and 2.1.321 env flags and stores PLUGIN_DIR_WATCH=0", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const view = renderExpanded("claude");
		fireEvent.click(await view.findByText("Environment Flags"));
		for (const key of [
			"CLAUDE_CODE_DISABLE_MCP_TASKS",
			"CLAUDE_CODE_DISABLE_MCP_TASK_BACKGROUND",
			"CLAUDE_CODE_DISABLE_FEATURE_FLAGS",
			"CLAUDE_CODE_LOOP_KEEPALIVE",
			"CLAUDE_CODE_AUTO_MODE_SERVER",
			"CLAUDE_CODE_PLUGIN_DIR_WATCH",
		]) {
			expect(view.getAllByText(key), key).toHaveLength(1);
		}
		const row = view.getByText("CLAUDE_CODE_PLUGIN_DIR_WATCH").parentElement!;
		fireEvent.change(row.querySelector("select")!, { target: { value: "0" } });
		await waitFor(() =>
			expect(agentConfigsStore.getEnvFlags("claude")).toMatchObject({ CLAUDE_CODE_PLUGIN_DIR_WATCH: "0" }),
		);
	});

	it("stores zero minutes to keep managed children open", async () => {
		await setup();
		uiStore.setSettingsExpertMode(true);
		const { container } = renderExpanded("claude");
		const label = [...container.querySelectorAll("label")].find(
			(e) => e.textContent === "Close idle managed child after",
		);
		const input = container.querySelector<HTMLInputElement>(`#${label?.htmlFor}`);
		expect(input?.value).toBe("15");
		fireEvent.change(input!, { target: { value: "0" } });
		await waitFor(() => expect(agentConfigsStore.getIdleCloseMinutes("claude")).toBe(0));
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith(
				"save_agents_config",
				expect.objectContaining({
					config: expect.objectContaining({
						agents: expect.objectContaining({ claude: expect.objectContaining({ idle_close_minutes: 0 }) }),
					}),
				}),
			),
		);
	});
});

// The real machine registry hydrates from transport; only the external IO is held.
describe("launch preview machine ownership", () => {
	beforeEach(() => vi.spyOn(appLogger, "error").mockImplementation(() => undefined));
	let settleLoad: (() => void) | undefined;
	afterEach(() => {
		settleLoad?.();
		vi.restoreAllMocks();
		settingsExpertStore._resetForTests();
	});
	it.each(["delayed", "failed"])("catches stale local preview during %s remote hydration", async (outcome) => {
		let resolveLoad!: (value: { agents: Record<string, unknown> }) => void;
		let rejectLoad!: (error: Error) => void;
		const load = new Promise<{ agents: Record<string, unknown> }>((resolve, reject) => {
			resolveLoad = resolve;
			rejectLoad = reject;
		});
		settleLoad = () => resolveLoad({ agents: {} });
		const preview = (text: string) => ({
			...DEFAULTS,
			launch_instructions: { default_text: text, required_markers: [], effective_text: {} },
		});
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "load_agents_config") return Promise.resolve({ agents: {} });
			if (cmd === "get_config_defaults") return Promise.resolve(preview("LOCAL PREVIEW"));
			if (cmd === "list_installed_mcp_integrations") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});
		remoteRpc.mockImplementation((cmd: string) => {
			if (cmd === "load_agents_config") return load;
			if (cmd === "get_config_defaults") return Promise.resolve(preview("REMOTE PREVIEW"));
			return Promise.resolve(undefined);
		});
		await agentConfigsStore.hydrate();
		await settingsExpertStore.open();
		const [machine, setMachine] = createSignal<string>();
		const view = render(() => <AgentsTab connectionId={machine()} />);
		await waitFor(() => expect(view.getByText("LOCAL PREVIEW")).toBeTruthy());
		expect((view.getByRole("button", { name: "Add instructions" }) as HTMLButtonElement).disabled).toBe(false);
		setMachine(`preview-${outcome}`);
		await waitFor(() => expect(remoteRpc).toHaveBeenCalledWith("load_agents_config", {}, `preview-${outcome}`));
		expect(view.queryByText("LOCAL PREVIEW")).toBeNull();
		expect((view.getByRole("button", { name: "Add instructions" }) as HTMLButtonElement).disabled).toBe(true);
		if (outcome === "failed") {
			rejectLoad(new Error("remote offline"));
			await waitFor(() => expect(view.container.textContent).toContain("remote offline"));
			expect(appLogger.error).toHaveBeenCalledWith("config", "Failed to hydrate agent configs", expect.any(Error));
			expect(view.queryByText("LOCAL PREVIEW")).toBeNull();
			expect((view.getByRole("button", { name: "Add instructions" }) as HTMLButtonElement).disabled).toBe(true);
		} else {
			resolveLoad({ agents: {} });
			await waitFor(() => expect(view.getByText("REMOTE PREVIEW")).toBeTruthy());
			expect((view.getByRole("button", { name: "Add instructions" }) as HTMLButtonElement).disabled).toBe(false);
		}
	});
});
