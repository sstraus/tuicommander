import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import recordedAppConfig from "../fixtures/settings-concurrency/app-defaults.json";

const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

type Config = Record<string, unknown>;
type Save = { base: Config; config: Config };

describe("refused owner-confirmed settings edit", () => {
	let disk: Config;
	let packets: Save[];
	let HttpRpcError: typeof import("../../transport/http").HttpRpcError;

	beforeEach(() => {
		vi.useFakeTimers();
		localStorage.clear();
		disk = structuredClone(recordedAppConfig);
		packets = [];
		mockInvoke.mockReset().mockImplementation(async (command: string, args?: Save) => {
			if (command === "load_config") return structuredClone(disk);
			if (command === "save_config" && args) {
				// Mirrors PUT /config: a changed owner-confirmed field refuses the whole save.
				if (args.config.ego_executable !== args.base.ego_executable) {
					throw new HttpRpcError("save_config", 403, JSON.stringify({ error: "confirmation_required" }));
				}
				packets.push(structuredClone(args));
				disk = structuredClone(args.config);
			}
		});
	});

	afterEach(() => vi.useRealTimers());

	// Catches: the refused field stays pending, so every later ordinary save is refused too.
	it("drops the refused edit, names the confirmation requirement, and lets a later save write", async () => {
		vi.resetModules();
		({ HttpRpcError } = await import("../../transport/http"));
		const { settingsStore } = await import("../../stores/settings");
		const { toastsStore } = await import("../../stores/toasts");
		await settingsStore.hydrate();
		const original = settingsStore.state.egoExecutable;

		settingsStore.setEgoExecutable("/tmp/other-ego");
		await vi.advanceTimersByTimeAsync(600);
		expect(settingsStore.state.egoExecutable).toBe(original);
		expect(toastsStore.toasts.some((t) => /confirm/i.test(`${t.title} ${t.message}`))).toBe(true);

		settingsStore.setIde("zed");
		await vi.advanceTimersByTimeAsync(600);
		expect(packets).toHaveLength(1);
		expect(disk.ide).toBe("zed");
	});
});
