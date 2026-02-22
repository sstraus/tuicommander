import { describe, it, expect, beforeEach } from "vitest";
import { PluginLogger } from "../../plugins/pluginLogger";

describe("PluginLogger", () => {
  let logger: PluginLogger;

  beforeEach(() => {
    logger = new PluginLogger();
  });

  // -------------------------------------------------------------------------
  // Basic logging
  // -------------------------------------------------------------------------

  it("starts with zero entries", () => {
    expect(logger.size).toBe(0);
    expect(logger.getEntries()).toEqual([]);
  });

  it("logs entries and retrieves them in order", () => {
    logger.info("first");
    logger.warn("second");
    logger.error("third");

    const entries = logger.getEntries();
    expect(entries).toHaveLength(3);
    expect(entries[0].message).toBe("first");
    expect(entries[0].level).toBe("info");
    expect(entries[1].message).toBe("second");
    expect(entries[1].level).toBe("warn");
    expect(entries[2].message).toBe("third");
    expect(entries[2].level).toBe("error");
  });

  it("stores optional data payload", () => {
    const data = { code: 42, detail: "oops" };
    logger.error("fail", data);
    expect(logger.getEntries()[0].data).toEqual(data);
  });

  it("sets timestamp on each entry", () => {
    const before = Date.now();
    logger.info("msg");
    const after = Date.now();
    const ts = logger.getEntries()[0].timestamp;
    expect(ts).toBeGreaterThanOrEqual(before);
    expect(ts).toBeLessThanOrEqual(after);
  });

  it("convenience methods map to correct levels", () => {
    logger.debug("d");
    logger.info("i");
    logger.warn("w");
    logger.error("e");
    const levels = logger.getEntries().map((e) => e.level);
    expect(levels).toEqual(["debug", "info", "warn", "error"]);
  });

  // -------------------------------------------------------------------------
  // Ring buffer overflow
  // -------------------------------------------------------------------------

  it("drops oldest entries when capacity is exceeded", () => {
    const small = new PluginLogger(3);
    small.info("a");
    small.info("b");
    small.info("c");
    small.info("d"); // pushes out "a"

    expect(small.size).toBe(3);
    const messages = small.getEntries().map((e) => e.message);
    expect(messages).toEqual(["b", "c", "d"]);
  });

  it("handles multiple wraps around the buffer", () => {
    const tiny = new PluginLogger(2);
    tiny.info("1");
    tiny.info("2");
    tiny.info("3");
    tiny.info("4");
    tiny.info("5");

    expect(tiny.size).toBe(2);
    const messages = tiny.getEntries().map((e) => e.message);
    expect(messages).toEqual(["4", "5"]);
  });

  it("default capacity is 500", () => {
    expect(logger.capacity).toBe(500);
  });

  // -------------------------------------------------------------------------
  // Error count
  // -------------------------------------------------------------------------

  it("errorCount tracks only error-level entries", () => {
    logger.info("ok");
    logger.error("fail1");
    logger.warn("meh");
    logger.error("fail2");
    expect(logger.errorCount).toBe(2);
  });

  it("errorCount accounts for overflow dropping errors", () => {
    const small = new PluginLogger(2);
    small.error("old-error");
    small.info("info");
    small.info("info2"); // pushes out old-error
    expect(small.errorCount).toBe(0);
  });

  // -------------------------------------------------------------------------
  // getEntries immutability
  // -------------------------------------------------------------------------

  it("getEntries returns a new array each call", () => {
    logger.info("a");
    const a = logger.getEntries();
    const b = logger.getEntries();
    expect(a).not.toBe(b);
    expect(a).toEqual(b);
  });

  // -------------------------------------------------------------------------
  // Clear
  // -------------------------------------------------------------------------

  it("clear removes all entries", () => {
    logger.info("a");
    logger.error("b");
    logger.clear();
    expect(logger.size).toBe(0);
    expect(logger.errorCount).toBe(0);
    expect(logger.getEntries()).toEqual([]);
  });

  it("logging works normally after clear", () => {
    logger.info("before");
    logger.clear();
    logger.info("after");
    expect(logger.size).toBe(1);
    expect(logger.getEntries()[0].message).toBe("after");
  });
});
