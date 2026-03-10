import { describe, it, expect } from "vitest";

/**
 * Edge-detection for notification sounds: given a previous and current
 * awaitingInput state, returns which sound to play (or null for no sound).
 * Sound fires ONCE per state transition — repeated sets of the same state
 * must NOT re-trigger (prevents resize/reflow spam).
 */
import { getAwaitingInputSound } from "../../components/Terminal/awaitingInputSound";

describe("getAwaitingInputSound", () => {
  describe("null → state transitions (first detection)", () => {
    it("plays error on null → error", () => {
      expect(getAwaitingInputSound(null, "error")).toBe("error");
    });

    it("plays question on null → question", () => {
      expect(getAwaitingInputSound(null, "question")).toBe("question");
    });
  });

  describe("same-state re-sets (dedup — must NOT play)", () => {
    it("no sound on error → error", () => {
      expect(getAwaitingInputSound("error", "error")).toBeNull();
    });

    it("no sound on question → question", () => {
      expect(getAwaitingInputSound("question", "question")).toBeNull();
    });

    it("no sound on null → null", () => {
      expect(getAwaitingInputSound(null, null)).toBeNull();
    });
  });

  describe("cross-transitions (plays correct sound)", () => {
    it("plays question on error → question", () => {
      expect(getAwaitingInputSound("error", "question")).toBe("question");
    });

    it("plays error on question → error", () => {
      expect(getAwaitingInputSound("question", "error")).toBe("error");
    });
  });

  describe("clearing state (must NOT play)", () => {
    it("no sound on error → null", () => {
      expect(getAwaitingInputSound("error", null)).toBeNull();
    });

    it("no sound on question → null", () => {
      expect(getAwaitingInputSound("question", null)).toBeNull();
    });
  });
});
