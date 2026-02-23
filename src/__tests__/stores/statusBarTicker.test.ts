import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { statusBarTicker } from "../../stores/statusBarTicker";

beforeEach(() => {
  statusBarTicker.clear();
  vi.useFakeTimers();
});

afterEach(() => {
  statusBarTicker.clear();
  vi.useRealTimers();
});

describe("statusBarTicker", () => {
  it("returns null when empty", () => {
    expect(statusBarTicker.getCurrentMessage()).toBeNull();
  });

  it("returns the only message when one is added", () => {
    statusBarTicker.addMessage({
      id: "usage",
      pluginId: "claude-usage",
      text: "Claude: 29% (5h)",
      priority: 0,
      ttlMs: 60_000,
    });
    const msg = statusBarTicker.getCurrentMessage();
    expect(msg).not.toBeNull();
    expect(msg!.text).toBe("Claude: 29% (5h)");
    expect(msg!.pluginId).toBe("claude-usage");
  });

  it("shows highest priority message first", () => {
    statusBarTicker.addMessage({
      id: "low",
      pluginId: "p1",
      text: "Low priority",
      priority: 0,
      ttlMs: 0,
    });
    statusBarTicker.addMessage({
      id: "high",
      pluginId: "p1",
      text: "High priority",
      priority: 10,
      ttlMs: 0,
    });
    expect(statusBarTicker.getCurrentMessage()!.text).toBe("High priority");
  });

  it("rotates among same-priority messages", () => {
    statusBarTicker.addMessage({ id: "a", pluginId: "p1", text: "A", priority: 0, ttlMs: 0 });
    statusBarTicker.addMessage({ id: "b", pluginId: "p1", text: "B", priority: 0, ttlMs: 0 });

    const first = statusBarTicker.getCurrentMessage()!.text;
    statusBarTicker._rotate();
    const second = statusBarTicker.getCurrentMessage()!.text;

    // Should show different messages after rotation
    expect([first, second].sort()).toEqual(["A", "B"]);
  });

  it("deduplicates by id + pluginId on addMessage", () => {
    statusBarTicker.addMessage({ id: "usage", pluginId: "p1", text: "v1", priority: 0, ttlMs: 0 });
    statusBarTicker.addMessage({ id: "usage", pluginId: "p1", text: "v2", priority: 0, ttlMs: 0 });

    expect(statusBarTicker.getAll().length).toBe(1);
    expect(statusBarTicker.getAll()[0].text).toBe("v2");
  });

  it("removes message by id and pluginId", () => {
    statusBarTicker.addMessage({ id: "usage", pluginId: "p1", text: "test", priority: 0, ttlMs: 0 });
    statusBarTicker.removeMessage("usage", "p1");
    expect(statusBarTicker.getCurrentMessage()).toBeNull();
  });

  it("removes all messages for a plugin", () => {
    statusBarTicker.addMessage({ id: "a", pluginId: "p1", text: "A", priority: 0, ttlMs: 0 });
    statusBarTicker.addMessage({ id: "b", pluginId: "p1", text: "B", priority: 0, ttlMs: 0 });
    statusBarTicker.addMessage({ id: "c", pluginId: "p2", text: "C", priority: 0, ttlMs: 0 });

    statusBarTicker.removeAllForPlugin("p1");
    expect(statusBarTicker.getAll().length).toBe(1);
    expect(statusBarTicker.getAll()[0].pluginId).toBe("p2");
  });

  it("scavenges expired messages based on TTL", () => {
    statusBarTicker.addMessage({ id: "short", pluginId: "p1", text: "Short-lived", priority: 0, ttlMs: 500 });
    statusBarTicker.addMessage({ id: "long", pluginId: "p1", text: "Long-lived", priority: 0, ttlMs: 0 });

    expect(statusBarTicker.getAll().length).toBe(2);

    // Advance time past the TTL
    vi.advanceTimersByTime(600);
    statusBarTicker._scavenge();

    expect(statusBarTicker.getAll().length).toBe(1);
    expect(statusBarTicker.getAll()[0].text).toBe("Long-lived");
  });

  it("persistent messages (ttlMs=0) never expire", () => {
    statusBarTicker.addMessage({ id: "persistent", pluginId: "p1", text: "Always here", priority: 0, ttlMs: 0 });

    vi.advanceTimersByTime(1_000_000);
    statusBarTicker._scavenge();

    expect(statusBarTicker.getAll().length).toBe(1);
  });

  it("clear removes all messages", () => {
    statusBarTicker.addMessage({ id: "a", pluginId: "p1", text: "A", priority: 0, ttlMs: 0 });
    statusBarTicker.addMessage({ id: "b", pluginId: "p2", text: "B", priority: 0, ttlMs: 0 });

    statusBarTicker.clear();
    expect(statusBarTicker.getAll().length).toBe(0);
    expect(statusBarTicker.getCurrentMessage()).toBeNull();
  });
});
