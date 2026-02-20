import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

// Mocks must be defined before module import
const mockCheck = vi.fn();
const mockRelaunch = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: mockCheck,
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: mockRelaunch,
}));

describe("updaterStore", () => {
  let store: typeof import("../../stores/updater").updaterStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    mockCheck.mockReset();
    mockRelaunch.mockReset().mockResolvedValue(undefined);

    vi.doMock("@tauri-apps/plugin-updater", () => ({ check: mockCheck }));
    vi.doMock("@tauri-apps/plugin-process", () => ({ relaunch: mockRelaunch }));

    store = (await import("../../stores/updater")).updaterStore;
  });

  describe("initial state", () => {
    it("defaults to not available, not checking", () => {
      createRoot((dispose) => {
        expect(store.state.available).toBe(false);
        expect(store.state.checking).toBe(false);
        expect(store.state.downloading).toBe(false);
        expect(store.state.error).toBeNull();
        dispose();
      });
    });
  });

  describe("checkForUpdate()", () => {
    it("sets checking=true during check", async () => {
      let resolveCheck!: (v: null) => void;
      mockCheck.mockReturnValue(new Promise((r) => { resolveCheck = () => r(null); }));

      await createRoot(async (dispose) => {
        const checkPromise = store.checkForUpdate();
        expect(store.state.checking).toBe(true);
        resolveCheck(null);
        await checkPromise;
        expect(store.state.checking).toBe(false);
        dispose();
      });
    });

    it("sets available=true and version when update exists", async () => {
      const fakeUpdate = { version: "1.2.3", body: "fixes", downloadAndInstall: vi.fn() };
      mockCheck.mockResolvedValue(fakeUpdate);

      await createRoot(async (dispose) => {
        await store.checkForUpdate();
        expect(store.state.available).toBe(true);
        expect(store.state.version).toBe("1.2.3");
        expect(store.state.body).toBe("fixes");
        dispose();
      });
    });

    it("sets available=false when no update", async () => {
      mockCheck.mockResolvedValue(null);

      await createRoot(async (dispose) => {
        await store.checkForUpdate();
        expect(store.state.available).toBe(false);
        expect(store.state.version).toBeNull();
        dispose();
      });
    });

    it("times out after 10s and resolves with no update", async () => {
      // check() never resolves
      mockCheck.mockReturnValue(new Promise(() => {}));

      await createRoot(async (dispose) => {
        const checkPromise = store.checkForUpdate();
        await vi.advanceTimersByTimeAsync(10_000);
        await checkPromise;
        expect(store.state.available).toBe(false);
        expect(store.state.checking).toBe(false);
        dispose();
      });
    });

    it("maps 404/fetch errors to friendly message", async () => {
      mockCheck.mockRejectedValue(new Error("fetch error: 404 not found"));

      await createRoot(async (dispose) => {
        await store.checkForUpdate();
        expect(store.state.error).toBe("No published releases found yet");
        dispose();
      });
    });

    it("passes through unrecognized errors", async () => {
      mockCheck.mockRejectedValue(new Error("some unexpected error"));

      await createRoot(async (dispose) => {
        await store.checkForUpdate();
        expect(store.state.error).toBe("some unexpected error");
        dispose();
      });
    });

    it("is a no-op when already checking", async () => {
      mockCheck.mockReturnValue(new Promise(() => {})); // never resolves

      await createRoot(async (dispose) => {
        store.checkForUpdate(); // starts check
        await store.checkForUpdate(); // should no-op
        expect(mockCheck).toHaveBeenCalledTimes(1);
        dispose();
      });
    });
  });

  describe("downloadAndInstall()", () => {
    it("is a no-op when no pending update", async () => {
      await createRoot(async (dispose) => {
        await store.downloadAndInstall();
        expect(store.state.downloading).toBe(false);
        dispose();
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

      await createRoot(async (dispose) => {
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
        dispose();
      });
    });

    it("sets error when install fails", async () => {
      const fakeUpdate = {
        version: "1.0.0",
        body: null,
        downloadAndInstall: vi.fn().mockRejectedValue(new Error("disk full")),
      };
      mockCheck.mockResolvedValue(fakeUpdate);

      await createRoot(async (dispose) => {
        await store.checkForUpdate();
        await store.downloadAndInstall();
        expect(store.state.error).toBe("disk full");
        expect(store.state.downloading).toBe(false);
        dispose();
      });
    });
  });

  describe("dismiss()", () => {
    it("clears available state", async () => {
      const fakeUpdate = { version: "1.0.0", body: null, downloadAndInstall: vi.fn() };
      mockCheck.mockResolvedValue(fakeUpdate);

      await createRoot(async (dispose) => {
        await store.checkForUpdate();
        expect(store.state.available).toBe(true);
        store.dismiss();
        expect(store.state.available).toBe(false);
        expect(store.state.version).toBeNull();
        expect(store.state.body).toBeNull();
        expect(store.state.error).toBeNull();
        dispose();
      });
    });

    it("prevents downloadAndInstall after dismiss", async () => {
      const fakeUpdate = { version: "1.0.0", body: null, downloadAndInstall: vi.fn() };
      mockCheck.mockResolvedValue(fakeUpdate);

      await createRoot(async (dispose) => {
        await store.checkForUpdate();
        store.dismiss();
        await store.downloadAndInstall();
        expect(fakeUpdate.downloadAndInstall).not.toHaveBeenCalled();
        dispose();
      });
    });
  });
});
