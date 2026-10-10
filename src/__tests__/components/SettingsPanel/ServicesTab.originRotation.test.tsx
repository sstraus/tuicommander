import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
vi.mock("../../../transport", () => ({ rpc: vi.fn() }));

import { UpstreamMcpPanel } from "../../../components/SettingsPanel/tabs/services/UpstreamMcpPanel";
import { rpc, type UpstreamMcpServer } from "../../../transport";

afterEach(() => vi.clearAllMocks());

it("does not expose the previous provider's Bearer token when saving a new origin and token together", async () => {
	const original: UpstreamMcpServer = {
		id: "origin-rotation",
		name: "provider",
		enabled: true,
		timeout_secs: 30,
		transport: { type: "http", url: "https://old-provider.example/mcp" },
		auth: { type: "bearer", token: "" },
	};
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
			target: { value: "https://new-provider.example/mcp" },
		});
		fireEvent.input(view.getByPlaceholderText("Enter new token (leave blank to keep current)"), {
			target: { value: "DUMMY_NEW_PROVIDER_SECRET" },
		});
		fireEvent.click(view.getByText("Save"));
		await waitFor(() => expect(requests).toHaveLength(1));
		expect(requests[0].url).toBe("https://new-provider.example/mcp");
		expect(requests[0].authorization, "new provider must never receive the old provider credential").not.toBe(
			"DUMMY_OLD_PROVIDER_SECRET",
		);
	} finally {
		view.unmount();
	}
});
