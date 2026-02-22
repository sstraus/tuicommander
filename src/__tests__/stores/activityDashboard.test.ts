import { describe, it, expect, beforeEach } from "vitest";
import "../mocks/tauri";
import { activityDashboardStore } from "../../stores/activityDashboard";

describe("activityDashboardStore", () => {
  beforeEach(() => {
    activityDashboardStore.close();
  });

  it("starts closed", () => {
    expect(activityDashboardStore.state.isOpen).toBe(false);
  });

  it("open() sets isOpen to true", () => {
    activityDashboardStore.open();
    expect(activityDashboardStore.state.isOpen).toBe(true);
  });

  it("close() sets isOpen to false", () => {
    activityDashboardStore.open();
    activityDashboardStore.close();
    expect(activityDashboardStore.state.isOpen).toBe(false);
  });

  it("toggle() opens when closed and closes when open", () => {
    activityDashboardStore.toggle();
    expect(activityDashboardStore.state.isOpen).toBe(true);
    activityDashboardStore.toggle();
    expect(activityDashboardStore.state.isOpen).toBe(false);
  });
});
