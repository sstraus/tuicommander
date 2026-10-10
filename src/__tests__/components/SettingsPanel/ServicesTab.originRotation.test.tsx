import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", () => ({ rpc: vi.fn() }));

import { UpstreamMcpPanel } from "../../../components/SettingsPanel/tabs/services/UpstreamMcpPanel";
import { rpc, type UpstreamMcpServer } from "../../../transport";

afterEach(() => vi.clearAllMocks());

it.each([
	["bearer", "https://new-provider.example/mcp", true],
	["oauth2", "https://new-provider.example/mcp", true],
	["header", "https://new-provider.example/mcp", true],
	["bearer", "https://OLD-provider.example:443/other", false],
	["none", "https://new-provider.example/mcp", false],
] as const)("protects %s credentials when editing URL to %s (rejected=%s)", async (method, url, rejected) => {
	const original: UpstreamMcpServer = {
		id: "origin-rotation",
		name: "provider",
		enabled: true,
		timeout_secs: 30,
		transport: { type: "http", url: "https://old-provider.example/mcp" },
		auth: { type: "bearer", token: "" },
	};
	if (method === "oauth2") original.auth = { type: "oauth2", client_id: "public" };
	if (method === "none") delete original.auth;
	if (method === "header") {
		delete original.auth;
		original.headers = [{ name: "x-api-key", credential_ref: "00000000-0000-4000-8000-000000000001" }];
	}
	let vault = "DUMMY_OLD_PROVIDER_SECRET";
	const requests: { url: string; authorization: string }[] = [];
	vi.mocked(rpc).mockImplementation(async (command, args) => {
		if (command === "load_mcp_upstreams") return { servers: [original] };
		if (command === "get_mcp_upstream_status") return { upstreams: [] };
		if (command === "save_mcp_upstream_credential") vault = args?.token as string;
		if (command === "save_mcp_upstreams") {
			// The owned backend's real RPC contract persists AND connects before
			// returning: persist_and_apply_upstream_delta -> apply_config_diff ->
			// connect_upstream. HttpMcpClient reads the shared name-keyed vault.
			const config = args?.config as { servers: UpstreamMcpServer[] };
			const transport = config.servers[0].transport;
			if (transport.type === "http") requests.push({ url: transport.url, authorization: vault });
		}
	});
	const view = render(() => <UpstreamMcpPanel />);
	try {
		await waitFor(() => expect(view.queryByTitle("Edit")).not.toBeNull());
		fireEvent.click(view.getByTitle("Edit"));
		fireEvent.input(view.getByDisplayValue("https://old-provider.example/mcp"), {
			target: { value: url },
		});
		if (method !== "oauth2") {
			fireEvent.input(view.getByPlaceholderText("Enter new token (leave blank to keep current)"), {
				target: { value: "DUMMY_NEW_PROVIDER_SECRET" },
			});
		}
		fireEvent.click(view.getByText("Save"));
		if (!rejected) {
			await waitFor(() => expect(requests).toHaveLength(1));
			expect(requests[0].url).toBe(url);
			return;
		}
		await waitFor(() => expect(view.getByText(/Cannot change the origin/)).toBeTruthy());
		expect(requests, "a rejected edit must never connect to the new provider").toEqual([]);
		expect(vault).toBe("DUMMY_OLD_PROVIDER_SECRET");
		expect(
			vi.mocked(rpc).mock.calls.some(([cmd]) => cmd === "save_mcp_upstreams" || cmd === "save_mcp_upstream_credential"),
		).toBe(false);
	} finally {
		view.unmount();
	}
});
