import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: mockInvoke }));

import { TunnelsPanel } from "../../components/TunnelsPanel/TunnelsPanel";
import { invoke } from "../../invoke";
import { tunnelPanelStore } from "../../stores/tunnelPanel";

// Catches: shared profile action buttons lose Stop/audit callbacks or dismiss the panel on action clicks.
it("stops the active profile, opens its audit and closes on Escape", async () => {
	const profile = {
		id: "fixture",
		name: "Fixture tunnel",
		host: "host.example",
		port: 22,
		user: "dev",
		identity_file: null,
		forwards: [],
		options: {
			server_alive_interval: 30,
			server_alive_count_max: 3,
			strict_host_key_checking: "Yes",
			compression: true,
		},
		auto_connect: false,
	};
	mockInvoke.mockImplementation(async (command: string) =>
		command === "list_tunnel_profiles"
			? [profile]
			: command === "list_active_tunnels"
				? [{ id: "fixture", status: { type: "connected" }, started_at: "2026-10-24T00:00:00Z" }]
				: [],
	);
	tunnelPanelStore.open();
	const view = render(() => <TunnelsPanel />);
	fireEvent.click(await view.findByRole("button", { name: "Stop" }));
	await view.findByRole("button", { name: "Start" });
	expect(invoke).toHaveBeenCalledWith("stop_tunnel", { id: "fixture" });
	fireEvent.click(view.getByRole("button", { name: "Log" }));
	await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_tunnel_audit", { id: "fixture", limit: 20 }));
	expect(tunnelPanelStore.state.isOpen).toBe(true);
	fireEvent.keyDown(document, { key: "Escape" });
	expect(tunnelPanelStore.state.isOpen).toBe(false);
});
