import { fireEvent, render, within } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn(),
}));

import { LocalMcpPanel } from "../../../components/SettingsPanel/tabs/services/LocalMcpPanel";
import type { McpCapability } from "../../../components/SettingsPanel/tabs/services/servicesShared";
import { rpc } from "../../../transport";
import { mockInvoke } from "../../mocks/tauri";

// Match mcp_capabilities::catalog: permission rows now carry grouping metadata.
const telegramCapability = {
	name: "telegram",
	title: "Telegram",
	description: "Send external messages; requires Telegram configuration.",
	domain: "telegram",
	parent: null,
	section: "conditional",
} satisfies McpCapability;

afterEach(() => {
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.clearAllMocks();
});

// Catches: Settings snapshots the initial registry and misses tools supplied
// by a restarted backend on a subsequent status poll.
it("refreshes native switches when a later backend poll registers a tool", async () => {
	vi.useFakeTimers();
	let registered = false;
	const config = {
		mcp_capabilities: { denied: ["telegram", "diagnostics"], migration_unknown_tools: [] },
		collapse_tools: false,
	};
	mockInvoke.mockImplementation(async (command) => (command === "load_config" ? structuredClone(config) : undefined));
	vi.mocked(rpc).mockImplementation((command: string) => {
		if (command === "get_mcp_status") {
			return Promise.resolve({
				enabled: true,
				running: true,
				native_capabilities: registered ? [telegramCapability] : [],
			});
		}
		if (command === "load_config") return Promise.resolve(structuredClone(config));
		return Promise.resolve({ connected: false });
	});
	const view = render(() => <LocalMcpPanel />);
	try {
		await vi.advanceTimersByTimeAsync(0);
		expect(view.queryByRole("checkbox", { name: telegramCapability.title })).toBeNull();
		registered = true;
		await vi.advanceTimersByTimeAsync(3000);
		const toggle = within(view.getByRole("region", { name: "Conditional" })).getByRole("checkbox", {
			name: telegramCapability.title,
		}) as HTMLInputElement;
		expect(toggle.checked).toBe(false);
		expect(view.getByText(telegramCapability.description)).toBeDefined();
		fireEvent.click(toggle);
		await vi.advanceTimersByTimeAsync(0);
		expect(toggle.checked).toBe(true);
		expect(mockInvoke).toHaveBeenCalledWith(
			"save_config",
			expect.objectContaining({
				config: { ...config, mcp_capabilities: { denied: ["diagnostics"], migration_unknown_tools: [] } },
			}),
		);
	} finally {
		view.unmount();
	}
});
