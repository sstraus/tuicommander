import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock the invoke module — NotificationManager delegates to Rust
vi.mock("../invoke", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

describe("NotificationManager", () => {
  let NotificationManager: typeof import("../notifications").NotificationManager;
  let manager: InstanceType<typeof NotificationManager>;
  let mockInvoke: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    vi.useFakeTimers();
    vi.advanceTimersByTime(1000);
    vi.resetModules();

    const mod = await import("../notifications");
    NotificationManager = mod.NotificationManager;
    manager = new NotificationManager();

    const invokeModule = await import("../invoke");
    mockInvoke = invokeModule.invoke as ReturnType<typeof vi.fn>;
    mockInvoke.mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("constructor", () => {
    it("uses defaults", () => {
      const config = manager.getConfig();
      expect(config.enabled).toBe(true);
      expect(config.volume).toBe(0.5);
    });

    it("merges partial config", () => {
      const m = new NotificationManager({ volume: 0.8 });
      expect(m.getConfig().volume).toBe(0.8);
      expect(m.getConfig().enabled).toBe(true);
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
      manager.setSoundEnabled("question", false);
      await manager.play("question");
      expect(mockInvoke).not.toHaveBeenCalled();
    });

    it("rate-limits rapid plays of same sound", async () => {
      await manager.play("question");
      await manager.play("question");
      // Only first call should go through
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });

    it("allows play after rate limit interval", async () => {
      await manager.play("question");
      expect(mockInvoke).toHaveBeenCalledTimes(1);
      // Advance past the 500ms rate limit
      await vi.advanceTimersByTimeAsync(600);
      await manager.play("question");
      expect(mockInvoke).toHaveBeenCalledTimes(2);
    });

    it("handles invoke error gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("audio error"));
      // Should not throw
      await expect(manager.play("question")).resolves.toBeUndefined();
    });

    it("passes configured volume to Rust", async () => {
      manager.setVolume(0.8);
      await manager.play("completion");
      expect(mockInvoke).toHaveBeenCalledWith("play_notification_sound", {
        sound: "completion",
        volume: 0.8,
      });
    });
  });

  describe("convenience play methods", () => {
    it("playQuestion delegates to play", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playQuestion();
      expect(spy).toHaveBeenCalledWith("question");
    });

    it("playError delegates to play", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playError();
      expect(spy).toHaveBeenCalledWith("error");
    });

    it("playCompletion delegates to play", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playCompletion();
      expect(spy).toHaveBeenCalledWith("completion");
    });

    it("playWarning delegates to play", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playWarning();
      expect(spy).toHaveBeenCalledWith("warning");
    });

    it("playInfo delegates to play", async () => {
      const spy = vi.spyOn(manager, "play").mockResolvedValue(undefined);
      await manager.playInfo();
      expect(spy).toHaveBeenCalledWith("info");
    });
  });

  describe("configuration methods", () => {
    it("setEnabled updates enabled state", () => {
      manager.setEnabled(false);
      expect(manager.getConfig().enabled).toBe(false);
    });

    it("setVolume clamps to 0-1", () => {
      manager.setVolume(1.5);
      expect(manager.getConfig().volume).toBe(1);
      manager.setVolume(-0.5);
      expect(manager.getConfig().volume).toBe(0);
    });

    it("setSoundEnabled updates specific sound", () => {
      manager.setSoundEnabled("question", false);
      expect(manager.getConfig().sounds.question).toBe(false);
    });

    it("updateConfig merges config", () => {
      manager.updateConfig({ volume: 0.9, enabled: false });
      expect(manager.getConfig().volume).toBe(0.9);
      expect(manager.getConfig().enabled).toBe(false);
    });
  });

  describe("isAvailable()", () => {
    it("always returns true (native audio via Rust)", () => {
      expect(manager.isAvailable()).toBe(true);
    });
  });
});

describe("DEFAULT_NOTIFICATION_CONFIG", () => {
  it("has expected defaults", async () => {
    vi.resetModules();
    const mod = await import("../notifications");
    expect(mod.DEFAULT_NOTIFICATION_CONFIG.enabled).toBe(true);
    expect(mod.DEFAULT_NOTIFICATION_CONFIG.volume).toBe(0.5);
    expect(mod.DEFAULT_NOTIFICATION_CONFIG.sounds.question).toBe(true);
    expect(mod.DEFAULT_NOTIFICATION_CONFIG.sounds.error).toBe(true);
    expect(mod.DEFAULT_NOTIFICATION_CONFIG.sounds.completion).toBe(true);
    expect(mod.DEFAULT_NOTIFICATION_CONFIG.sounds.warning).toBe(true);
  });
});
