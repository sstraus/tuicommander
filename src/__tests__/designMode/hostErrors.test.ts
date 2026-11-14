import { expect, it, vi } from "vitest";
// @ts-expect-error plain MV3 module
import { syncHostErrors } from "../../../browser-extension/design-mode/hostErrors.js";

it("registers error worlds for consented remote hosts and removes them when consent is gone", async () => {
	// Catches: host consent enabling eval but never capturing errors on remote pages, or a Chrome-only
	// origin record (capture grant, leftover host grant) injecting error scripts without TUIC consent.
	let consents = [JSON.stringify(["A", "localhost"]), JSON.stringify(["A", "example.com"])];
	const chrome = {
		permissions: {
			getAll: vi.fn(async () => ({ origins: ["http://localhost/*", "<all_urls>", "https://other.example/*"] })),
		},
		storage: { session: { get: vi.fn(async () => ({ hostConsents: consents })) } },
		scripting: {
			getRegisteredContentScripts: vi.fn(async () => [] as { id: string }[]),
			unregisterContentScripts: vi.fn(async () => {}),
			registerContentScripts: vi.fn(async () => {}),
		},
	};
	await syncHostErrors(chrome);
	expect(chrome.scripting.registerContentScripts).toHaveBeenCalledWith([
		expect.objectContaining({
			matches: ["*://example.com/*"],
			js: ["errors.js"],
			world: "MAIN",
			runAt: "document_start",
		}),
		expect.objectContaining({ matches: ["*://example.com/*"], js: ["errorsBridge.js"], world: "ISOLATED" }),
	]);
	consents = [];
	chrome.scripting.getRegisteredContentScripts.mockResolvedValue([
		{ id: "tuic-granted-errors-main" },
		{ id: "tuic-granted-errors-bridge" },
	]);
	await syncHostErrors(chrome);
	expect(chrome.scripting.unregisterContentScripts).toHaveBeenCalledWith({
		ids: ["tuic-granted-errors-main", "tuic-granted-errors-bridge"],
	});
	expect(chrome.scripting.registerContentScripts).toHaveBeenCalledTimes(1);
});
