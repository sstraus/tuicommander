import { describe, it, expect } from "vitest";
import { shouldShowAuthorize } from "../../../components/SettingsPanel/tabs/ServicesTab";

describe("shouldShowAuthorize", () => {
  it("shows button when auth type is oauth2 (explicit config)", () => {
    expect(shouldShowAuthorize("oauth2", "ready")).toBe(true);
  });

  it("shows button when status is needs_auth regardless of auth config", () => {
    expect(shouldShowAuthorize(undefined, "needs_auth")).toBe(true);
  });

  it("shows button when needs_auth with no auth (DCR case)", () => {
    expect(shouldShowAuthorize(undefined, "needs_auth")).toBe(true);
  });

  it("shows button when authenticating (flow in progress)", () => {
    expect(shouldShowAuthorize(undefined, "authenticating")).toBe(true);
  });

  it("shows button when oauth2 and authenticating", () => {
    expect(shouldShowAuthorize("oauth2", "authenticating")).toBe(true);
  });

  it("hides button when bearer auth and not needs_auth", () => {
    expect(shouldShowAuthorize("bearer", "ready")).toBe(false);
  });

  it("hides button when no auth config and status is connected", () => {
    expect(shouldShowAuthorize(undefined, "ready")).toBe(false);
  });

  it("hides button when no auth config and status is connecting", () => {
    expect(shouldShowAuthorize(undefined, "connecting")).toBe(false);
  });

  it("hides button when no auth config and status is undefined", () => {
    expect(shouldShowAuthorize(undefined, undefined)).toBe(false);
  });
});
