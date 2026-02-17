import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

describe("NotificationManager", () => {
  let NotificationManager: typeof import("../notifications").NotificationManager;
  let manager: InstanceType<typeof NotificationManager>;

  const mockOscillator = {
    type: "",
    frequency: { setValueAtTime: vi.fn() },
    connect: vi.fn(),
    start: vi.fn(),
    stop: vi.fn(),
  };

  const mockGainNode = {
    gain: {
      setValueAtTime: vi.fn(),
      linearRampToValueAtTime: vi.fn(),
    },
    connect: vi.fn(),
  };

  const mockAudioContext = {
    state: "running",
    currentTime: 0,
    destination: {},
    createOscillator: vi.fn(() => mockOscillator),
    createGain: vi.fn(() => mockGainNode),
    resume: vi.fn().mockResolvedValue(undefined),
  };

  const OriginalAudioContext = globalThis.AudioContext;

  beforeEach(async () => {
    vi.useFakeTimers();
    vi.advanceTimersByTime(1000);
    vi.resetModules();
    // Must use a class so `new AudioContext()` works (arrow/plain fn isn't constructable).
    // Returns mockAudioContext directly so state mutations propagate.
    globalThis.AudioContext = class {
      constructor() { return mockAudioContext as unknown as AudioContext; }
    } as unknown as typeof AudioContext;
    mockAudioContext.state = "running";
    mockAudioContext.createOscillator.mockReturnValue(mockOscillator);
    mockAudioContext.createGain.mockReturnValue(mockGainNode);
    mockAudioContext.resume.mockResolvedValue(undefined);

    const mod = await import("../notifications");
    NotificationManager = mod.NotificationManager;
    manager = new NotificationManager();
  });

  afterEach(() => {
    vi.useRealTimers();
    globalThis.AudioContext = OriginalAudioContext;
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
    // playTone() returns new Promise(resolve => setTimeout(resolve, ...)).
    // With fake timers we must: start play, advance timers, then await.
    async function playAndFlush(m: InstanceType<typeof NotificationManager>, sound: Parameters<typeof m.play>[0]) {
      const p = m.play(sound);
      await vi.advanceTimersByTimeAsync(1000);
      await p;
    }

    it("plays sound when enabled", async () => {
      await playAndFlush(manager, "question");
      expect(mockAudioContext.createOscillator).toHaveBeenCalled();
    });

    it("does nothing when disabled", async () => {
      manager.setEnabled(false);
      mockAudioContext.createOscillator.mockClear();
      await playAndFlush(manager, "question");
      expect(mockAudioContext.createOscillator).not.toHaveBeenCalled();
    });

    it("does nothing when specific sound is disabled", async () => {
      manager.setSoundEnabled("question", false);
      mockAudioContext.createOscillator.mockClear();
      await playAndFlush(manager, "question");
      expect(mockAudioContext.createOscillator).not.toHaveBeenCalled();
    });

    it("rate-limits rapid plays of same sound", async () => {
      // Start play, but DON'T advance timers yet — just let it set lastPlayTime
      const p1 = manager.play("question");
      // Immediately start second play (same tick — no time passed)
      const p2 = manager.play("question");
      // Now advance to resolve both setTimeout promises
      await vi.advanceTimersByTimeAsync(1000);
      await p1;
      await p2;
      // Only one oscillator should have been created (second was rate-limited)
      expect(mockAudioContext.createOscillator).toHaveBeenCalledTimes(1);
    });

    it("allows play after rate limit interval", async () => {
      await playAndFlush(manager, "question");
      const callCount = mockAudioContext.createOscillator.mock.calls.length;
      // Advance past the 500ms rate limit
      await vi.advanceTimersByTimeAsync(600);
      await playAndFlush(manager, "question");
      expect(mockAudioContext.createOscillator.mock.calls.length).toBeGreaterThan(callCount);
    });

    it("handles audio error gracefully", async () => {
      mockAudioContext.createOscillator.mockImplementationOnce(() => {
        throw new Error("audio error");
      });
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      await playAndFlush(manager, "question");
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("Failed to play"), expect.anything());
      warnSpy.mockRestore();
    });

    it("resumes suspended audio context", async () => {
      // Play once to create the audio context
      await playAndFlush(manager, "error");
      // Now set state to suspended and advance past rate limit
      mockAudioContext.state = "suspended";
      await vi.advanceTimersByTimeAsync(600);
      await playAndFlush(manager, "error");
      expect(mockAudioContext.resume).toHaveBeenCalled();
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
    it("returns true when AudioContext exists", () => {
      expect(manager.isAvailable()).toBe(true);
    });

    it("returns false when no AudioContext available", () => {
      const savedAC = globalThis.AudioContext;
      // @ts-expect-error — intentionally removing for test
      delete globalThis.AudioContext;
      delete (window as unknown as Record<string, unknown>).webkitAudioContext;
      const m = new NotificationManager();
      expect(m.isAvailable()).toBe(false);
      globalThis.AudioContext = savedAC;
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
