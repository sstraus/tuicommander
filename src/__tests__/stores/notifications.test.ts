import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
const mockSetBadgeCount = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setBadgeCount: mockSetBadgeCount,
  }),
}));

// Mock the notificationManager before importing the store
vi.mock("../../notifications", () => ({
  notificationManager: {
    play: vi.fn().mockResolvedValue(undefined),
    playQuestion: vi.fn().mockResolvedValue(undefined),
    playError: vi.fn().mockResolvedValue(undefined),
    playCompletion: vi.fn().mockResolvedValue(undefined),
    playWarning: vi.fn().mockResolvedValue(undefined),
    playInfo: vi.fn().mockResolvedValue(undefined),
    updateConfig: vi.fn(),
    setEnabled: vi.fn(),
    setVolume: vi.fn(),
    setSoundEnabled: vi.fn(),
    isAvailable: vi.fn().mockReturnValue(true),
    getConfig: vi.fn().mockReturnValue({
      enabled: true,
      volume: 0.5,
      sounds: { question: true, error: true, completion: true, warning: true, info: true },
    }),
  },
  DEFAULT_NOTIFICATION_CONFIG: {
    enabled: true,
    volume: 0.5,
    sounds: { question: true, error: true, completion: true, warning: true, info: true },
  },
}));

describe("notificationsStore", () => {
  let store: typeof import("../../stores/notifications").notificationsStore;
  let mockManager: {
    play: ReturnType<typeof vi.fn>;
    playQuestion: ReturnType<typeof vi.fn>;
    playError: ReturnType<typeof vi.fn>;
    playCompletion: ReturnType<typeof vi.fn>;
    playWarning: ReturnType<typeof vi.fn>;
    playInfo: ReturnType<typeof vi.fn>;
    updateConfig: ReturnType<typeof vi.fn>;
    setEnabled: ReturnType<typeof vi.fn>;
    setVolume: ReturnType<typeof vi.fn>;
    setSoundEnabled: ReturnType<typeof vi.fn>;
    isAvailable: ReturnType<typeof vi.fn>;
    getConfig: ReturnType<typeof vi.fn>;
  };

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    mockSetBadgeCount.mockReset().mockResolvedValue(undefined);
    localStorage.clear();

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    vi.doMock("@tauri-apps/api/window", () => ({
      getCurrentWindow: () => ({
        setBadgeCount: mockSetBadgeCount,
      }),
    }));

    // Re-mock after resetModules
    vi.doMock("../../notifications", () => ({
      notificationManager: {
        play: vi.fn().mockResolvedValue(undefined),
        playQuestion: vi.fn().mockResolvedValue(undefined),
        playError: vi.fn().mockResolvedValue(undefined),
        playCompletion: vi.fn().mockResolvedValue(undefined),
        playWarning: vi.fn().mockResolvedValue(undefined),
        updateConfig: vi.fn(),
        setEnabled: vi.fn(),
        setVolume: vi.fn(),
        setSoundEnabled: vi.fn(),
        isAvailable: vi.fn().mockReturnValue(true),
        getConfig: vi.fn().mockReturnValue({
          enabled: true,
          volume: 0.5,
          sounds: { question: true, error: true, completion: true, warning: true },
        }),
      },
      DEFAULT_NOTIFICATION_CONFIG: {
        enabled: true,
        volume: 0.5,
        sounds: { question: true, error: true, completion: true, warning: true },
      },
    }));

    const notifMod = await import("../../notifications");
    mockManager = notifMod.notificationManager as unknown as typeof mockManager;
    store = (await import("../../stores/notifications")).notificationsStore;
  });

  describe("defaults", () => {
    it("has correct defaults", () => {
      createRoot((dispose) => {
        expect(store.state.config.enabled).toBe(true);
        expect(store.state.config.volume).toBe(0.5);
        expect(store.state.config.sounds.question).toBe(true);
        expect(store.state.config.sounds.error).toBe(true);
        expect(store.state.config.sounds.completion).toBe(true);
        expect(store.state.config.sounds.warning).toBe(true);
        dispose();
      });
    });
  });

  describe("setEnabled()", () => {
    it("enables/disables notifications", () => {
      createRoot((dispose) => {
        store.setEnabled(false);
        expect(store.state.config.enabled).toBe(false);
        expect(store.isEnabled()).toBe(false);
        dispose();
      });
    });

    it("persists via Tauri invoke", () => {
      createRoot((dispose) => {
        store.setEnabled(false);
        expect(mockInvoke).toHaveBeenCalledWith(
          "save_notification_config",
          expect.objectContaining({ config: expect.objectContaining({ enabled: false }) }),
        );
        dispose();
      });
    });
  });

  describe("setVolume()", () => {
    it("sets volume", () => {
      createRoot((dispose) => {
        store.setVolume(0.8);
        expect(store.state.config.volume).toBe(0.8);
        dispose();
      });
    });

    it("clamps volume to valid range", () => {
      createRoot((dispose) => {
        store.setVolume(2);
        expect(store.state.config.volume).toBe(1);
        store.setVolume(-0.5);
        expect(store.state.config.volume).toBe(0);
        dispose();
      });
    });
  });

  describe("setSoundEnabled()", () => {
    it("enables/disables specific sound", () => {
      createRoot((dispose) => {
        store.setSoundEnabled("question", false);
        expect(store.state.config.sounds.question).toBe(false);
        dispose();
      });
    });
  });

  describe("isEnabled()", () => {
    it("returns enabled state", () => {
      createRoot((dispose) => {
        expect(store.isEnabled()).toBe(true);
        store.setEnabled(false);
        expect(store.isEnabled()).toBe(false);
        dispose();
      });
    });
  });

  describe("isSoundEnabled()", () => {
    it("checks both global and per-sound enabled", () => {
      createRoot((dispose) => {
        expect(store.isSoundEnabled("question")).toBe(true);

        store.setSoundEnabled("question", false);
        expect(store.isSoundEnabled("question")).toBe(false);

        store.setSoundEnabled("question", true);
        store.setEnabled(false);
        expect(store.isSoundEnabled("question")).toBe(false);
        dispose();
      });
    });
  });

  describe("reset()", () => {
    it("resets to defaults", () => {
      createRoot((dispose) => {
        store.setEnabled(false);
        store.setVolume(0.1);
        store.setSoundEnabled("question", false);
        store.reset();
        expect(store.state.config.enabled).toBe(true);
        expect(store.state.config.volume).toBe(0.5);
        expect(store.state.config.sounds.question).toBe(true);
        dispose();
      });
    });
  });

  describe("hydrate()", () => {
    it("loads config from Tauri backend", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_notification_config") {
          return Promise.resolve({
            enabled: false,
            volume: 0.3,
            sounds: { question: false, error: true, completion: true, warning: true },
          });
        }
        return Promise.resolve(undefined);
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.config.enabled).toBe(false);
        expect(store.state.config.volume).toBe(0.3);
        expect(store.state.config.sounds.question).toBe(false);
        dispose();
      });
    });

    it("migrates from localStorage on first run", async () => {
      localStorage.setItem("tui-commander-notifications", JSON.stringify({
        enabled: false,
        volume: 0.7,
        sounds: { question: false, error: true, completion: true, warning: true },
      }));

      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_notification_config") {
          return Promise.resolve({
            enabled: false,
            volume: 0.7,
            sounds: { question: false, error: true, completion: true, warning: true },
          });
        }
        return Promise.resolve(undefined);
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        // Should have saved legacy data to Tauri
        expect(mockInvoke).toHaveBeenCalledWith(
          "save_notification_config",
          expect.objectContaining({ config: expect.objectContaining({ volume: 0.7 }) }),
        );
        // Should have removed legacy key
        expect(localStorage.getItem("tui-commander-notifications")).toBeNull();
        dispose();
      });
    });

    it("handles corrupt localStorage data gracefully", async () => {
      localStorage.setItem("tui-commander-notifications", "not-json{{{");

      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_notification_config") {
          return Promise.resolve({
            enabled: true,
            volume: 0.5,
            sounds: { question: true, error: true, completion: true, warning: true },
          });
        }
        return Promise.resolve(undefined);
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        // Should have removed corrupt data
        expect(localStorage.getItem("tui-commander-notifications")).toBeNull();
        dispose();
      });
    });

    it("falls back to defaults when Tauri invoke fails", async () => {
      mockInvoke.mockRejectedValue(new Error("invoke failed"));
      const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.config.enabled).toBe(true);
        expect(store.state.config.volume).toBe(0.5);
        dispose();
      });

      debugSpy.mockRestore();
    });
  });

  describe("play()", () => {
    it("delegates to notificationManager.play()", async () => {
      await store.play("question");
      expect(mockManager.play).toHaveBeenCalledWith("question");
    });

    it("plays error sound", async () => {
      await store.play("error");
      expect(mockManager.play).toHaveBeenCalledWith("error");
    });

    it("plays completion sound", async () => {
      await store.play("completion");
      expect(mockManager.play).toHaveBeenCalledWith("completion");
    });

    it("plays warning sound", async () => {
      await store.play("warning");
      expect(mockManager.play).toHaveBeenCalledWith("warning");
    });
  });

  describe("playQuestion()", () => {
    it("plays question sound via play()", async () => {
      await store.playQuestion();
      expect(mockManager.play).toHaveBeenCalledWith("question");
    });
  });

  describe("playError()", () => {
    it("plays error sound via play()", async () => {
      await store.playError();
      expect(mockManager.play).toHaveBeenCalledWith("error");
    });
  });

  describe("playCompletion()", () => {
    it("plays completion sound via play()", async () => {
      await store.playCompletion();
      expect(mockManager.play).toHaveBeenCalledWith("completion");
    });
  });

  describe("playWarning()", () => {
    it("plays warning sound via play()", async () => {
      await store.playWarning();
      expect(mockManager.play).toHaveBeenCalledWith("warning");
    });
  });

  describe("playInfo()", () => {
    it("plays info sound via play()", async () => {
      await store.playInfo();
      expect(mockManager.play).toHaveBeenCalledWith("info");
    });
  });

  describe("badge count", () => {
    it("defaults badgeCount to 0", () => {
      createRoot((dispose) => {
        expect(store.state.badgeCount).toBe(0);
        dispose();
      });
    });

    it("incrementBadge increments count and calls setBadgeCount", async () => {
      await createRoot(async (dispose) => {
        await store.incrementBadge();
        expect(store.state.badgeCount).toBe(1);
        expect(mockSetBadgeCount).toHaveBeenCalledWith(1);

        await store.incrementBadge();
        expect(store.state.badgeCount).toBe(2);
        expect(mockSetBadgeCount).toHaveBeenCalledWith(2);
        dispose();
      });
    });

    it("clearBadge resets count and calls setBadgeCount(0)", async () => {
      await createRoot(async (dispose) => {
        await store.incrementBadge();
        await store.incrementBadge();
        expect(store.state.badgeCount).toBe(2);

        await store.clearBadge();
        expect(store.state.badgeCount).toBe(0);
        expect(mockSetBadgeCount).toHaveBeenCalledWith(0);
        dispose();
      });
    });

    it("clearBadge is a no-op when count is already 0", async () => {
      await createRoot(async (dispose) => {
        await store.clearBadge();
        expect(mockSetBadgeCount).not.toHaveBeenCalled();
        dispose();
      });
    });
  });

  describe("testSound()", () => {
    it("temporarily enables everything, plays, then restores state", async () => {
      await createRoot(async (dispose) => {
        // Disable notifications and question sound
        store.setEnabled(false);
        store.setSoundEnabled("question", false);

        await store.testSound("question");

        // Should have temporarily enabled, played, and restored
        const calls = mockManager.setEnabled.mock.calls;
        const soundCalls = mockManager.setSoundEnabled.mock.calls;

        expect(calls).toContainEqual([true]); // Temporarily enable
        expect(calls).toContainEqual([false]); // Restore
        expect(soundCalls).toContainEqual(["question", true]); // Temporarily enable sound
        expect(soundCalls).toContainEqual(["question", false]); // Restore sound

        expect(mockManager.play).toHaveBeenCalledWith("question");

        dispose();
      });
    });

    it("restores original enabled state when it was already enabled", async () => {
      await createRoot(async (dispose) => {
        await store.testSound("question");

        expect(mockManager.setEnabled).toHaveBeenCalledWith(true);
        expect(mockManager.play).toHaveBeenCalledWith("question");

        dispose();
      });
    });
  });
});
