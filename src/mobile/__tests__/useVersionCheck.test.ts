import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";

// ---------------------------------------------------------------------------
// Mock only the external boundary (network + logger), test real signal logic.
// ---------------------------------------------------------------------------

vi.mock("../../stores/appLogger", () => ({
  appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

// ---------------------------------------------------------------------------
// Browser API stubs
// ---------------------------------------------------------------------------

let mockRegistrations: Array<{ unregister: ReturnType<typeof vi.fn>; update: ReturnType<typeof vi.fn> }> = [];

const originalLocation = window.location;
const originalServiceWorker = navigator.serviceWorker;

beforeEach(() => {
  vi.useFakeTimers();
  mockRegistrations = [];

  // Stub serviceWorker.getRegistrations
  Object.defineProperty(navigator, "serviceWorker", {
    value: {
      getRegistrations: vi.fn(() => Promise.resolve(mockRegistrations)),
      ready: Promise.resolve(mockRegistrations[0]),
    },
    configurable: true,
    writable: true,
  });

  // Stub location.replace (used for cache-busting navigation)
  Object.defineProperty(window, "location", {
    value: {
      ...originalLocation,
      pathname: "/mobile",
      replace: vi.fn(),
      reload: vi.fn(),
    },
    configurable: true,
    writable: true,
  });

  // Stub fetch for /api/version
  vi.stubGlobal("fetch", vi.fn(() =>
    Promise.resolve({
      ok: true,
      json: () => Promise.resolve({ version: "1.0.0", git_hash: "abc123" }),
    }),
  ));
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  Object.defineProperty(navigator, "serviceWorker", {
    value: originalServiceWorker,
    configurable: true,
    writable: true,
  });
  Object.defineProperty(window, "location", {
    value: originalLocation,
    configurable: true,
    writable: true,
  });
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("useVersionCheck", () => {
  // Re-import fresh each test to reset module state
  async function importFresh() {
    const mod = await import("../useVersionCheck");
    return mod.useVersionCheck;
  }

  it("applyUpdate does NOT call reg.unregister()", async () => {
    const useVersionCheck = await importFresh();
    const unregisterSpy = vi.fn();
    mockRegistrations = [{ unregister: unregisterSpy, update: vi.fn() }];

    await createRoot(async (dispose) => {
      const { applyUpdate } = useVersionCheck();

      applyUpdate();
      // Let the getRegistrations() promise resolve
      await vi.advanceTimersByTimeAsync(0);

      expect(unregisterSpy).not.toHaveBeenCalled();
      dispose();
    });
  });

  it("applyUpdate uses location.replace with cache-bust param", async () => {
    const useVersionCheck = await importFresh();

    await createRoot(async (dispose) => {
      const { applyUpdate } = useVersionCheck();

      applyUpdate();
      await vi.advanceTimersByTimeAsync(0);

      expect(window.location.reload).not.toHaveBeenCalled();
      expect(window.location.replace).toHaveBeenCalledOnce();
      const url = (window.location.replace as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
      expect(url).toMatch(/^\/mobile\?v=\d+$/);
      dispose();
    });
  });

  it("serverDown becomes true after 2 consecutive poll failures", async () => {
    const useVersionCheck = await importFresh();
    let failCount = 0;
    vi.stubGlobal("fetch", vi.fn(() => {
      failCount++;
      return Promise.reject(new Error("network"));
    }));

    await createRoot(async (dispose) => {
      const { serverDown } = useVersionCheck();
      // Initial check fires immediately (1st failure)
      await vi.advanceTimersByTimeAsync(0);
      expect(serverDown()).toBe(false); // 1 failure not enough

      // Advance to next poll interval (2nd failure)
      await vi.advanceTimersByTimeAsync(60_000);
      expect(serverDown()).toBe(true);
      dispose();
    });
  });

  it("serverDown becomes false on next successful poll", async () => {
    const useVersionCheck = await importFresh();
    let shouldFail = true;
    vi.stubGlobal("fetch", vi.fn(() => {
      if (shouldFail) return Promise.reject(new Error("network"));
      return Promise.resolve({
        ok: true,
        json: () => Promise.resolve({ version: "1.0.0", git_hash: "abc123" }),
      });
    }));

    await createRoot(async (dispose) => {
      const { serverDown } = useVersionCheck();
      // 2 failures → serverDown=true
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(60_000);
      expect(serverDown()).toBe(true);

      // Recovery
      shouldFail = false;
      await vi.advanceTimersByTimeAsync(60_000);
      expect(serverDown()).toBe(false);
      dispose();
    });
  });

  it("calls registration.update() on init for iOS SW freshness", async () => {
    const updateSpy = vi.fn(() => Promise.resolve());
    mockRegistrations = [{ unregister: vi.fn(), update: updateSpy }];
    // Make navigator.serviceWorker.ready resolve with our mock
    Object.defineProperty(navigator, "serviceWorker", {
      value: {
        getRegistrations: vi.fn(() => Promise.resolve(mockRegistrations)),
        ready: Promise.resolve({ update: updateSpy }),
      },
      configurable: true,
      writable: true,
    });

    const useVersionCheck = await importFresh();

    await createRoot(async (dispose) => {
      useVersionCheck();
      // Let the ready promise + update() resolve
      await vi.advanceTimersByTimeAsync(0);

      expect(updateSpy).toHaveBeenCalled();
      dispose();
    });
  });
});
