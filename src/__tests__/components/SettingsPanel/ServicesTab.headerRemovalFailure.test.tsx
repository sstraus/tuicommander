import { render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", () => ({ rpc: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm: vi.fn().mockResolvedValue(true) }));

import { UpstreamMcpPanel } from "../../../components/SettingsPanel/tabs/services/UpstreamMcpPanel";
import { rpc } from "../../../transport";

afterEach(() => {
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.clearAllMocks();
});

it("preserves a configured upstream's header secret when removing its config fails", async () => {
	vi.useFakeTimers();
	const header = { name: "x-api-key", credential_ref: "05622c90-2335-45d7-bf6f-a7132a2a461e" };
	const server = {
		id: "saved",
		name: "example",
		transport: { type: "http", url: "https://example.com/mcp" },
		enabled: true,
		timeout_secs: 30,
		headers: [header],
	};
	// Model the two independent persistence boundaries: config write fails,
	// while deleting a vault entry succeeds and cannot be undone by reloading.
	const vault = new Map([[header.credential_ref, "DUMMY_STILL_NEEDED"]]);
	vi.mocked(rpc).mockImplementation((command, args) => {
		if (command === "load_mcp_upstreams") return Promise.resolve({ servers: [server] });
		if (command === "get_mcp_upstream_status") return Promise.resolve({ upstreams: [] });
		if (command === "save_mcp_upstreams") return Promise.reject(new Error("Configuration write failed"));
		if (command === "delete_mcp_upstream_credential" && args?.header) {
			vault.delete(header.credential_ref);
		}
		return Promise.resolve(undefined);
	});
	const view = render(() => <UpstreamMcpPanel />);
	try {
		await vi.advanceTimersByTimeAsync(0);
		view.getByTitle("Remove").click();
		await vi.advanceTimersByTimeAsync(0);
		expect(view.getByText("Error: Configuration write failed")).toBeTruthy();
		expect(view.getByTitle("Edit")).toBeTruthy();
		expect(vault.get(header.credential_ref)).toBe("DUMMY_STILL_NEEDED");
	} finally {
		view.unmount();
	}
});
