import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  NotificationManager,
  DEFAULT_NOTIFICATION_CONFIG,
  type NotificationSound,
} from "../notifications";

// Mock the invoke module — NotificationManager delegates to Rust
vi.mock("../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

// --- Tests ---

describe("DEFAULT_NOTIFICATION_CONFIG", () => {
  it("has all sounds enabled", () => {
    const sounds: NotificationSound[] = ["question", "error", "completion", "warning"];
    for (const sound of sounds) {
      expect(DEFAULT_NOTIFICATION_CONFIG.sounds[sound]).toBe(true);
    }
  });

  it("volume is 0.5", () => {
    expect(DEFAULT_NOTIFICATION_CONFIG.volume).toBe(0.5);
  });

  it("enabled is true", () => {
    expect(DEFAULT_NOTIFICATION_CONFIG.enabled).toBe(true);
  });
});

describe("NotificationManager", () => {
  let manager: NotificationManager;
  let mockInvoke: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    manager = new NotificationManager();
    vi.clearAllMocks();
    vi.useFakeTimers();
    const invokeModule = await import("../invoke");
    mockInvoke = invokeModule.invoke as ReturnType<typeof vi.fn>;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("constructor", () => {
    it("uses DEFAULT_NOTIFICATION_CONFIG when no config provided", () => {
      const config = manager.getConfig();
      expect(config.enabled).toBe(true);
      expect(config.volume).toBe(0.5);
      expect(config.sounds.question).toBe(true);
      expect(config.sounds.error).toBe(true);
      expect(config.sounds.completion).toBe(true);
      expect(config.sounds.warning).toBe(true);
    });

    it("merges partial config with defaults", () => {
      const custom = new NotificationManager({ volume: 0.8, enabled: false });
      const config = custom.getConfig();
      expect(config.volume).toBe(0.8);
      expect(config.enabled).toBe(false);
      expect(config.sounds.question).toBe(true);
    });
  });

  describe("play()", () => {
    it("calls Rust play_notification_sound when enabled", async () => {
      await manager.play("question");
      expect(mockInvoke).toHaveBeenCalledWith("play_notification_sound", {
        sound: "question",
        volume: 0.5,
      });
    });

    it("does nothing when disabled", async () => {
      manager.setEnabled(false);
      await manager.play("question");
      expect(mockInvoke).not.toHaveBeenCalled();
    });

    it("does nothing when specific sound is disabled", async () => {
      manager.setSoundEnabled("error", false);
      await manager.play("error");
      expect(mockInvoke).not.toHaveBeenCalled();
    });

    it("rate limits: does nothing within 500ms of last play for same sound", async () => {
      await manager.play("question");
      expect(mockInvoke).toHaveBeenCalledTimes(1);

      // Immediately try again — should be rate-limited
      await manager.play("question");
      expect(mockInvoke).toHaveBeenCalledTimes(1);

      // Advance past the 500ms threshold, then play should succeed again
      vi.advanceTimersByTime(501);
      await manager.play("question");
      expect(mockInvoke).toHaveBeenCalledTimes(2);
    });

    it("handles invoke error gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("audio error"));
      await expect(manager.play("question")).resolves.toBeUndefined();
    });
  });

  describe("convenience methods", () => {
    it("playQuestion calls play with 'question'", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playQuestion();
      expect(spy).toHaveBeenCalledWith("question");
    });

    it("playError calls play with 'error'", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playError();
      expect(spy).toHaveBeenCalledWith("error");
    });

    it("playCompletion calls play with 'completion'", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playCompletion();
      expect(spy).toHaveBeenCalledWith("completion");
    });

    it("playWarning calls play with 'warning'", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playWarning();
      expect(spy).toHaveBeenCalledWith("warning");
    });

    it("playInfo calls play with 'info'", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playInfo();
      expect(spy).toHaveBeenCalledWith("info");
    });
  });

  describe("updateConfig()", () => {
    it("merges new config values", () => {
      manager.updateConfig({ volume: 0.9 });
      expect(manager.getConfig().volume).toBe(0.9);
      expect(manager.getConfig().enabled).toBe(true);
    });
  });

  describe("setEnabled()", () => {
    it("enables notifications", () => {
      manager.setEnabled(false);
      expect(manager.getConfig().enabled).toBe(false);
      manager.setEnabled(true);
      expect(manager.getConfig().enabled).toBe(true);
    });
  });

  describe("setVolume()", () => {
    it("sets volume within range", () => {
      manager.setVolume(0.7);
      expect(manager.getConfig().volume).toBe(0.7);
    });

    it("clamps volume to 0 at minimum", () => {
      manager.setVolume(-0.5);
      expect(manager.getConfig().volume).toBe(0);
    });

    it("clamps volume to 1 at maximum", () => {
      manager.setVolume(2.5);
      expect(manager.getConfig().volume).toBe(1);
    });
  });

  describe("setSoundEnabled()", () => {
    it("disables a specific sound", () => {
      manager.setSoundEnabled("warning", false);
      expect(manager.getConfig().sounds.warning).toBe(false);
      expect(manager.getConfig().sounds.question).toBe(true);
    });
  });

  describe("getConfig()", () => {
    it("returns a copy of the config (not a reference)", () => {
      const config1 = manager.getConfig();
      config1.volume = 0.99;
      const config2 = manager.getConfig();
      expect(config2.volume).toBe(0.5);
    });
  });

  describe("isAvailable()", () => {
    it("always returns true (native audio via Rust)", () => {
      expect(manager.isAvailable()).toBe(true);
    });
  });
});
