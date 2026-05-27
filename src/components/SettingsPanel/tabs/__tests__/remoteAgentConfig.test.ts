import { describe, expect, it, vi } from "vitest";

vi.mock("../../../../transport", () => ({
	rpc: vi.fn(),
}));

import type { AgentsConfig } from "../../../../agents";
import { rpc } from "../../../../transport";
import { loadRemoteAgentConfig, saveRemoteAgentConfig } from "../remoteAgentConfig";

const mockRpc = vi.mocked(rpc);

describe("loadRemoteAgentConfig", () => {
	it("calls rpc with connectionId and returns the result", async () => {
		const fakeConfig = { agents: {}, headless_agent: "claude" };
		mockRpc.mockResolvedValueOnce(fakeConfig);

		const result = await loadRemoteAgentConfig("conn-123");

		expect(mockRpc).toHaveBeenCalledWith("load_agents_config", {}, "conn-123");
		expect(result).toBe(fakeConfig);
	});

	it("propagates rpc errors", async () => {
		mockRpc.mockRejectedValueOnce(new Error("connection refused"));
		await expect(loadRemoteAgentConfig("conn-bad")).rejects.toThrow("connection refused");
	});
});

describe("saveRemoteAgentConfig", () => {
	it("calls rpc with connectionId and config payload", async () => {
		mockRpc.mockResolvedValueOnce(undefined);
		const config: AgentsConfig = { agents: {}, headless_agent: "claude" };

		await saveRemoteAgentConfig("conn-123", config);

		expect(mockRpc).toHaveBeenCalledWith("save_agents_config", { config }, "conn-123");
	});

	it("propagates rpc errors", async () => {
		mockRpc.mockRejectedValueOnce(new Error("write failed"));
		await expect(saveRemoteAgentConfig("conn-bad", { agents: {} })).rejects.toThrow("write failed");
	});
});
