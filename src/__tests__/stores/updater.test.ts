import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

// Mocks must be defined before module import
const mockCheck = vi.fn();
const mockRelaunch = vi.fn().mockResolvedValue(undefined);
const mockRpc = vi.fn().mockResolvedValue(undefined);

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
const mockSettingsState = { updateChannel: "stable" as string };
vi.mock("../../stores/settings", () => ({
  settingsStore: { state: mockSettingsState },
}));

describe("updaterStore", () => {
  let store: typeof import("../../stores/updater").updaterStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    mockCheck.mockReset();
    mockRelaunch.mockReset().mockResolvedValue(undefined);
    mockRpc.mockReset().mockResolvedValue(undefined);
    mockSettingsState.updateChannel = "stable";

    vi.doMock("@tauri-apps/plugin-updater", () => ({ check: mockCheck }));
    vi.doMock("@tauri-apps/plugin-process", () => ({ relaunch: mockRelaunch }));
    vi.doMock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("0.0.1") }));
    vi.doMock("../../transport", () => ({ isTauri: () => true, rpc: mockRpc }));
    vi.doMock("../../stores/settings", () => ({
      settingsStore: { state: mockSettingsState },
    }));

    store = (await import("../../stores/updater")).updaterStore;
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
      mockCheck.mockReturnValue(new Promise((r) => { resolveCheck = () => r(null); }));

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
      mockCheck.mockReturnValue(new Promise(() => {}));

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

    it("passes through unrecognized errors", async () => {
      mockCheck.mockRejectedValue(new Error("some unexpected error"));

      await testInScopeAsync(async () => {
        await store.checkForUpdate();
        expect(store.state.error).toBe("some unexpected error");
      });
    });

    it("is a no-op when already checking", async () => {
      mockCheck.mockReturnValue(new Promise(() => {})); // never resolves

      await testInScopeAsync(async () => {
        store.checkForUpdate(); // starts check
        await store.checkForUpdate(); // should no-op
        expect(mockCheck).toHaveBeenCalledTimes(1);
      });
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
        available: true, version: "1.0.1-nightly.1", notes: "Nightly",
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
        available: true, version: "2.0.0-nightly.1", notes: "Nightly release",
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
        available: false, version: null, notes: null,
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
        available: false, version: null, notes: null,
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
      mockCheck.mockReturnValue(new Promise(() => {}));
      // Nightly: available
      mockRpc.mockResolvedValue({
        available: true, version: "2.0.0-nightly.1", notes: "Nightly",
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

    it("treats prerelease as NOT newer than same base version", async () => {
      mockSettingsState.updateChannel = "nightly";
      mockCheck.mockResolvedValue(null);
      // Nightly version has same base as current (0.0.1 from mock getVersion)
      mockRpc.mockResolvedValue({
        available: true, version: "0.0.1-nightly.5", notes: "Nightly",
        release_page: "https://example.com", not_found: false,
      });

      await testInScopeAsync(async () => {
        await store.checkForUpdate();
        // 0.0.1-nightly.5 strips to 0.0.1 which is NOT newer than 0.0.1
        expect(store.state.available).toBe(false);
      });
    });
  });

  describe("downloadAndInstall()", () => {
    it("is a no-op when no pending update", async () => {
      await testInScopeAsync(async () => {
        await store.downloadAndInstall();
        expect(store.state.downloading).toBe(false);
      });
    });

    it("sets downloading=true and tracks progress", async () => {
      let capturedCallback: ((event: { event: string; data: unknown }) => void) | undefined;
      const fakeUpdate = {
        version: "1.0.0",
        body: null,
        downloadAndInstall: vi.fn((cb: (event: { event: string; data: unknown }) => void) => {
          capturedCallback = cb;
          return Promise.resolve();
        }),
      };
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
        available: true, version: "2.0.0-nightly.1", notes: "Nightly",
        release_page: "https://github.com/sstraus/tuicommander/releases/tag/nightly",
        not_found: false,
      });
      const mockOpen = vi.fn();
      Object.defineProperty(window, "open", { value: mockOpen, writable: true });

      await testInScopeAsync(async () => {
        await store.checkForUpdate();
        await store.downloadAndInstall();
        expect(mockOpen).toHaveBeenCalledWith(
          "https://github.com/sstraus/tuicommander/releases/tag/nightly",
          "_blank",
        );
      });
    });

    it("sets error when install fails", async () => {
      const fakeUpdate = {
        version: "1.0.0",
        body: null,
        downloadAndInstall: vi.fn().mockRejectedValue(new Error("disk full")),
      };
      mockCheck.mockResolvedValue(fakeUpdate);

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
        available: false, version: null, notes: null,
        release_page: "https://example.com", not_found: true,
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
});
