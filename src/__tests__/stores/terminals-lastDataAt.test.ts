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
});
