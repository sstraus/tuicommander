import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  classifyError,
  calculateBackoffDelay,
  ErrorHandler,
  DEFAULT_ERROR_CONFIG,
  type ErrorHandlerConfig,
} from "../error-handler";

describe("classifyError", () => {
  it("classifies rate limit errors", () => {
    expect(classifyError("rate limit exceeded")).toBe("rate_limit");
    expect(classifyError("Too Many Requests")).toBe("rate_limit");
    expect(classifyError("quota exceeded")).toBe("rate_limit");
    expect(classifyError("Error 429")).toBe("rate_limit");
  });

  it("classifies network errors", () => {
    expect(classifyError("network error")).toBe("network");
    expect(classifyError("connection refused")).toBe("network");
    expect(classifyError("request timeout")).toBe("network");
    expect(classifyError("ECONNREFUSED")).toBe("network");
    expect(classifyError("ETIMEDOUT")).toBe("network");
  });

  it("classifies auth errors", () => {
    expect(classifyError("unauthorized access")).toBe("auth");
    expect(classifyError("authentication failed")).toBe("auth");
    expect(classifyError("invalid api key")).toBe("auth");
  });

  it("classifies validation errors", () => {
    expect(classifyError("invalid request body")).toBe("validation");
    expect(classifyError("validation error on field X")).toBe("validation");
  });

  it("returns unknown for unrecognized errors", () => {
    expect(classifyError("something went wrong")).toBe("unknown");
    expect(classifyError("")).toBe("unknown");
  });
});

describe("calculateBackoffDelay", () => {
  it("returns base delay for first retry", () => {
    // With random jitter, the delay should be approximately baseDelayMs
    const delay = calculateBackoffDelay(0, DEFAULT_ERROR_CONFIG);
    expect(delay).toBeGreaterThanOrEqual(DEFAULT_ERROR_CONFIG.baseDelayMs * 0.95);
    expect(delay).toBeLessThanOrEqual(DEFAULT_ERROR_CONFIG.baseDelayMs * 1.05);
  });

  it("increases exponentially with retry count", () => {
    // Seeded random for deterministic test
    vi.spyOn(Math, "random").mockReturnValue(0.5); // jitter = 0
    const delay0 = calculateBackoffDelay(0, DEFAULT_ERROR_CONFIG);
    const delay1 = calculateBackoffDelay(1, DEFAULT_ERROR_CONFIG);
    const delay2 = calculateBackoffDelay(2, DEFAULT_ERROR_CONFIG);

    expect(delay1).toBeGreaterThan(delay0);
    expect(delay2).toBeGreaterThan(delay1);
    vi.restoreAllMocks();
  });

  it("caps at maxDelayMs", () => {
    const config: ErrorHandlerConfig = {
      ...DEFAULT_ERROR_CONFIG,
      maxDelayMs: 5000,
    };
    const delay = calculateBackoffDelay(100, config);
    expect(delay).toBeLessThanOrEqual(config.maxDelayMs);
  });

  it("computes exact values with zero jitter (random=0.5)", () => {
    // When Math.random() = 0.5, jitter factor is (0.5 - 0.5) = 0, so jitter = 0
    vi.spyOn(Math, "random").mockReturnValue(0.5);

    // Default config: baseDelayMs=1000, backoffMultiplier=2, maxDelayMs=30000
    // delay = baseDelayMs * multiplier^retryCount
    expect(calculateBackoffDelay(0, DEFAULT_ERROR_CONFIG)).toBe(1000);  // 1000 * 2^0 = 1000
    expect(calculateBackoffDelay(1, DEFAULT_ERROR_CONFIG)).toBe(2000);  // 1000 * 2^1 = 2000
    expect(calculateBackoffDelay(2, DEFAULT_ERROR_CONFIG)).toBe(4000);  // 1000 * 2^2 = 4000
    expect(calculateBackoffDelay(3, DEFAULT_ERROR_CONFIG)).toBe(8000);  // 1000 * 2^3 = 8000
    expect(calculateBackoffDelay(4, DEFAULT_ERROR_CONFIG)).toBe(16000); // 1000 * 2^4 = 16000
    expect(calculateBackoffDelay(5, DEFAULT_ERROR_CONFIG)).toBe(30000); // 1000 * 2^5 = 32000, capped at 30000

    vi.restoreAllMocks();
  });

  it("applies jitter correctly with non-zero random values", () => {
    // When Math.random() = 1.0, jitter factor is (1.0 - 0.5) = 0.5
    // jitter = delay * 0.1 * 0.5 = delay * 0.05
    vi.spyOn(Math, "random").mockReturnValue(1.0);
    expect(calculateBackoffDelay(0, DEFAULT_ERROR_CONFIG)).toBe(1050); // 1000 + 1000*0.05
    vi.restoreAllMocks();

    // When Math.random() = 0.0, jitter factor is (0.0 - 0.5) = -0.5
    // jitter = delay * 0.1 * (-0.5) = delay * -0.05
    vi.spyOn(Math, "random").mockReturnValue(0.0);
    expect(calculateBackoffDelay(0, DEFAULT_ERROR_CONFIG)).toBe(950); // 1000 - 1000*0.05
    vi.restoreAllMocks();
  });

  it("uses custom config values correctly", () => {
    vi.spyOn(Math, "random").mockReturnValue(0.5); // zero jitter
    const config: ErrorHandlerConfig = {
      strategy: "retry",
      maxRetries: 5,
      baseDelayMs: 500,
      maxDelayMs: 10000,
      backoffMultiplier: 3,
    };
    expect(calculateBackoffDelay(0, config)).toBe(500);   // 500 * 3^0 = 500
    expect(calculateBackoffDelay(1, config)).toBe(1500);  // 500 * 3^1 = 1500
    expect(calculateBackoffDelay(2, config)).toBe(4500);  // 500 * 3^2 = 4500
    expect(calculateBackoffDelay(3, config)).toBe(10000); // 500 * 3^3 = 13500, capped at 10000
    vi.restoreAllMocks();
  });
});

describe("ErrorHandler", () => {
  let handler: ErrorHandler;

  beforeEach(() => {
    handler = new ErrorHandler();
  });

  describe("handle()", () => {
    it("retries on network errors", () => {
      const decision = handler.handle("sess-1", "network error");
      expect(decision.action).toBe("retry");
      expect(decision.delayMs).toBeDefined();
      expect(decision.reason).toContain("Retry 1/3");
    });

    it("aborts on auth errors", () => {
      const decision = handler.handle("sess-1", "unauthorized");
      expect(decision.action).toBe("abort");
      expect(decision.reason).toContain("Authentication");
    });

    it("skips on validation errors", () => {
      const decision = handler.handle("sess-1", "validation error");
      expect(decision.action).toBe("skip");
      expect(decision.reason).toContain("Validation");
    });

    it("skips after max retries exceeded", () => {
      handler.handle("sess-1", "network error"); // retry 1
      handler.handle("sess-1", "network error"); // retry 2
      handler.handle("sess-1", "network error"); // retry 3
      const decision = handler.handle("sess-1", "network error"); // exceeded
      expect(decision.action).toBe("skip");
      expect(decision.reason).toContain("Max retries");
    });

    it("increments retry counter per session", () => {
      handler.handle("sess-1", "network error");
      expect(handler.getRetryCount("sess-1")).toBe(1);
      handler.handle("sess-1", "network error");
      expect(handler.getRetryCount("sess-1")).toBe(2);
    });

    it("tracks retries independently per session", () => {
      handler.handle("sess-1", "network error");
      handler.handle("sess-2", "network error");
      expect(handler.getRetryCount("sess-1")).toBe(1);
      expect(handler.getRetryCount("sess-2")).toBe(1);
    });
  });

  describe("with abort strategy", () => {
    it("always aborts", () => {
      const abortHandler = new ErrorHandler({ strategy: "abort" });
      const decision = abortHandler.handle("sess-1", "network error");
      expect(decision.action).toBe("abort");
    });
  });

  describe("with skip strategy", () => {
    it("always skips", () => {
      const skipHandler = new ErrorHandler({ strategy: "skip" });
      const decision = skipHandler.handle("sess-1", "network error");
      expect(decision.action).toBe("skip");
    });
  });

  describe("resetRetryCount()", () => {
    it("resets the retry counter for a session", () => {
      handler.handle("sess-1", "network error");
      expect(handler.getRetryCount("sess-1")).toBe(1);
      handler.resetRetryCount("sess-1");
      expect(handler.getRetryCount("sess-1")).toBe(0);
    });
  });

  describe("getRetryCount()", () => {
    it("returns 0 for unknown session", () => {
      expect(handler.getRetryCount("unknown")).toBe(0);
    });
  });

  describe("updateConfig()", () => {
    it("merges partial config", () => {
      handler.updateConfig({ maxRetries: 5 });
      const config = handler.getConfig();
      expect(config.maxRetries).toBe(5);
      expect(config.strategy).toBe("retry"); // unchanged
    });
  });

  describe("getConfig()", () => {
    it("returns a copy of the config", () => {
      const config1 = handler.getConfig();
      const config2 = handler.getConfig();
      expect(config1).toEqual(config2);
      expect(config1).not.toBe(config2); // different objects
    });
  });

  describe("clearAll()", () => {
    it("clears all retry counters", () => {
      handler.handle("sess-1", "network error");
      handler.handle("sess-2", "network error");
      handler.clearAll();
      expect(handler.getRetryCount("sess-1")).toBe(0);
      expect(handler.getRetryCount("sess-2")).toBe(0);
    });
  });
});
