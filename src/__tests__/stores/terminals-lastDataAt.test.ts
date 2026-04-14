import { describe, it, expect, beforeEach } from "vitest";
import "../mocks/tauri";
import { terminalsStore } from "../../stores/terminals";

describe("terminalsStore - lastDataAt", () => {
  beforeEach(() => {
    for (const id of terminalsStore.getIds()) {
      terminalsStore.remove(id);
    }
  });

  it("new terminals have lastDataAt set to null", () => {
    const id = terminalsStore.add({
      sessionId: null,
      fontSize: 14,
      name: "Test",
      cwd: null,
      awaitingInput: null,
    });
    expect(terminalsStore.get(id)?.lastDataAt).toBeNull();
  });

  it("lastDataAt can be updated via update()", () => {
    const id = terminalsStore.add({
      sessionId: null,
      fontSize: 14,
      name: "Test",
      cwd: null,
      awaitingInput: null,
    });
    const now = Date.now();
    terminalsStore.update(id, { lastDataAt: now });
    expect(terminalsStore.get(id)?.lastDataAt).toBe(now);
  });

  it("touchLastDataAt writes to non-reactive map, readable via getLastDataAt", () => {
    const id = terminalsStore.add({
      sessionId: null,
      fontSize: 14,
      name: "Test",
      cwd: null,
      awaitingInput: null,
    });
    const now = Date.now();
    terminalsStore.touchLastDataAt(id, now);
    // Non-reactive read should return the value immediately
    expect(terminalsStore.getLastDataAt(id)).toBe(now);
    // Store value should still be null (not flushed yet)
    expect(terminalsStore.get(id)?.lastDataAt).toBeNull();
  });

  it("flushLastDataAt syncs non-reactive map to store", () => {
    const id = terminalsStore.add({
      sessionId: null,
      fontSize: 14,
      name: "Test",
      cwd: null,
      awaitingInput: null,
    });
    const now = Date.now();
    terminalsStore.touchLastDataAt(id, now);
    terminalsStore.flushLastDataAt();
    expect(terminalsStore.get(id)?.lastDataAt).toBe(now);
  });

  it("getLastDataAt falls back to store value when map has no entry", () => {
    const id = terminalsStore.add({
      sessionId: null,
      fontSize: 14,
      name: "Test",
      cwd: null,
      awaitingInput: null,
    });
    const now = Date.now();
    terminalsStore.update(id, { lastDataAt: now });
    expect(terminalsStore.getLastDataAt(id)).toBe(now);
  });
});
