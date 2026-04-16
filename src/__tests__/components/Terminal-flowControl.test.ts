import { describe, it, expect } from "vitest";
import { FlowController, HIGH_WATERMARK, LOW_WATERMARK } from "../../components/Terminal/flowControl";

describe("FlowController", () => {
  describe("initial state", () => {
    it("starts unpaused with zero pending bytes", () => {
      const fc = new FlowController();
      expect(fc.isPaused).toBe(false);
      expect(fc.pendingBytes).toBe(0);
    });
  });

  describe("trackWrite / trackDrain", () => {
    it("accumulates pending bytes on trackWrite", () => {
      const fc = new FlowController();
      fc.trackWrite(1000);
      expect(fc.pendingBytes).toBe(1000);
      fc.trackWrite(500);
      expect(fc.pendingBytes).toBe(1500);
    });

    it("decrements pending bytes on trackDrain", () => {
      const fc = new FlowController();
      fc.trackWrite(1000);
      fc.trackDrain(400);
      expect(fc.pendingBytes).toBe(600);
    });
  });

  describe("pause threshold", () => {
    it("returns 'pause' when pending exceeds HIGH_WATERMARK", () => {
      const fc = new FlowController();
      fc.trackWrite(HIGH_WATERMARK + 1);
      expect(fc.checkPause()).toBe("pause");
      expect(fc.isPaused).toBe(true);
    });

    it("returns 'none' when pending is at or below HIGH_WATERMARK", () => {
      const fc = new FlowController();
      fc.trackWrite(HIGH_WATERMARK);
      expect(fc.checkPause()).toBe("none");
      expect(fc.isPaused).toBe(false);
    });

    it("returns 'none' when already paused (no duplicate pause)", () => {
      const fc = new FlowController();
      fc.trackWrite(HIGH_WATERMARK + 1);
      expect(fc.checkPause()).toBe("pause");
      // Second check while still paused
      expect(fc.checkPause()).toBe("none");
    });
  });

  describe("resume threshold", () => {
    it("returns 'resume' when pending drops below LOW_WATERMARK after pause", () => {
      const fc = new FlowController();
      fc.trackWrite(HIGH_WATERMARK + 1);
      fc.checkPause(); // triggers pause
      fc.trackDrain(HIGH_WATERMARK + 1 - LOW_WATERMARK + 1);
      expect(fc.checkResume()).toBe("resume");
      expect(fc.isPaused).toBe(false);
    });

    it("returns 'none' when not paused", () => {
      const fc = new FlowController();
      expect(fc.checkResume()).toBe("none");
    });

    it("returns 'none' when paused but pending still above LOW_WATERMARK", () => {
      const fc = new FlowController();
      fc.trackWrite(HIGH_WATERMARK + 1);
      fc.checkPause();
      fc.trackDrain(1); // barely drained
      expect(fc.checkResume()).toBe("none");
    });
  });

  describe("reset", () => {
    it("clears pending bytes and paused state", () => {
      const fc = new FlowController();
      fc.trackWrite(HIGH_WATERMARK + 1);
      fc.checkPause();
      expect(fc.isPaused).toBe(true);
      fc.reset();
      expect(fc.isPaused).toBe(false);
      expect(fc.pendingBytes).toBe(0);
    });
  });

  describe("forceResume", () => {
    it("unpauses without checking threshold", () => {
      const fc = new FlowController();
      fc.trackWrite(HIGH_WATERMARK + 1);
      fc.checkPause();
      expect(fc.isPaused).toBe(true);
      const result = fc.forceResume();
      expect(result).toBe(true); // was paused
      expect(fc.isPaused).toBe(false);
    });

    it("returns false when not paused", () => {
      const fc = new FlowController();
      expect(fc.forceResume()).toBe(false);
    });
  });
});
