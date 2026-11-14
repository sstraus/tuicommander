import { describe, expect, it, vi } from "vitest";

const { mockRpc } = vi.hoisted(() => ({ mockRpc: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../transport", () => ({ rpc: mockRpc }));
vi.mock("@tauri-apps/api/core", () => ({
	Channel: class {
		onmessage: unknown;
	},
}));

import { installUpdate } from "../../utils/installUpdate";

describe("native update transport", () => {
	// Catches: using a different update resource or dropping the progress channel.
	it("sends the already checked resource and forwards native progress events", async () => {
		const progress = vi.fn();
		await installUpdate({ rid: 42 }, progress);
		const [command, args] = mockRpc.mock.calls[0];
		expect(command).toBe("install_app_update");
		expect(args.rid).toBe(42);
		args.onEvent.onmessage({ event: "Progress", data: { chunkLength: 512 } });
		expect(progress).toHaveBeenCalledWith({ event: "Progress", data: { chunkLength: 512 } });
	});
});
