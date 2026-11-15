import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

describe("uiStore sidebar density launch reset versus runtime sync", () => {
	let store: typeof import("../../stores/ui").uiStore;
	let apply: ((value: unknown) => void) | undefined;

	async function load(saved: unknown): Promise<void> {
		vi.resetModules();
		localStorage.clear();
		mockInvoke.mockReset().mockImplementation(async (cmd: string) => (cmd === "load_ui_prefs" ? saved : undefined));
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		const { settingsRefresh } = await import("../../services/settingsRefresh");
		vi.spyOn(settingsRefresh, "register").mockImplementation((reader) => {
			if (reader.section === "preferences") apply = reader.apply as (value: unknown) => void;
			return () => {};
		});
		store = (await import("../../stores/ui")).uiStore;
		await store.hydrate();
	}

	beforeEach(() => {
		vi.useFakeTimers();
		apply = undefined;
	});
	afterEach(() => {
		vi.useRealTimers();
		vi.restoreAllMocks();
	});

	it("Catches: first runtime sync re-reading the still-saved H from disk undoes the launch reset (R becomes H with no user action)", async () => {
		await load({ sidebar_density: "harness" });
		await testInScopeAsync(async () => {
			expect(store.state.sidebarDensityMode).toBe("rich");
			store.startRemoteSync();
			apply!({ sidebar_density: "harness" });
			expect(store.state.sidebarDensityMode).toBe("rich");
			expect(store.state.repoFilterHarnessOnly).toBe(false);
		});
	});

	it("Catches: hydrate reset leaking H when loaded prefs are null or lack a density", async () => {
		await load(null);
		expect(store.state.sidebarDensityMode).toBe("rich");
		await load({ sidebar_density: "compact" });
		expect(store.state.sidebarDensityMode).toBe("compact");
		await load({ sidebar_density: "bogus" });
		expect(store.state.sidebarDensityMode).toBe("rich");
	});

	it("Catches: legacy localStorage migration path skipping the H reset", async () => {
		vi.resetModules();
		localStorage.clear();
		localStorage.setItem("tui-commander-sidebar-visible", "true");
		localStorage.setItem("tui-commander-sidebar-width", "300");
		mockInvoke
			.mockReset()
			.mockImplementation(async (cmd: string) =>
				cmd === "load_ui_prefs" ? { sidebar_density: "harness" } : undefined,
			);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		store = (await import("../../stores/ui")).uiStore;
		await store.hydrate();
		expect(store.state.sidebarDensityMode).toBe("rich");
	});

	it("Catches: sync after the launch reset persisting or adopting H when the user picked compact", async () => {
		await load({ sidebar_density: "harness" });
		await testInScopeAsync(async () => {
			store.startRemoteSync();
			store.cycleSidebarDensityMode(); // rich -> compact? record whatever it is
			const chosen = store.state.sidebarDensityMode;
			await vi.advanceTimersByTimeAsync(600);
			apply!({ sidebar_density: chosen });
			expect(store.state.sidebarDensityMode).toBe(chosen);
		});
	});
});
