import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { rpc } = vi.hoisted(() => ({ rpc: vi.fn() }));
vi.mock("../../../transport", () => ({ rpc }));
vi.mock("../../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { McpUsagePanel } from "../tabs/services/McpUsagePanel";

describe("McpUsagePanel", () => {
	beforeEach(() => rpc.mockReset());
	afterEach(cleanup);

	// Catches: a role's rows render under another role, or the rejected count shows as the error count.
	it("shows each role with its own operation counts, errors and rejections apart", async () => {
		rpc.mockResolvedValue({
			enabled: true,
			days: 7,
			roles: [
				{
					role: "managed_child",
					calls: 5,
					application_errors: 1,
					invalid_arguments: 3,
					catalogue_bytes: 900,
					actions: [
						{
							tool: "agent",
							action: "send",
							calls: 5,
							application_errors: 1,
							invalid_arguments: 3,
							response_bytes_p95: 127,
						},
					],
				},
			],
		});
		const { container } = render(() => <McpUsagePanel />);
		await waitFor(() => expect(container.querySelector("tbody tr")).toBeTruthy());
		expect(rpc).toHaveBeenCalledWith("get_mcp_usage");
		expect(container.textContent).toContain("managed_child");
		const cells = Array.from(container.querySelectorAll("tbody td")).map((td) => td.textContent);
		expect(cells).toEqual(["agent.send", "5", "1", "3", "127"]);
	});

	// Catches: an empty week renders a blank table instead of saying nothing was counted.
	it("says so when no calls were counted", async () => {
		rpc.mockResolvedValue({ enabled: true, days: 7, roles: [] });
		const { container } = render(() => <McpUsagePanel />);
		await waitFor(() => expect(container.textContent).toContain("No calls counted"));
	});
});
