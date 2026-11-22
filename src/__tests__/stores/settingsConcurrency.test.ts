import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import recordedAppConfig from "../fixtures/settings-concurrency/app-defaults.json";

const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

type Config = Record<string, unknown>;
type Save = { base: Config; config: Config };
type Settings = typeof import("../../stores/settings").settingsStore;

describe("independent settings windows", () => {
	let disk: Config;
	let packets: Save[];
	let rejectNextSave: boolean;
	let holdNextSave: Promise<void> | null;

	beforeEach(() => {
		vi.useFakeTimers();
		localStorage.clear();
		disk = structuredClone(recordedAppConfig);
		packets = [];
		rejectNextSave = false;
		holdNextSave = null;
		mockInvoke.mockReset().mockImplementation(async (command: string, args?: Save) => {
			if (command === "load_config") return structuredClone(disk);
			if (command === "save_config" && args) {
				if (rejectNextSave) {
					rejectNextSave = false;
					throw new Error("fixture save failed");
				}
				const gate = holdNextSave;
				holdNextSave = null;
				if (gate) await gate;
				packets.push(structuredClone(args));
				// Isolated config IO: keep the actual frontend packet, without repairing
				// stale fields in the test transport. No real config files are touched.
				disk = structuredClone(args.config);
			}
		});
	});

	afterEach(() => vi.useRealTimers());

	async function openWindow(): Promise<Settings> {
		// Each window has its own settings state and updateAppConfig queue.
		vi.resetModules();
		const store = (await import("../../stores/settings")).settingsStore;
		await store.hydrate();
		return store;
	}

	async function flushSave(): Promise<void> {
		await vi.advanceTimersByTimeAsync(600);
	}

	// Catches: a fresh load followed by copying every stale store-owned field
	// turns an IDE-only edit into an explicit request to re-enable Antigravity (or vice versa).
	it.each(["agents then IDE", "IDE then agents"])("preserves both edits in %s save order", async (order) => {
		const agentsWindow = await openWindow();
		const ideWindow = await openWindow();
		const edits = {
			agents: () => agentsWindow.toggleAgent("agy"),
			IDE: () => ideWindow.setIde("zed"),
		};
		const sequence = order === "agents then IDE" ? [edits.agents, edits.IDE] : [edits.IDE, edits.agents];
		for (const edit of sequence) {
			edit();
			await flushSave();
		}

		const reopened = await openWindow();
		expect(reopened.state.disabledAgents).toEqual(["agy"]);
		expect(reopened.state.ide).toBe("zed");
		expect(packets).toHaveLength(2);
		const changedFields = packets.map(({ base, config }) =>
			Object.keys(config).filter((key) => JSON.stringify(config[key]) !== JSON.stringify(base[key])),
		);
		expect(changedFields).toEqual(
			order === "agents then IDE" ? [["disabled_agents"], ["ide"]] : [["ide"], ["disabled_agents"]],
		);
	});

	// Catches: a stale IDE window reverts a peer's new Dock preference, or an
	// unchanged Dock value is sent as dirty on that window's next save.
	it("preserves the Dock preference across stale and repeated window saves", async () => {
		const dockWindow = await openWindow();
		const ideWindow = await openWindow();
		dockWindow.setHideDockIcon(false);
		await flushSave();
		ideWindow.setIde("zed");
		await flushSave();
		ideWindow.setDefaultFontSize(18);
		await flushSave();

		const reopened = await openWindow();
		expect(reopened.state.hideDockIcon).toBe(false);
		expect(reopened.state.ide).toBe("zed");
		expect(reopened.state.defaultFontSize).toBe(18);
		// An absent tray field keeps the owner default; a Dock-only edit must not
		// materialize unrelated defaults or overwrite another writer's intent.
		expect(packets[0].config.background).toEqual({ hide_dock_icon: false });
		expect(reopened.state.trayIcon).toBe("background_only");
		expect(
			packets
				.slice(1)
				.every(({ base, config }) => JSON.stringify(config.background) === JSON.stringify(base.background)),
		).toBe(true);
	});

	// Catches: a stale second window resets a peer's tray preference on an
	// unrelated save, or a tray edit drops sibling background configuration.
	it("does not revert the tray preference from a stale second window", async () => {
		disk.background = { hide_dock_icon: false, future_setting: "preserved" };
		const trayWindow = await openWindow();
		const staleWindow = await openWindow();
		trayWindow.setTrayIcon("always");
		await flushSave();
		staleWindow.setIde("zed");
		await flushSave();
		staleWindow.setDefaultFontSize(18);
		await flushSave();

		const reopened = await openWindow();
		expect(reopened.state.trayIcon).toBe("always");
		expect(reopened.state.ide).toBe("zed");
		expect(disk.background).toEqual({
			hide_dock_icon: false,
			future_setting: "preserved",
			tray_icon: "always",
		});
		expect(packets).toHaveLength(3);
		expect(
			packets
				.slice(1)
				.every(({ base, config }) => JSON.stringify(config.background) === JSON.stringify(base.background)),
		).toBe(true);
	});

	// Catches: advancing the local baseline to a merged disk snapshot reintroduces
	// stale IDE values on the same window's second save.
	it("keeps a peer's IDE through repeated local saves", async () => {
		const agentsWindow = await openWindow();
		const ideWindow = await openWindow();
		ideWindow.setIde("zed");
		await flushSave();
		agentsWindow.toggleAgent("agy");
		await flushSave();
		agentsWindow.setDefaultFontSize(18);
		await flushSave();

		const reopened = await openWindow();
		expect(reopened.state.ide).toBe("zed");
		expect(reopened.state.disabledAgents).toEqual(["agy"]);
		expect(reopened.state.defaultFontSize).toBe(18);
		expect(disk.font_size).toBe(18);
	});

	// Catches: acknowledging a failed save drops its unsaved fields on the next edit.
	it("retries unsaved edits after a failed save", async () => {
		const window = await openWindow();
		rejectNextSave = true;
		const { appLogger } = await import("../../stores/appLogger");
		const error = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		window.setIde("zed");
		try {
			await flushSave();
			expect(error.mock.calls).toEqual([["config", "Failed to save config", expect.any(Error)]]);
		} finally {
			error.mockRestore();
		}
		expect(packets).toHaveLength(0);
		window.toggleAgent("agy");
		await flushSave();

		const reopened = await openWindow();
		expect(reopened.state.ide).toBe("zed");
		expect(reopened.state.disabledAgents).toEqual(["agy"]);
	});

	// Catches: acknowledging current UI state instead of the saved snapshot loses
	// a reversal made while the first save is still in flight.
	it("persists a reversal queued while the previous save is in flight", async () => {
		const window = await openWindow();
		let release = () => {};
		holdNextSave = new Promise<void>((resolve) => {
			release = resolve;
		});
		window.setIde("zed");
		await flushSave();
		window.setIde("vscode");
		await flushSave();
		release();
		await vi.advanceTimersByTimeAsync(0);

		const reopened = await openWindow();
		expect(packets.map(({ config }) => config.ide)).toEqual(["zed", "vscode"]);
		expect(reopened.state.ide).toBe("vscode");
	});
});
