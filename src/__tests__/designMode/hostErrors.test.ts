import { expect, it, vi } from "vitest";
// @ts-expect-error plain MV3 module
import { syncHostErrors } from "../../../browser-extension/design-mode/hostErrors.js";

it("registers granted remote error worlds and removes them after revocation", async () => {
	// Catches: host grants enabling eval but never capturing errors on remote pages.
	const chrome = {
		permissions: {
			getAll: vi.fn(async () => ({ origins: ["http://localhost/*", "<all_urls>", "https://example.com/*"] })),
		},
		scripting: {
			getRegisteredContentScripts: vi.fn(async () => [] as { id: string }[]),
			unregisterContentScripts: vi.fn(async () => {}),
			registerContentScripts: vi.fn(async () => {}),
		},
	};
	await syncHostErrors(chrome);
	expect(chrome.scripting.registerContentScripts).toHaveBeenCalledWith([
		expect.objectContaining({
			matches: ["https://example.com/*"],
			js: ["errors.js"],
			world: "MAIN",
			runAt: "document_start",
		}),
		expect.objectContaining({ matches: ["https://example.com/*"], js: ["errorsBridge.js"], world: "ISOLATED" }),
	]);
	chrome.permissions.getAll.mockResolvedValue({ origins: ["http://localhost/*", "<all_urls>"] });
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
