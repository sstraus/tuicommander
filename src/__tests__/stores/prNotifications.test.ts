import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { prNotificationsStore } from "../../stores/prNotifications";
import type { PrNotificationType } from "../../stores/prNotifications";
import { testInScope } from "../helpers/store";

function makeNotification(overrides: Partial<{
  repoPath: string;
  branch: string;
  prNumber: number;
  title: string;
  type: PrNotificationType;
}> = {}) {
  return {
    repoPath: "/repo/path",
    branch: "feature/test",
    prNumber: 42,
    title: "Test PR",
    type: "ready" as PrNotificationType,
    ...overrides,
  };
}

describe("prNotificationsStore", () => {
  beforeEach(() => {
    prNotificationsStore.clearAll();
    prNotificationsStore.stopFocusTimer();
  });

  afterEach(() => {
    prNotificationsStore.stopFocusTimer();
  });

  describe("add()", () => {
    it("adds a notification with id, dismissed=false, focusedTimeMs=0", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification());
        const active = prNotificationsStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].id).toBe("/repo/path:42:ready");
        expect(active[0].dismissed).toBe(false);
        expect(active[0].focusedTimeMs).toBe(0);
        expect(active[0].createdAt).toBeGreaterThan(0);
      });
    });

    it("ignores duplicate active notification (same repo+pr+type)", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification());
        prNotificationsStore.add(makeNotification({ title: "Updated title" }));
        expect(prNotificationsStore.getActive()).toHaveLength(1);
      });
    });

    it("replaces a dismissed notification with same id", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification());
        prNotificationsStore.dismiss("/repo/path:42:ready");
        expect(prNotificationsStore.getActive()).toHaveLength(0);

        prNotificationsStore.add(makeNotification({ title: "Re-triggered" }));
        const active = prNotificationsStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].dismissed).toBe(false);
        expect(active[0].title).toBe("Re-triggered");
      });
    });

    it("allows different notification types for same PR", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification({ type: "ready" }));
        prNotificationsStore.add(makeNotification({ type: "ci_failed" }));
        expect(prNotificationsStore.getActive()).toHaveLength(2);
      });
    });

    it("allows same type for different PRs", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification({ prNumber: 1 }));
        prNotificationsStore.add(makeNotification({ prNumber: 2 }));
        expect(prNotificationsStore.getActive()).toHaveLength(2);
      });
    });
  });

  describe("dismiss()", () => {
    it("marks a single notification as dismissed", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification({ prNumber: 1 }));
        prNotificationsStore.add(makeNotification({ prNumber: 2 }));

        prNotificationsStore.dismiss("/repo/path:1:ready");
        const active = prNotificationsStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].id).toBe("/repo/path:2:ready");
      });
    });

    it("does not affect other notifications", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification({ prNumber: 1 }));
        prNotificationsStore.add(makeNotification({ prNumber: 2 }));
        prNotificationsStore.add(makeNotification({ prNumber: 3 }));

        prNotificationsStore.dismiss("/repo/path:2:ready");
        const active = prNotificationsStore.getActive();
        expect(active).toHaveLength(2);
        expect(active.map((n) => n.id)).not.toContain("/repo/path:2:ready");
      });
    });

    it("is a no-op for unknown id", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification());
        prNotificationsStore.dismiss("unknown:id");
        expect(prNotificationsStore.getActive()).toHaveLength(1);
      });
    });
  });

  describe("dismissAll()", () => {
    it("marks all active notifications as dismissed", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification({ prNumber: 1 }));
        prNotificationsStore.add(makeNotification({ prNumber: 2 }));
        prNotificationsStore.add(makeNotification({ prNumber: 3 }));

        prNotificationsStore.dismissAll();
        expect(prNotificationsStore.getActive()).toHaveLength(0);
      });
    });

    it("is a no-op when no active notifications", () => {
      testInScope(() => {
        prNotificationsStore.dismissAll();
        expect(prNotificationsStore.getActive()).toHaveLength(0);
      });
    });

    it("does not remove already-dismissed notifications from internal list", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification({ prNumber: 1 }));
        prNotificationsStore.dismiss("/repo/path:1:ready");
        prNotificationsStore.add(makeNotification({ prNumber: 2 }));

        prNotificationsStore.dismissAll();
        expect(prNotificationsStore.getActive()).toHaveLength(0);
        // Total state still has 2 entries (both dismissed)
        expect(prNotificationsStore.state.notifications).toHaveLength(2);
      });
    });
  });

  describe("getActive()", () => {
    it("returns only non-dismissed notifications", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification({ prNumber: 1 }));
        prNotificationsStore.add(makeNotification({ prNumber: 2 }));
        prNotificationsStore.dismiss("/repo/path:1:ready");

        const active = prNotificationsStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].id).toBe("/repo/path:2:ready");
      });
    });

    it("returns empty array when all are dismissed", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification());
        prNotificationsStore.dismissAll();
        expect(prNotificationsStore.getActive()).toHaveLength(0);
      });
    });

    it("returns empty array when store is empty", () => {
      testInScope(() => {
        expect(prNotificationsStore.getActive()).toHaveLength(0);
      });
    });
  });

  describe("startFocusTimer()", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      prNotificationsStore.stopFocusTimer();
      vi.useRealTimers();
    });

    it("increments focusedTimeMs each second when document is focused", () => {
      testInScope(() => {
        vi.spyOn(document, "hasFocus").mockReturnValue(true);
        prNotificationsStore.add(makeNotification());
        prNotificationsStore.startFocusTimer();

        vi.advanceTimersByTime(3000);
        const active = prNotificationsStore.getActive();
        expect(active[0].focusedTimeMs).toBe(3000);
      });
    });

    it("does not increment when document is not focused", () => {
      testInScope(() => {
        vi.spyOn(document, "hasFocus").mockReturnValue(false);
        prNotificationsStore.add(makeNotification());
        prNotificationsStore.startFocusTimer();

        vi.advanceTimersByTime(5000);
        const active = prNotificationsStore.getActive();
        expect(active[0].focusedTimeMs).toBe(0);
      });
    });

    it("auto-dismisses notifications after 5 minutes of focus time", () => {
      testInScope(() => {
        vi.spyOn(document, "hasFocus").mockReturnValue(true);
        prNotificationsStore.add(makeNotification());
        prNotificationsStore.startFocusTimer();

        // Advance 5 minutes (300 ticks)
        vi.advanceTimersByTime(5 * 60 * 1000);
        expect(prNotificationsStore.getActive()).toHaveLength(0);
      });
    });

    it("does not start a second timer if already running", () => {
      testInScope(() => {
        vi.spyOn(document, "hasFocus").mockReturnValue(true);
        prNotificationsStore.add(makeNotification());
        prNotificationsStore.startFocusTimer();
        prNotificationsStore.startFocusTimer(); // Should be a no-op

        vi.advanceTimersByTime(1000);
        // If two timers ran, focusedTimeMs would be 2000; one timer gives 1000
        expect(prNotificationsStore.getActive()[0].focusedTimeMs).toBe(1000);
      });
    });

    it("only auto-dismisses notifications that reached the threshold", () => {
      testInScope(() => {
        vi.spyOn(document, "hasFocus").mockReturnValue(true);
        prNotificationsStore.add(makeNotification({ prNumber: 1 }));
        prNotificationsStore.startFocusTimer();

        // Advance 4 minutes — not yet dismissed
        vi.advanceTimersByTime(4 * 60 * 1000);
        expect(prNotificationsStore.getActive()).toHaveLength(1);

        // Add second notification with 0 focusedTimeMs
        prNotificationsStore.add(makeNotification({ prNumber: 2 }));

        // Advance 1 more minute — first reaches 5 min threshold, second has only 1 min
        vi.advanceTimersByTime(60 * 1000);
        const active = prNotificationsStore.getActive();
        expect(active).toHaveLength(1);
        expect(active[0].id).toBe("/repo/path:2:ready");
      });
    });
  });

  describe("clearAll()", () => {
    it("removes all notifications including dismissed ones", () => {
      testInScope(() => {
        prNotificationsStore.add(makeNotification({ prNumber: 1 }));
        prNotificationsStore.add(makeNotification({ prNumber: 2 }));
        prNotificationsStore.dismiss("/repo/path:1:ready");

        prNotificationsStore.clearAll();
        expect(prNotificationsStore.state.notifications).toHaveLength(0);
        expect(prNotificationsStore.getActive()).toHaveLength(0);
      });
    });
  });
});
