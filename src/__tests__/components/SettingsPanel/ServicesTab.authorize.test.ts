import { describe, expect, it } from "vitest";
import { shouldShowAuthorize } from "../../../components/SettingsPanel/tabs/ServicesTab";

describe("shouldShowAuthorize", () => {
	it("shows button when auth type is oauth2 (explicit config)", () => {
		expect(shouldShowAuthorize("oauth2", "ready", true)).toBe(true);
	});

	it("shows button when status is needs_auth regardless of auth config", () => {
		expect(shouldShowAuthorize(undefined, "needs_auth", true)).toBe(true);
	});

	it("shows button when needs_auth with no auth (DCR case)", () => {
		expect(shouldShowAuthorize(undefined, "needs_auth", true)).toBe(true);
	});

	it("shows button when authenticating (flow in progress)", () => {
		expect(shouldShowAuthorize(undefined, "authenticating", true)).toBe(true);
	});

	it("shows button when oauth2 and authenticating", () => {
		expect(shouldShowAuthorize("oauth2", "authenticating", true)).toBe(true);
	});

	it("hides button when bearer auth and not needs_auth", () => {
		expect(shouldShowAuthorize("bearer", "ready", true)).toBe(false);
	});

	it("hides button when no auth config and status is connected", () => {
		expect(shouldShowAuthorize(undefined, "ready", true)).toBe(false);
	});

	it("hides button when no auth config and status is connecting", () => {
		expect(shouldShowAuthorize(undefined, "connecting", true)).toBe(false);
	});

	it("hides button when no auth config and status is undefined", () => {
		expect(shouldShowAuthorize(undefined, undefined, true)).toBe(false);
	});

	it("hides button for a disabled oauth2 upstream (cannot authorize what is off)", () => {
		expect(shouldShowAuthorize("oauth2", "ready", false)).toBe(false);
	});

	it("hides button for a disabled upstream even when status is needs_auth", () => {
		expect(shouldShowAuthorize(undefined, "needs_auth", false)).toBe(false);
	});

	it("hides button for a disabled upstream mid-authentication", () => {
		expect(shouldShowAuthorize("oauth2", "authenticating", false)).toBe(false);
	});
});
