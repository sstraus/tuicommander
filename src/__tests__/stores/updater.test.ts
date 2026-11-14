import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

/** Creates a promise that never resolves on its own, but can be settled via .settle() to avoid leak detection. */
function hangingPromise(): Promise<null> & { settle: () => void } {
	let resolve!: (v: null) => void;
	const p = new Promise<null>((r) => {
		resolve = r;
	}) as Promise<null> & { settle: () => void };
	p.settle = () => resolve(null);
	return p;
}

// Mocks must be defined before module import
const mockCheck = vi.fn();
const mockRelaunch = vi.fn().mockResolvedValue(undefined);
const mockRpc = vi.fn().mockResolvedValue(undefined);
const mockInstallUpdate = vi.fn().mockResolvedValue(undefined);
vi.mock("../../utils/installUpdate", () => ({ installUpdate: mockInstallUpdate }));

vi.mock("@tauri-apps/plugin-updater", () => ({
	check: mockCheck,
}));

vi.mock("@tauri-apps/plugin-process", () => ({
	relaunch: mockRelaunch,
}));

vi.mock("@tauri-apps/api/app", () => ({
	getVersion: vi.fn().mockResolvedValue("0.0.1"),
}));

vi.mock("../../transport", () => ({
	isTauri: () => true,
	rpc: mockRpc,
}));

// Mock settings store — default to "stable" channel
const mockSettingsState = { updateChannel: "stable" as string, deferUpdateRelaunch: true };
// Terminal sessions as the updater sees them: id -> debounced busy / awaiting-input flags
const mockTerminals = new Map<string, { busy: boolean; awaitingInput: "question" | "error" | null }>();
let mockAppVersion = "0.0.1";
vi.mock("../../stores/settings", () => ({
	settingsStore: { state: mockSettingsState },
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getIds: () => [...mockTerminals.keys()],
		isBusy: (id: string) => mockTerminals.get(id)?.busy ?? false,
		get: (id: string) => mockTerminals.get(id),
	},
}));

describe("updaterStore", () => {
	let store: typeof import("../../stores/updater").updaterStore;

	const pendingPromises: Array<{ settle: () => void }> = [];

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		mockCheck.mockReset();
		mockRelaunch.mockReset().mockResolvedValue(undefined);
		mockRpc.mockReset().mockResolvedValue(undefined);
		mockInstallUpdate.mockReset().mockResolvedValue(undefined);
		mockSettingsState.updateChannel = "stable";
		mockSettingsState.deferUpdateRelaunch = true;
		mockTerminals.clear();
		localStorage.clear();
		mockAppVersion = "0.0.1";
		pendingPromises.length = 0;

		vi.doMock("@tauri-apps/plugin-updater", () => ({ check: mockCheck }));
		vi.doMock("@tauri-apps/plugin-process", () => ({ relaunch: mockRelaunch }));
		vi.doMock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => mockAppVersion) }));
		vi.doMock("../../transport", () => ({ isTauri: () => true, rpc: mockRpc }));
		vi.doMock("../../stores/settings", () => ({
			settingsStore: { state: mockSettingsState },
		}));

		vi.doMock("../../stores/terminals", () => ({
			terminalsStore: {
				getIds: () => [...mockTerminals.keys()],
				isBusy: (id: string) => mockTerminals.get(id)?.busy ?? false,
				get: (id: string) => mockTerminals.get(id),
			},
		}));

		store = (await import("../../stores/updater")).updaterStore;
	});

	afterEach(() => {
		for (const p of pendingPromises) p.settle();
		pendingPromises.length = 0;
		vi.useRealTimers();
	});

	describe("initial state", () => {
		it("defaults to not available, not checking", () => {
			testInScope(() => {
				expect(store.state.available).toBe(false);
				expect(store.state.checking).toBe(false);
				expect(store.state.downloading).toBe(false);
				expect(store.state.error).toBeNull();
				expect(store.state.downloadUrl).toBeNull();
			});
		});
	});

	describe("checkForUpdate() — stable channel", () => {
		it("sets checking=true during check", async () => {
			let resolveCheck!: (v: null) => void;
			mockCheck.mockReturnValue(
				new Promise((r) => {
					resolveCheck = () => r(null);
				}),
			);

			await testInScopeAsync(async () => {
				const checkPromise = store.checkForUpdate();
				expect(store.state.checking).toBe(true);
				resolveCheck(null);
				await checkPromise;
				expect(store.state.checking).toBe(false);
			});
		});

		it("sets available=true and version when update exists", async () => {
			const fakeUpdate = { version: "1.2.3", body: "fixes", downloadAndInstall: vi.fn() };
			mockCheck.mockResolvedValue(fakeUpdate);

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("1.2.3");
				expect(store.state.body).toBe("fixes");
				expect(store.state.downloadUrl).toBeNull();
			});
		});

		it("offers the final release to a beta user (catches: 2.0.0 stripped equal to 2.0.0-beta.1)", async () => {
			mockAppVersion = "2.0.0-beta.1";
			mockCheck.mockResolvedValue({ version: "2.0.0", body: "final", downloadAndInstall: vi.fn() });

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("2.0.0");
			});
		});

		it("offers a later beta to a beta user (catches: beta.2 stripped equal to beta.1)", async () => {
			mockAppVersion = "2.0.0-beta.1";
			mockCheck.mockResolvedValue({ version: "2.0.0-beta.2", body: "beta", downloadAndInstall: vi.fn() });

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("2.0.0-beta.2");
			});
		});

		it("never offers a beta to a user on its final release", async () => {
			mockAppVersion = "2.0.0";
			mockCheck.mockResolvedValue({ version: "2.0.0-beta.2", body: "beta", downloadAndInstall: vi.fn() });

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(false);
			});
		});

		it("sets available=false when no update", async () => {
			mockCheck.mockResolvedValue(null);

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(false);
				expect(store.state.version).toBeNull();
			});
		});

		it("times out after 10s and shows error", async () => {
			// check() never resolves
			{
				const p = hangingPromise();
				pendingPromises.push(p);
				mockCheck.mockReturnValue(p);
			}

			await testInScopeAsync(async () => {
				const checkPromise = store.checkForUpdate();
				await vi.advanceTimersByTimeAsync(10_000);
				await checkPromise;
				expect(store.state.available).toBe(false);
				expect(store.state.checking).toBe(false);
				expect(store.state.error).toBe("Update check timed out");
			});
		});

		it("sets noRelease (not error) for 404/fetch errors", async () => {
			mockCheck.mockRejectedValue(new Error("fetch error: 404 not found"));

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.noRelease).toBe(true);
				expect(store.state.error).toBeNull();
			});
		});

		it("sets noRelease (not error) for Safari 'Load failed'", async () => {
			mockCheck.mockRejectedValue(new TypeError("Load failed"));

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.noRelease).toBe(true);
				expect(store.state.error).toBeNull();
			});
		});

		it("reports a symlinked binary path as unsupported, not as an error", async () => {
			mockCheck.mockRejectedValue(
				new Error(
					"StartingBinary found current_exe() that contains a symlink on a non-allowed platform: /Users/x/src-tauri/target",
				),
			);

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.unsupported).toMatch(/symlink/);
				expect(store.state.error).toBeNull();
				expect(store.state.noRelease).toBe(false);
				expect(store.state.available).toBe(false);
			});
		});

		it("passes through unrecognized errors", async () => {
			mockCheck.mockRejectedValue(new Error("some unexpected error"));

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.error).toBe("some unexpected error");
			});
		});

		it("is a no-op when already checking", async () => {
			{
				const p = hangingPromise();
				pendingPromises.push(p);
				mockCheck.mockReturnValue(p);
			} // never resolves

			await testInScopeAsync(async () => {
				store.checkForUpdate(); // starts check
				await store.checkForUpdate(); // should no-op
				expect(mockCheck).toHaveBeenCalledTimes(1);
			});
		});
	});

	describe("checkForUpdate() — beta channel", () => {
		// Catches: beta checks stable only, or lower stable hides the next beta.
		it("offers beta.2 over a lower stable release to a beta-channel user", async () => {
			mockSettingsState.updateChannel = "beta";
			mockAppVersion = "1.7.4";
			mockCheck.mockResolvedValue({ version: "1.7.6", downloadAndInstall: vi.fn() });
			mockRpc.mockResolvedValue({
				available: true,
				version: "2.0.0-beta.2",
				notes: "Beta notes",
				release_page: "https://github.com/sstraus/tuicommander/releases/tag/v2.0.0-beta.2",
				not_found: false,
			});
			await store.checkForUpdate();
			expect(store.state.version).toBe("2.0.0-beta.2");
			expect(store.state.downloadUrl).toBe("https://github.com/sstraus/tuicommander/releases/tag/v2.0.0-beta.2");
		});

		// Catches: suffix stripping suppresses beta.1 -> beta.2 on the actual beta feed.
		it("offers the next beta when already running a beta", async () => {
			mockSettingsState.updateChannel = "beta";
			mockAppVersion = "2.0.0-beta.1";
			mockCheck.mockResolvedValue(null);
			mockRpc.mockResolvedValue({
				available: true,
				version: "2.0.0-beta.2",
				notes: "Next beta",
				release_page: "https://github.com/sstraus/tuicommander/releases/tag/v2.0.0-beta.2",
				not_found: false,
			});
			await store.checkForUpdate();
			expect(store.state.available).toBe(true);
			expect(store.state.version).toBe("2.0.0-beta.2");
		});

		// Catches: the beta feed traps testers on a prerelease after the final ships.
		it("installs the final release in-app even while the beta feed remains published", async () => {
			mockSettingsState.updateChannel = "beta";
			mockAppVersion = "2.0.0-beta.2";
			const install = vi.fn().mockResolvedValue(undefined);
			mockCheck.mockResolvedValue({ version: "2.0.0", downloadAndInstall: install });
			mockRpc.mockResolvedValue({ available: true, version: "2.0.0-beta.2", not_found: false });
			await store.checkForUpdate();
			expect(store.state.version).toBe("2.0.0");
			expect(store.state.downloadUrl).toBeNull();
			await store.downloadAndInstall();
			expect(mockInstallUpdate).toHaveBeenCalledOnce();
			expect(mockRelaunch).toHaveBeenCalledOnce();
		});

		// Catches: opting into beta globally changes the endpoint for stable users.
		it("keeps stable users on stable when a beta feed exists", async () => {
			mockSettingsState.updateChannel = "stable";
			mockAppVersion = "1.7.6";
			mockCheck.mockResolvedValue(null);
			mockRpc.mockResolvedValue({ available: true, version: "2.0.0-beta.2", not_found: false });
			await store.checkForUpdate();
			expect(store.state.available).toBe(false);
			expect(mockRpc).not.toHaveBeenCalled();
		});

		// Catches: the first beta's absent feed hides a final stable update.
		it("offers stable when the beta feed is missing", async () => {
			mockSettingsState.updateChannel = "beta";
			mockAppVersion = "2.0.0-beta.1";
			mockCheck.mockResolvedValue({ version: "2.0.0", downloadAndInstall: vi.fn() });
			mockRpc.mockResolvedValue({ available: false, version: null, not_found: true });
			await store.checkForUpdate();
			expect(store.state.version).toBe("2.0.0");
			expect(store.state.noRelease).toBe(false);
		});
	});

	describe("checkForUpdate() — nightly channel", () => {
		it("prefers stable release over nightly when both available", async () => {
			mockSettingsState.updateChannel = "nightly";
			// Stable: check() returns an update
			const fakeStableUpdate = { version: "1.0.0", body: "Stable release", downloadAndInstall: vi.fn() };
			mockCheck.mockResolvedValue(fakeStableUpdate);
			// Nightly: also available
			mockRpc.mockResolvedValue({
				available: true,
				version: "1.0.1-nightly.1",
				notes: "Nightly",
				release_page: "https://github.com/sstraus/tuicommander/releases/tag/nightly",
				not_found: false,
			});

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				// Should prefer stable (has downloadAndInstall support)
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("1.0.0");
				expect(store.state.body).toBe("Stable release");
				expect(store.state.downloadUrl).toBeNull();
				expect(mockCheck).toHaveBeenCalled();
				expect(mockRpc).toHaveBeenCalledWith("check_update_channel", { channel: "nightly" });
			});
		});

		it("falls back to nightly when no stable update available", async () => {
			mockSettingsState.updateChannel = "nightly";
			// Stable: no update
			mockCheck.mockResolvedValue(null);
			// Nightly: available
			mockRpc.mockResolvedValue({
				available: true,
				version: "2.0.0-nightly.1",
				notes: "Nightly release",
				release_page: "https://github.com/sstraus/tuicommander/releases/tag/nightly",
				not_found: false,
			});

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("2.0.0-nightly.1");
				expect(store.state.body).toBe("Nightly release");
				expect(store.state.downloadUrl).toBe("https://github.com/sstraus/tuicommander/releases/tag/nightly");
			});
		});

		it("shows stable even when nightly check fails", async () => {
			mockSettingsState.updateChannel = "nightly";
			// Stable: available
			const fakeStableUpdate = { version: "1.0.0", body: "Stable", downloadAndInstall: vi.fn() };
			mockCheck.mockResolvedValue(fakeStableUpdate);
			// Nightly: network error
			mockRpc.mockRejectedValue(new Error("Network error"));

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("1.0.0");
				expect(store.state.downloadUrl).toBeNull();
				expect(store.state.error).toBeNull();
			});
		});

		it("sets noRelease when nightly not_found and no stable update", async () => {
			mockSettingsState.updateChannel = "nightly";
			mockCheck.mockResolvedValue(null);
			mockRpc.mockResolvedValue({
				available: false,
				version: null,
				notes: null,
				release_page: "https://github.com/sstraus/tuicommander/releases/tag/nightly",
				not_found: true,
			});

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.noRelease).toBe(true);
				expect(store.state.error).toBeNull();
				expect(store.state.available).toBe(false);
			});
		});

		it("sets available=false when neither source has updates", async () => {
			mockSettingsState.updateChannel = "nightly";
			mockCheck.mockResolvedValue(null);
			mockRpc.mockResolvedValue({
				available: false,
				version: null,
				notes: null,
				release_page: "https://github.com/sstraus/tuicommander/releases/tag/nightly",
				not_found: false,
			});

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(false);
				expect(store.state.version).toBeNull();
			});
		});

		it("handles stable check timeout gracefully and falls back to nightly", async () => {
			mockSettingsState.updateChannel = "nightly";
			// Stable: times out
			{
				const p = hangingPromise();
				pendingPromises.push(p);
				mockCheck.mockReturnValue(p);
			}
			// Nightly: available
			mockRpc.mockResolvedValue({
				available: true,
				version: "2.0.0-nightly.1",
				notes: "Nightly",
				release_page: "https://github.com/sstraus/tuicommander/releases/tag/nightly",
				not_found: false,
			});

			await testInScopeAsync(async () => {
				const checkPromise = store.checkForUpdate();
				await vi.advanceTimersByTimeAsync(10_000);
				await checkPromise;
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("2.0.0-nightly.1");
				expect(store.state.downloadUrl).toBe("https://github.com/sstraus/tuicommander/releases/tag/nightly");
			});
		});

		it("sets error when both stable and nightly checks fail", async () => {
			mockSettingsState.updateChannel = "nightly";
			mockCheck.mockRejectedValue(new Error("Stable network error"));
			mockRpc.mockRejectedValue(new Error("Nightly RPC error"));

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(false);
				expect(store.state.error).toBe("Nightly RPC error");
				expect(store.state.checking).toBe(false);
			});
		});

		it("offers a nightly built after the installed release (catches: nightly ranked before its base)", async () => {
			mockSettingsState.updateChannel = "nightly";
			mockCheck.mockResolvedValue(null);
			// patch-nightly-version.sh stamps <released version>-nightly.*, built after 0.0.1 shipped
			mockRpc.mockResolvedValue({
				available: true,
				version: "0.0.1-nightly.5",
				notes: "Nightly",
				release_page: "https://example.com",
				not_found: false,
			});

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("0.0.1-nightly.5");
			});
		});

		it("does not offer a nightly user the release their nightly was built after (catches: same-base stable masks newer nightly)", async () => {
			mockSettingsState.updateChannel = "nightly";
			mockAppVersion = "1.8.0-nightly.20261012.t0900";
			mockCheck.mockResolvedValue({ version: "1.8.0", body: "stable", downloadAndInstall: vi.fn() });
			mockRpc.mockResolvedValue({
				available: true,
				version: "1.8.0-nightly.20261013.t0900",
				notes: "Nightly",
				release_page: "https://example.com",
				not_found: false,
			});

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("1.8.0-nightly.20261013.t0900");
			});
		});

		it("does not re-offer the installed nightly", async () => {
			mockSettingsState.updateChannel = "nightly";
			mockAppVersion = "1.8.0-nightly.20261012.t0900";
			mockCheck.mockResolvedValue(null);
			mockRpc.mockResolvedValue({
				available: true,
				version: "1.8.0-nightly.20261012.t0900",
				notes: "Nightly",
				release_page: "https://example.com",
				not_found: false,
			});

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(false);
			});
		});

		it("offers a later nightly to a nightly user (catches: nightly suffix stripped before comparing)", async () => {
			mockSettingsState.updateChannel = "nightly";
			mockAppVersion = "0.0.1-nightly.20261011.t0300";
			mockCheck.mockResolvedValue(null);
			mockRpc.mockResolvedValue({
				available: true,
				version: "0.0.1-nightly.20261012.t0300",
				notes: "Nightly",
				release_page: "https://example.com",
				not_found: false,
			});

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.available).toBe(true);
				expect(store.state.version).toBe("0.0.1-nightly.20261012.t0300");
			});
		});
	});

	describe("downloadAndInstall()", () => {
		// Catches: bypassing the TUIC installer silently drops the underlying IO cause.
		it("native installer failure keeps the complete cause and never relaunches", async () => {
			mockCheck.mockResolvedValue({
				rid: 7,
				version: "2.0.1",
				body: null,
				downloadAndInstall: vi.fn().mockResolvedValue(undefined),
			});
			const message = "failed to unpack executable\nCaused by: Permission denied (os error 13)";
			mockInstallUpdate.mockRejectedValue(message);
			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				await store.downloadAndInstall();
				expect(store.state.error).toBe(message);
				expect(store.state.downloading).toBe(false);
				expect(mockRelaunch).not.toHaveBeenCalled();
			});
		});

		it("is a no-op when no pending update", async () => {
			await testInScopeAsync(async () => {
				await store.downloadAndInstall();
				expect(store.state.downloading).toBe(false);
			});
		});

		it("sets downloading=true and tracks progress", async () => {
			let capturedCallback: ((event: { event: string; data: unknown }) => void) | undefined;
			const fakeUpdate = { rid: 7, version: "1.0.0", body: null };
			mockInstallUpdate.mockImplementation((_update, cb: (event: { event: string; data: unknown }) => void) => {
				capturedCallback = cb;
				return Promise.resolve();
			});
			mockCheck.mockResolvedValue(fakeUpdate);

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				const installPromise = store.downloadAndInstall();

				// Simulate progress events
				capturedCallback?.({ event: "Started", data: { contentLength: 1000 } });
				capturedCallback?.({ event: "Progress", data: { chunkLength: 500 } });
				expect(store.state.progress).toBe(50);

				capturedCallback?.({ event: "Progress", data: { chunkLength: 500 } });
				expect(store.state.progress).toBe(100);

				await installPromise;
				expect(mockRelaunch).toHaveBeenCalled();
			});
		});

		it("opens browser for nightly-only updates (no stable available)", async () => {
			mockSettingsState.updateChannel = "nightly";
			// No stable update
			mockCheck.mockResolvedValue(null);
			// Nightly available
			mockRpc.mockResolvedValue({
				available: true,
				version: "2.0.0-nightly.1",
				notes: "Nightly",
				release_page: "https://github.com/sstraus/tuicommander/releases/tag/nightly",
				not_found: false,
			});
			const mockOpen = vi.fn();
			Object.defineProperty(window, "open", { value: mockOpen, writable: true });

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				await store.downloadAndInstall();
				expect(mockOpen).toHaveBeenCalledWith("https://github.com/sstraus/tuicommander/releases/tag/nightly", "_blank");
			});
		});

		it("sets error when install fails", async () => {
			const fakeUpdate = {
				version: "1.0.0",
				body: null,
				downloadAndInstall: vi.fn().mockRejectedValue(new Error("disk full")),
			};
			mockCheck.mockResolvedValue(fakeUpdate);
			mockInstallUpdate.mockRejectedValue(new Error("disk full"));

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				await store.downloadAndInstall();
				expect(store.state.error).toBe("disk full");
				expect(store.state.downloading).toBe(false);
			});
		});
	});

	describe("dismiss()", () => {
		it("clears available state and noRelease", async () => {
			mockSettingsState.updateChannel = "nightly";
			mockRpc.mockResolvedValue({
				available: false,
				version: null,
				notes: null,
				release_page: "https://example.com",
				not_found: true,
			});

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.noRelease).toBe(true);
				store.dismiss();
				expect(store.state.available).toBe(false);
				expect(store.state.version).toBeNull();
				expect(store.state.body).toBeNull();
				expect(store.state.error).toBeNull();
				expect(store.state.noRelease).toBe(false);
				expect(store.state.downloadUrl).toBeNull();
			});
		});

		it("clears the unsupported-binary notice", async () => {
			mockCheck.mockRejectedValue(new Error("current_exe() that contains a symlink"));

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				expect(store.state.unsupported).not.toBeNull();
				store.dismiss();
				expect(store.state.unsupported).toBeNull();
			});
		});

		it("prevents downloadAndInstall after dismiss", async () => {
			const fakeUpdate = { version: "1.0.0", body: null, downloadAndInstall: vi.fn() };
			mockCheck.mockResolvedValue(fakeUpdate);

			await testInScopeAsync(async () => {
				await store.checkForUpdate();
				store.dismiss();
				await store.downloadAndInstall();
				expect(fakeUpdate.downloadAndInstall).not.toHaveBeenCalled();
			});
		});
	});

	describe("relaunch deferral", () => {
		const installedUpdate = () => ({
			version: "1.0.0",
			body: null,
			downloadAndInstall: vi.fn().mockResolvedValue(undefined),
		});

		async function install(): Promise<void> {
			mockCheck.mockResolvedValue(installedUpdate());
			await store.checkForUpdate();
			await store.downloadAndInstall();
		}

		it("does not relaunch while a session is busy and reports how many", async () => {
			mockTerminals.set("a", { busy: true, awaitingInput: null });
			mockTerminals.set("b", { busy: false, awaitingInput: null });
			await testInScopeAsync(async () => {
				await install();
				expect(mockRelaunch).not.toHaveBeenCalled();
				expect(store.state.restartPending).toBe(true);
				expect(store.state.downloading).toBe(false);
				expect(store.busyCount()).toBe(1);
			});
		});

		it("counts a session awaiting input as busy", async () => {
			mockTerminals.set("a", { busy: false, awaitingInput: "question" });
			await testInScopeAsync(async () => {
				await install();
				expect(mockRelaunch).not.toHaveBeenCalled();
				expect(store.busyCount()).toBe(1);
			});
		});

		it("relaunches once the last busy session goes idle", async () => {
			mockTerminals.set("a", { busy: true, awaitingInput: null });
			mockTerminals.set("b", { busy: true, awaitingInput: null });
			await testInScopeAsync(async () => {
				await install();
				mockTerminals.get("a")!.busy = false;
				await vi.advanceTimersByTimeAsync(10_000);
				expect(mockRelaunch).not.toHaveBeenCalled();
				mockTerminals.get("b")!.busy = false;
				await vi.advanceTimersByTimeAsync(10_000);
				expect(mockRelaunch).toHaveBeenCalledTimes(1);
			});
		});

		it("relaunches immediately when the deferral setting is off", async () => {
			mockSettingsState.deferUpdateRelaunch = false;
			mockTerminals.set("a", { busy: true, awaitingInput: null });
			await testInScopeAsync(async () => {
				await install();
				expect(mockRelaunch).toHaveBeenCalledTimes(1);
				expect(store.state.restartPending).toBe(false);
			});
		});

		it("relaunches immediately when no session is busy", async () => {
			mockTerminals.set("a", { busy: false, awaitingInput: null });
			await testInScopeAsync(async () => {
				await install();
				expect(mockRelaunch).toHaveBeenCalledTimes(1);
			});
		});

		it("restartNow() relaunches despite busy sessions and stops the idle watcher", async () => {
			mockTerminals.set("a", { busy: true, awaitingInput: null });
			await testInScopeAsync(async () => {
				await install();
				await store.restartNow();
				expect(mockRelaunch).toHaveBeenCalledTimes(1);
				mockTerminals.get("a")!.busy = false;
				await vi.advanceTimersByTimeAsync(10_000);
				expect(mockRelaunch).toHaveBeenCalledTimes(1);
			});
		});

		// Catches: no marker at relaunch, so the next boot cannot tell an update restart
		// from an ordinary one and eager resume never fires.
		it("marks the restart as caused by an update before relaunching", async () => {
			let markerAtRelaunch: string | null = null;
			mockRelaunch.mockImplementation(async () => {
				markerAtRelaunch = localStorage.getItem("tuic.updateRestartAt");
			});
			await testInScopeAsync(async () => {
				await install();
				expect(markerAtRelaunch).not.toBeNull();
			});
		});

		// Catches: a stale marker after a failed relaunch arming a later manual restart.
		it("clears the update marker when the relaunch fails", async () => {
			mockRelaunch.mockRejectedValue(new Error("relaunch denied"));
			await testInScopeAsync(async () => {
				await install();
				expect(localStorage.getItem("tuic.updateRestartAt")).toBeNull();
			});
		});

		it("surfaces a failed relaunch instead of swallowing it", async () => {
			mockRelaunch.mockRejectedValue(new Error("relaunch denied"));
			await testInScopeAsync(async () => {
				await install();
				expect(store.state.error).toBe("relaunch denied");
				expect(store.state.downloading).toBe(false);
			});
		});
	});
});
