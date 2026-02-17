import { describe, it, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

import { isMainBranch } from "../../stores/repositories";

describe("isMainBranch()", () => {
  it("returns true for 'main'", () => {
    expect(isMainBranch("main")).toBe(true);
  });

  it("returns true for 'master'", () => {
    expect(isMainBranch("master")).toBe(true);
  });

  it("returns true for 'develop'", () => {
    expect(isMainBranch("develop")).toBe(true);
  });

  it("returns true for 'development'", () => {
    expect(isMainBranch("development")).toBe(true);
  });

  it("returns true for 'dev'", () => {
    expect(isMainBranch("dev")).toBe(true);
  });

  it("is case-insensitive", () => {
    expect(isMainBranch("Main")).toBe(true);
    expect(isMainBranch("MASTER")).toBe(true);
    expect(isMainBranch("Develop")).toBe(true);
    expect(isMainBranch("DEVELOPMENT")).toBe(true);
    expect(isMainBranch("DEV")).toBe(true);
  });

  it("returns false for feature branches", () => {
    expect(isMainBranch("feature/foo")).toBe(false);
    expect(isMainBranch("feature/main")).toBe(false);
    expect(isMainBranch("bugfix/master-fix")).toBe(false);
  });

  it("returns false for other branches", () => {
    expect(isMainBranch("staging")).toBe(false);
    expect(isMainBranch("release/1.0")).toBe(false);
    expect(isMainBranch("hotfix/urgent")).toBe(false);
    expect(isMainBranch("")).toBe(false);
  });
});
