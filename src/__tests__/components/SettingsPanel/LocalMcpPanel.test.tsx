import { fireEvent, render, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
// Only `rpc` is replaced: `invoke` (the config defaults) needs the real
// `isTauri` and connection resolver to reach the Tauri mock.
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn(),
}));
vi.mock("../../../stores/ui", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({ settingsExpertMode: false });
	return {
		uiStore: {
			state,
			setSettingsExpertMode: vi.fn((enabled: boolean) => setState("settingsExpertMode", enabled)),
		},
	};
});

import { scrollToSetting } from "../../../components/SettingsPanel/SettingsSearch";
import { SETTINGS_SEARCH_INDEX } from "../../../components/SettingsPanel/settingsSearchIndex";
import { LocalMcpPanel } from "../../../components/SettingsPanel/tabs/services/LocalMcpPanel";
import { settingsExpertStore } from "../../../stores/settingsExpert";
import { uiStore } from "../../../stores/ui";
import { rpc } from "../../../transport";
import { mockInvoke } from "../../mocks/tauri";

const nativeTools = [
	"session",
	"plugin_dev_guide",
	"settings",
	"debug",
	"remote",
	"secret",
	"progress",
	"telegram",
].map((name) => ({
	name,
	title: name,
	description: `${name} full description`,
	domain: name,
	parent: null,
	section: "always",
}));

describe("LocalMcpPanel", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(rpc).mockImplementation((command: string) => {
			if (command === "get_mcp_status") {
				return Promise.resolve({
					enabled: true,
					running: true,
					remote_port: null,
					active_sessions: 0,
					mcp_clients: 0,
					max_sessions: 10,
					reachable: null,
					native_capabilities: nativeTools,
				});
			}
			if (command === "get_relay_status") {
				return Promise.resolve({ enabled: false, connected: false, url: "", session_id: "" });
			}
			if (command === "load_config") {
				return Promise.resolve({
					mcp_capabilities: { denied: [], migration_unknown_tools: [] },
					collapse_tools: false,
				});
			}
			return Promise.resolve(undefined);
		});
	});

	// Catches: redundant MCP narration returning above the real visibility controls.
	it("does not narrate native tools or on-demand discovery", async () => {
		uiStore.setSettingsExpertMode(true);
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		expect(view.getByRole("checkbox", { name: /Collapse tools/ })).toBeDefined();
		for (const copy of [
			"Capabilities exposed via MCP. Disable tools to restrict what AI agents can access.",
			"Discover tools on demand through search_tools, get_tool_schema and call_tool.",
		])
			expect(view.queryByText(copy)).toBeNull();
		view.unmount();
		uiStore.setSettingsExpertMode(false);
	});

	// Catches: page/card inventories returning as permanent MCP narration.
	it.each([
		"Local MCP connection, tool visibility and upstream servers.",
		"Choose which built-in tools MCP clients can call. Disabled tools are hidden from clients.",
	])("does not repeat MCP card contents: %s", async (copy) => {
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		expect(view.getByRole("heading", { name: "TUIC Tools" })).toBeDefined();
		expect(view.getByRole("checkbox", { name: "session" })).toBeDefined();
		expect(view.queryByText(copy)).toBeNull();
		view.unmount();
	});

	// Catches: a static UI inventory silently drops a newly registered backend tool.
	it("renders every backend native tool, including new tools and full descriptions", async () => {
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		for (const tool of nativeTools) {
			expect(view.getByRole("checkbox", { name: tool.name })).toBeDefined();
			expect(view.getByText(tool.description)).toBeDefined();
		}
		view.unmount();
	});

	// Catches: core permissions hidden in a disclosure, children promoted to domain rows,
	// or unavailable domains omitted so the user cannot configure them.
	it("shows domain sections with visible nested switches and backend availability", async () => {
		const base = vi.mocked(rpc).getMockImplementation();
		const capabilities = [
			{ name: "agent", domain: "agent", parent: null, section: "always" },
			{ name: "agent_tasks", domain: "agent", parent: "agent", section: "always" },
			{ name: "diagnostics", domain: "debug", parent: null, section: "rare" },
			{
				name: "voice",
				domain: "voice",
				parent: null,
				section: "conditional",
				available: false,
				availability: "Voice is not configured",
			},
		].map((capability) => ({ ...capability, title: capability.name, description: capability.name + " help" }));
		vi.mocked(rpc).mockImplementation((command, args) =>
			command === "get_mcp_status" ? Promise.resolve({ native_capabilities: capabilities }) : base!(command, args),
		);
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		expect(view.container.querySelector("details")).toBeNull();
		expect(
			within(view.getByRole("region", { name: "Always listed" })).getByRole("group", { name: "agent" }),
		).toBeDefined();
		expect(
			within(view.getByRole("group", { name: "agent" })).getByRole("checkbox", { name: "agent_tasks" }),
		).toBeDefined();
		expect(
			within(view.getByRole("region", { name: "Rarely needed" })).getByRole("checkbox", { name: "diagnostics" }),
		).toBeDefined();
		expect(
			within(view.getByRole("region", { name: "Conditional" })).getByText("Voice is not configured"),
		).toBeDefined();
		view.unmount();
	});

	// Catches: sibling edits drop migrated action denials or unresolved migration diagnostics.
	it("preserves migrated restrictions and unknown names across edits and reload", async () => {
		let saved = {
			mcp_capabilities: {
				denied: ["agent_tasks", "workflow.reports", "diagnostics"],
				migration_unknown_tools: ["unknown_tool"],
			},
			settings_mcp_enabled: false,
		};
		const capabilities = ["agent", "agent_tasks", "workflow", "workflow.reports", "diagnostics", "settings"].map(
			(name) => ({
				name,
				title: name,
				description: name + " help",
				domain: name === "agent_tasks" ? "agent" : name.startsWith("workflow.") ? "workflow" : name,
				parent: name === "agent_tasks" ? "agent" : name.startsWith("workflow.") ? "workflow" : null,
				section: "always",
			}),
		);
		vi.mocked(rpc).mockImplementation(async (command) =>
			command === "load_config"
				? structuredClone(saved)
				: command === "get_mcp_status"
					? { native_capabilities: capabilities }
					: undefined,
		);
		mockInvoke.mockImplementation(async (command, args) => {
			if (command === "load_config") return structuredClone(saved);
			if (command === "save_config") saved = structuredClone(args?.config as typeof saved);
		});
		let view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		expect(view.getByRole("status").textContent).toContain("unknown_tool");
		expect((view.getByRole("checkbox", { name: "agent_tasks" }) as HTMLInputElement).checked).toBe(false);
		fireEvent.click(view.getByRole("checkbox", { name: "agent" }));
		await vi.advanceTimersByTimeAsync(0);
		expect(saved.mcp_capabilities).toEqual({
			denied: ["agent_tasks", "workflow.reports", "diagnostics", "agent"],
			migration_unknown_tools: ["unknown_tool"],
		});
		view.unmount();
		view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		for (const name of ["agent", "agent_tasks", "workflow.reports", "diagnostics", "settings"])
			expect((view.getByRole("checkbox", { name }) as HTMLInputElement).checked).toBe(false);
		view.unmount();
	});

	// Catches: disabled tools disappear from Settings and can never be re-enabled.
	it("re-enables a disabled backend tool without dropping other disabled names", async () => {
		const base = vi.mocked(rpc).getMockImplementation();
		vi.mocked(rpc).mockImplementation((command: string, args?: Record<string, unknown>) =>
			command === "load_config"
				? Promise.resolve({
						mcp_capabilities: { denied: ["telegram", "diagnostics", "unknown_tool"], migration_unknown_tools: [] },
						collapse_tools: false,
					})
				: (base?.(command, args) as Promise<never>),
		);
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "load_config"
					? { mcp_capabilities: { denied: ["telegram", "diagnostics", "unknown_tool"], migration_unknown_tools: [] } }
					: undefined,
			),
		);
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);
		const toggle = view.getByRole("checkbox", { name: "telegram" }) as HTMLInputElement;
		expect(toggle.checked).toBe(false);
		fireEvent.click(toggle);
		await vi.advanceTimersByTimeAsync(0);
		expect(mockInvoke).toHaveBeenCalledWith(
			"save_config",
			expect.objectContaining({
				config: { mcp_capabilities: { denied: ["diagnostics", "unknown_tool"], migration_unknown_tools: [] } },
			}),
		);
		fireEvent.click(toggle);
		await vi.advanceTimersByTimeAsync(0);
		expect(mockInvoke).toHaveBeenLastCalledWith(
			"save_config",
			expect.objectContaining({
				config: {
					mcp_capabilities: { denied: ["diagnostics", "unknown_tool", "telegram"], migration_unknown_tools: [] },
				},
			}),
		);
		view.unmount();
	});

	afterEach(() => {
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.clearAllMocks();
	});

	it("shows the local MCP content it owns", async () => {
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.getByText("Server connections")).toBeDefined();
		expect(view.getByText("TUIC Tools")).toBeDefined();
		expect(view.getByText("Server Status")).toBeDefined();

		view.unmount();
	});

	it("never renders Remote Access or Cloud Relay content — that belongs to RemoteAccessPanel", async () => {
		const view = render(() => <LocalMcpPanel />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.queryByText("Remote Access")).toBeNull();
		expect(view.queryByText("Cloud Relay")).toBeNull();
		expect(view.queryByText("Enable remote access")).toBeNull();

		view.unmount();
	});

	it("owns a single get_mcp_status poll and clears it on unmount", async () => {
		const view = render(() => <LocalMcpPanel />);
		const statusCalls = () => vi.mocked(rpc).mock.calls.filter(([command]) => command === "get_mcp_status").length;

		await vi.advanceTimersByTimeAsync(0);
		expect(statusCalls()).toBe(1);

		await vi.advanceTimersByTimeAsync(3000);
		expect(statusCalls()).toBe(2);

		view.unmount();
		await vi.advanceTimersByTimeAsync(6000);
		expect(statusCalls()).toBe(2);
	});

	describe("expert controls", () => {
		const DEFAULTS = {
			app: { collapse_tools: false, mcp_capabilities: { denied: ["settings", "debug"], migration_unknown_tools: [] } },
			notifications: {},
			agent_settings: {},
		};

		/** Render the page over a saved config, with the Rust defaults loaded. */
		async function renderWith(saved: {
			collapse_tools: boolean;
			mcp_capabilities: { denied: string[]; migration_unknown_tools: string[] };
		}) {
			mockInvoke.mockImplementation((cmd: string) =>
				cmd === "get_config_defaults" ? Promise.resolve(DEFAULTS) : Promise.resolve(undefined),
			);
			const base = vi.mocked(rpc).getMockImplementation();
			vi.mocked(rpc).mockImplementation((command: string, args?: Record<string, unknown>) => {
				if (command === "load_config") return Promise.resolve(saved);
				return base?.(command, args) as Promise<never>;
			});
			await settingsExpertStore.open();
			const view = render(() => <LocalMcpPanel />);
			await vi.advanceTimersByTimeAsync(0);
			return view;
		}

		const collapseTools = (view: ReturnType<typeof render>) => view.queryByText(/Collapse tools/);
		const nativeToolToggles = (view: ReturnType<typeof render>) => view.queryByText("plugin_dev_guide");

		it("hides Collapse tools at its default, but keeps the basic native tool toggles", async () => {
			const view = await renderWith({
				collapse_tools: false,
				mcp_capabilities: { denied: ["settings", "debug"], migration_unknown_tools: [] },
			});
			expect(collapseTools(view)).toBeNull();
			expect(nativeToolToggles(view)).not.toBeNull();
			expect(view.getByText("Capabilities")).toBeDefined();
			view.unmount();
		});

		it("shows Collapse tools once its saved value differs from the default", async () => {
			const view = await renderWith({
				collapse_tools: true,
				mcp_capabilities: { denied: ["settings", "debug"], migration_unknown_tools: [] },
			});
			expect(collapseTools(view)).not.toBeNull();
			view.unmount();
		});

		it("shows Collapse tools at its default in expert mode", async () => {
			uiStore.setSettingsExpertMode(true);
			const view = await renderWith({
				collapse_tools: false,
				mcp_capabilities: { denied: ["settings", "debug"], migration_unknown_tools: [] },
			});
			expect(collapseTools(view)).not.toBeNull();
			view.unmount();
		});

		// The toggles render the saved list, so they must not show the `[]`
		// placeholder (every tool on) before `load_config` answers.
		it("renders the native tool toggles only once the saved list has loaded", async () => {
			let answer: (config: unknown) => void = () => {};
			const base = vi.mocked(rpc).getMockImplementation();
			vi.mocked(rpc).mockImplementation((command: string, args?: Record<string, unknown>) =>
				command === "load_config"
					? new Promise((resolve) => {
							answer = resolve;
						})
					: (base?.(command, args) as Promise<never>),
			);
			const view = render(() => <LocalMcpPanel />);
			await vi.advanceTimersByTimeAsync(0);
			expect(nativeToolToggles(view)).toBeNull();

			answer({
				collapse_tools: false,
				mcp_capabilities: { denied: ["settings", "debug"], migration_unknown_tools: [] },
			});
			await vi.advanceTimersByTimeAsync(0);
			const configToggle = view.getByRole("checkbox", { name: "settings" }) as HTMLInputElement;
			expect(configToggle.checked).toBe(false);
			view.unmount();
		});

		// A search result for either TUIC Tools control must land on the control itself,
		// not fall back to the section heading.
		it("scrolls to each indexed TUIC Tools control, not just its section", async () => {
			uiStore.setSettingsExpertMode(true);
			const view = await renderWith({
				collapse_tools: false,
				mcp_capabilities: { denied: ["settings", "debug"], migration_unknown_tools: [] },
			});
			const entries = SETTINGS_SEARCH_INDEX.filter(
				(entry) => entry.tab === "mcp" && entry.section === "TUIC Tools" && entry.label,
			);
			expect(entries.map((entry) => entry.label)).toEqual([
				"Collapse tools — Speakeasy MCP (reduces AI context ~98%)",
				"Capabilities",
			]);
			for (const entry of entries) {
				const scrolled: Element[] = [];
				for (const el of view.container.querySelectorAll("h3, label, span")) {
					(el as HTMLElement).scrollIntoView = () => scrolled.push(el);
				}
				expect(scrollToSetting(view.container, entry.section, entry.label)).toBe(true);
				expect(scrolled.map((el) => el.textContent?.trim())).toEqual([entry.label]);
			}
			view.unmount();
		});
	});
});

// Catches: legacy config access silently enabling the replacement settings tool.
it("requires trusted UI opt-in even when settings is absent from disabled names", async () => {
	vi.useFakeTimers();
	vi.mocked(rpc).mockImplementation((command: string) => {
		if (command === "load_config")
			return Promise.resolve({
				mcp_capabilities: { denied: [], migration_unknown_tools: [] },
				settings_mcp_enabled: false,
			});
		if (command === "get_mcp_status") return Promise.resolve({ native_capabilities: nativeTools });
		return Promise.resolve(undefined);
	});
	mockInvoke.mockImplementation((command: string) =>
		Promise.resolve(
			command === "load_config"
				? { mcp_capabilities: { denied: [], migration_unknown_tools: [] }, settings_mcp_enabled: false }
				: undefined,
		),
	);
	const view = render(() => <LocalMcpPanel />);
	await vi.advanceTimersByTimeAsync(0);
	const toggle = view.getByRole("checkbox", { name: "settings" });
	expect((toggle as HTMLInputElement).checked).toBe(false);
	fireEvent.click(toggle);
	await vi.advanceTimersByTimeAsync(0);
	expect(mockInvoke).toHaveBeenCalledWith(
		"save_config",
		expect.objectContaining({ config: expect.objectContaining({ settings_mcp_enabled: true }) }),
	);
	view.unmount();
	vi.useRealTimers();
});
