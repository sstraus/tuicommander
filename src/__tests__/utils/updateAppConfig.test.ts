import { beforeEach, describe, expect, it } from "vitest";
import { updateAppConfig } from "../../utils/updateAppConfig";
import { mockInvoke } from "../mocks/tauri";

describe("updateAppConfig", () => {
	beforeEach(() => mockInvoke.mockReset());

	it("serializes complete-config updates so concurrent writers do not clobber each other", async () => {
		let current: Record<string, unknown> = { theme: "dark", server_enabled: false };
		mockInvoke.mockImplementation(async (command: string, args?: { config?: Record<string, unknown> }) => {
			if (command === "load_config") return { ...current };
			if (command === "save_config") {
				current = { ...args?.config };
				return undefined;
			}
		});

		await Promise.all([
			updateAppConfig<Record<string, unknown>>((config) => {
				config.theme = "light";
			}),
			updateAppConfig<Record<string, unknown>>((config) => {
				config.server_enabled = true;
			}),
		]);

		expect(current).toEqual({ theme: "light", server_enabled: true });
		expect(mockInvoke.mock.calls.filter(([command]) => command === "save_config").map(([, args]) => args)).toEqual([
			{ base: { theme: "dark", server_enabled: false }, config: { theme: "light", server_enabled: false } },
			{ base: { theme: "light", server_enabled: false }, config: { theme: "light", server_enabled: true } },
		]);
		expect(mockInvoke.mock.calls.map(([command]) => command)).toEqual([
			"load_config",
			"save_config",
			"load_config",
			"save_config",
		]);
	});
	it("keeps nested saved baselines isolated from the next config mutation", async () => {
		mockInvoke.mockImplementation(async (command: string) =>
			command === "load_config" ? { nested: { values: ["original"] } } : undefined,
		);
		await updateAppConfig<{ nested: { values: string[] } }>((config) => {
			config.nested.values.push("edited");
		});
		expect(mockInvoke).toHaveBeenCalledWith("save_config", {
			base: { nested: { values: ["original"] } },
			config: { nested: { values: ["original", "edited"] } },
		});
	});
});
