import { describe, it, expect } from "vitest";
import { createGlobalHotkeyCaptureGuard } from "../KeyboardShortcutsTab";

/**
 * The KeyComboCapture widget emits two orthogonal events: `onCapturingChange`
 * (start/stop of the capture session) and `onChange` (a valid combo was
 * captured). Escape cancels the capture WITHOUT firing `onChange`, so the
 * component used to leave the global hotkey cleared — the user lost their
 * previously configured hotkey just by opening and escaping the picker.
 *
 * The guard owns the "remember pre-capture value → restore on cancel" policy
 * so it can be verified in isolation from the Solid component.
 * (Story 1280-6717.)
 */
function makeApi(initial: string | null) {
  let current = initial;
  const writes: Array<string | null> = [];
  return {
    reads: 0,
    writes,
    getCurrent: function (this: { reads: number }) {
      (this as { reads: number }).reads++;
      return current;
    },
    setCurrent: async (value: string | null) => {
      current = value;
      writes.push(value);
    },
    now: () => current,
  };
}

describe("createGlobalHotkeyCaptureGuard", () => {
  it("clears the hotkey on capture start and restores it when capture ends without onChange", async () => {
    // Regression: Escape/blur closes capture with no onChange — the pre-capture
    // value must come back.
    const api = makeApi("Super+J");
    const guard = createGlobalHotkeyCaptureGuard(api);

    await guard.onCapturingChange(true);
    expect(api.now()).toBeNull();

    await guard.onCapturingChange(false);
    expect(api.now()).toBe("Super+J");
    expect(api.writes).toEqual([null, "Super+J"]);
  });

  it("does not restore when a new combo was captured during the session", async () => {
    // notifyChange() is called by the component's onChange handler BEFORE the
    // capture ends, so the guard must not clobber the new value.
    const api = makeApi("Super+J");
    const guard = createGlobalHotkeyCaptureGuard(api);

    await guard.onCapturingChange(true);
    await api.setCurrent("Super+K"); // simulates handleGlobalHotkeyChange persisting the new combo
    guard.notifyChange();
    await guard.onCapturingChange(false);

    expect(api.now()).toBe("Super+K");
  });

  it("is a no-op when starting capture with no pre-existing hotkey", async () => {
    // Nothing was registered, so there's nothing to clear and nothing to restore.
    const api = makeApi(null);
    const guard = createGlobalHotkeyCaptureGuard(api);

    await guard.onCapturingChange(true);
    await guard.onCapturingChange(false);

    expect(api.now()).toBeNull();
    expect(api.writes).toEqual([]);
  });

  it("forgets the saved value after the session so a second Escape-cancel doesn't over-restore", async () => {
    // If the guard kept the saved value around across sessions, a user who
    // clears the hotkey via the Clear button and then opens + escapes the
    // picker would see the hotkey silently come back.
    const api = makeApi("Super+J");
    const guard = createGlobalHotkeyCaptureGuard(api);

    await guard.onCapturingChange(true);
    await guard.onCapturingChange(false);
    // User clears the hotkey via the explicit Clear button:
    await api.setCurrent(null);
    api.writes.length = 0;

    // Reopen + escape — guard must not restore "Super+J" from the earlier session.
    await guard.onCapturingChange(true);
    await guard.onCapturingChange(false);

    expect(api.now()).toBeNull();
    expect(api.writes).toEqual([]); // no writes because there was no hotkey to save or restore
  });

  it("extra onCapturingChange(false) calls without a matching start are a no-op", async () => {
    // Defensive: a stray false should not restore a stale saved value.
    const api = makeApi("Super+J");
    const guard = createGlobalHotkeyCaptureGuard(api);

    await guard.onCapturingChange(false);

    expect(api.now()).toBe("Super+J");
    expect(api.writes).toEqual([]);
  });
});
