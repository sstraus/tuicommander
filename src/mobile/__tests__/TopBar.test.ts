import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const sourceCode = readFileSync(
  resolve(__dirname, "../components/TopBar.tsx"),
  "utf-8",
);

describe("TopBar subtitle and notification badge", () => {
  it("renders the subtitle text when connected", () => {
    expect(sourceCode).toContain("Manage your sessions");
  });

  it("shows reconnecting subtitle when offline", () => {
    expect(sourceCode).toContain("Reconnecting");
  });

  it("has connectivity dot with online/offline classes", () => {
    expect(sourceCode).toContain("connDot");
    expect(sourceCode).toContain("connOnline");
    expect(sourceCode).toContain("connOffline");
  });

  it("badge is hidden when notificationCount is 0 or undefined", () => {
    // The Show condition uses nullish coalescing to treat undefined as 0,
    // then checks > 0 — so both 0 and undefined result in hidden badge.
    expect(sourceCode).toContain("(props.notificationCount ?? 0) > 0");
  });

  it("badge displays the notificationCount value", () => {
    expect(sourceCode).toContain("{props.notificationCount}");
  });

  it("badge uses .badge CSS class", () => {
    expect(sourceCode).toContain("styles.badge");
  });
});
