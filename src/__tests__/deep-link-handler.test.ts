import { beforeEach, describe, expect, it, vi } from "vitest";

import { mockInvoke } from "./mocks/tauri";

// isTauri() checks __TAURI_INTERNALS__ — set to ensure `initDeepLinkHandler`
// (which bails in a non-Tauri context) would run. `handleDeepLink` is
// exported separately and doesn't need this, but the module-level guard on
// `transport.isTauri()` uses it elsewhere.
(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};

import { handleDeepLink } from "../deep-link-handler";

const callbacks = {
	openSettings: vi.fn(),
	confirm: vi.fn().mockResolvedValue(true),
	onInstallError: vi.fn(),
};

describe("deep link handler — OAuth callback", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		callbacks.openSettings.mockReset();
		callbacks.confirm.mockReset().mockResolvedValue(true);
		callbacks.onInstallError.mockReset();
	});

	it("invokes mcp_oauth_callback with code + state from tuic://oauth-callback", async () => {
		await handleDeepLink("tuic://oauth-callback?code=AUTH_CODE&state=NONCE_123", callbacks);

		expect(mockInvoke).toHaveBeenCalledTimes(1);
		expect(mockInvoke).toHaveBeenCalledWith("mcp_oauth_callback", {
			code: "AUTH_CODE",
			oauthState: "NONCE_123",
		});
		expect(callbacks.onInstallError).not.toHaveBeenCalled();
	});

	it("surfaces invoke failures via onInstallError", async () => {
		mockInvoke.mockRejectedValueOnce("token exchange failed");

		await handleDeepLink("tuic://oauth-callback?code=AUTH_CODE&state=NONCE", callbacks);

		expect(callbacks.onInstallError).toHaveBeenCalledWith(expect.stringContaining("token exchange failed"));
	});

	it("skips invoke when code is missing", async () => {
		await handleDeepLink("tuic://oauth-callback?state=NONCE", callbacks);
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it("skips invoke when state is missing", async () => {
		await handleDeepLink("tuic://oauth-callback?code=XYZ", callbacks);
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it("handles authorization server error responses", async () => {
		await handleDeepLink("tuic://oauth-callback?error=access_denied&error_description=user%20cancelled", callbacks);

		expect(mockInvoke).not.toHaveBeenCalled();
		expect(callbacks.onInstallError).toHaveBeenCalledWith(expect.stringContaining("access_denied"));
	});
});

describe("deep link handler — cmd gateway (default-deny)", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		callbacks.confirm.mockReset().mockResolvedValue(true);
		callbacks.onInstallError.mockReset();
	});

	it("runs a safe read-only command without confirmation", async () => {
		await handleDeepLink("tuic://cmd/session/list", callbacks);
		expect(callbacks.confirm).not.toHaveBeenCalled();
		expect(mockInvoke).toHaveBeenCalledWith("deep_link_mcp_call", {
			tool: "session",
			action: "list",
			params: {},
		});
	});

	it("requires confirmation for a destructive command (session/input)", async () => {
		await handleDeepLink("tuic://cmd/session/input?data=rm", callbacks);
		expect(callbacks.confirm).toHaveBeenCalledTimes(1);
		expect(mockInvoke).toHaveBeenCalledWith("deep_link_mcp_call", {
			tool: "session",
			action: "input",
			params: { data: "rm" },
		});
	});

	it("requires confirmation for an unknown command — default-deny (agent/send was previously un-gated)", async () => {
		await handleDeepLink("tuic://cmd/agent/send?to=peer", callbacks);
		expect(callbacks.confirm).toHaveBeenCalledTimes(1);
	});

	it("does not execute when confirmation is denied", async () => {
		callbacks.confirm.mockResolvedValue(false);
		await handleDeepLink("tuic://cmd/agent/send?to=peer", callbacks);
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it("never executes a blocked command, even with confirmation available", async () => {
		await handleDeepLink("tuic://cmd/config/save", callbacks);
		expect(callbacks.confirm).not.toHaveBeenCalled();
		expect(mockInvoke).not.toHaveBeenCalled();
	});
});
