import { describe, it, expect } from "vitest";
import type { TabId } from "../components/BottomTabs";

// Test the tab-preservation logic in isolation.
// The navigateToSession function should NOT change the active tab, so that
// pressing Back restores the user to the original tab (Activity, Sessions, etc.)
describe("navigateToSession tab preservation", () => {
  function makeNavigationState(initialTab: TabId) {
    let activeTab: TabId = initialTab;
    let selectedSessionId: string | null = null;

    // Old (buggy) implementation: always switches to "sessions"
    function navigateToSessionOld(id: string) {
      selectedSessionId = id;
      activeTab = "sessions"; // bug: loses original tab
    }

    // New (fixed) implementation: preserves the current tab
    function navigateToSession(id: string) {
      selectedSessionId = id;
      // do NOT change activeTab
    }

    function goBack() {
      selectedSessionId = null;
    }

    return {
      getTab: () => activeTab,
      getSession: () => selectedSessionId,
      navigateToSessionOld,
      navigateToSession,
      goBack,
    };
  }

  it("old implementation: navigating from Activity loses the tab", () => {
    const nav = makeNavigationState("activity");
    nav.navigateToSessionOld("session-1");
    nav.goBack();
    // Bug: back returns to 'sessions' not 'activity'
    expect(nav.getTab()).toBe("sessions");
  });

  it("new implementation: navigating from Activity preserves it after back", () => {
    const nav = makeNavigationState("activity");
    nav.navigateToSession("session-1");
    expect(nav.getSession()).toBe("session-1");
    nav.goBack();
    // Fix: back returns to 'activity'
    expect(nav.getTab()).toBe("activity");
  });

  it("new implementation: navigating from sessions preserves sessions after back", () => {
    const nav = makeNavigationState("sessions");
    nav.navigateToSession("session-2");
    nav.goBack();
    expect(nav.getTab()).toBe("sessions");
  });

  it("new implementation: navigating from settings preserves settings after back", () => {
    const nav = makeNavigationState("settings");
    nav.navigateToSession("session-3");
    nav.goBack();
    expect(nav.getTab()).toBe("settings");
  });
});
