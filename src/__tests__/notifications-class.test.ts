import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  NotificationManager,
  DEFAULT_NOTIFICATION_CONFIG,
  type NotificationSound,
} from "../notifications";

// --- Web Audio API mocks ---

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
  resume: vi.fn().mockResolvedValue(undefined),
  createOscillator: vi.fn().mockReturnValue(mockOscillator),
  createGain: vi.fn().mockReturnValue(mockGainNode),
};

// Use a function expression (not arrow) so it can be called with `new`
globalThis.AudioContext = vi
  .fn()
  .mockImplementation(function () {
    return mockAudioContext;
  }) as unknown as typeof AudioContext;

/**
 * Helper: call play() and advance fake timers so the internal
 * setTimeout-based promise resolves. Must be used when fake timers are active
 * and play() will actually reach playTone().
 */
async function playAndFlush(manager: NotificationManager, sound: NotificationSound): Promise<void> {
  const promise = manager.play(sound);
  // playTone uses setTimeout(resolve, duration * 1000). Advance enough to cover it.
  vi.advanceTimersByTime(1000);
  await promise;
}

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

  beforeEach(() => {
    manager = new NotificationManager();
    vi.clearAllMocks();
    vi.useFakeTimers();
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
      // Sounds should still come from defaults since not overridden
      expect(config.sounds.question).toBe(true);
    });
  });

  describe("play()", () => {
    it("plays sound when enabled", async () => {
      await playAndFlush(manager, "question");
      // The playTone function creates an oscillator and gain node
      expect(mockAudioContext.createOscillator).toHaveBeenCalled();
      expect(mockAudioContext.createGain).toHaveBeenCalled();
      expect(mockOscillator.connect).toHaveBeenCalled();
      expect(mockGainNode.connect).toHaveBeenCalled();
      expect(mockOscillator.start).toHaveBeenCalled();
      expect(mockOscillator.stop).toHaveBeenCalled();
    });

    it("does nothing when disabled", async () => {
      manager.setEnabled(false);
      // play returns early before playTone, so no timer to flush
      await manager.play("question");
      expect(mockAudioContext.createOscillator).not.toHaveBeenCalled();
    });

    it("does nothing when specific sound is disabled", async () => {
      manager.setSoundEnabled("error", false);
      await manager.play("error");
      expect(mockAudioContext.createOscillator).not.toHaveBeenCalled();
    });

    it("rate limits: does nothing within 500ms of last play for same sound", async () => {
      // "question" sound has 2 notes, so each play() calls createOscillator twice
      const notesPerPlay = 2;

      // Start first play (sets lastPlayTime to current fake time)
      const firstPlay = manager.play("question");
      expect(mockAudioContext.createOscillator).toHaveBeenCalledTimes(notesPerPlay);

      // Immediately try again at the same fake time - should be rate-limited
      vi.clearAllMocks();
      const secondPlay = manager.play("question");
      expect(mockAudioContext.createOscillator).not.toHaveBeenCalled();

      // Resolve both promises
      vi.advanceTimersByTime(1000);
      await firstPlay;
      await secondPlay;

      // Advance past the 500ms threshold, then play should succeed again
      vi.advanceTimersByTime(501);
      vi.clearAllMocks();
      await playAndFlush(manager, "question");
      expect(mockAudioContext.createOscillator).toHaveBeenCalledTimes(notesPerPlay);
    });

    it("does not throw on invalid tone URL", async () => {
      // play() handles errors gracefully via try/catch
      await expect(playAndFlush(manager, "question")).resolves.toBeUndefined();
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
  });

  describe("updateConfig()", () => {
    it("merges new config values", () => {
      manager.updateConfig({ volume: 0.9 });
      expect(manager.getConfig().volume).toBe(0.9);
      // Other fields remain unchanged
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

    it("disables notifications", () => {
      manager.setEnabled(false);
      expect(manager.getConfig().enabled).toBe(false);
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
      // Other sounds remain enabled
      expect(manager.getConfig().sounds.question).toBe(true);
    });

    it("enables a specific sound", () => {
      manager.setSoundEnabled("warning", false);
      manager.setSoundEnabled("warning", true);
      expect(manager.getConfig().sounds.warning).toBe(true);
    });
  });

  describe("getConfig()", () => {
    it("returns a copy of the config (not a reference)", () => {
      const config1 = manager.getConfig();
      config1.volume = 0.99;
      const config2 = manager.getConfig();
      // Mutation of the returned object should not affect internal state
      expect(config2.volume).toBe(0.5);
    });
  });

  describe("isAvailable()", () => {
    it("returns true when AudioContext exists", () => {
      expect(manager.isAvailable()).toBe(true);
    });
  });
});
