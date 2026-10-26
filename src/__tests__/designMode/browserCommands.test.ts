import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
// @ts-expect-error plain MV3 module
import { browserCommand } from "../../../browser-extension/design-mode/browserCommands.js";

function browser(origins: string[] = []) {
	return {
		tabs: {
			get: vi.fn(async () => ({ id: 7, active: true, windowId: 2, url: "http://localhost:5173/" })),
			update: vi.fn(async () => {}),
			reload: vi.fn(async () => {}),
			captureVisibleTab: vi.fn(async () => "data:image/png;base64,iVBORw0KGgo="),
		},
		permissions: { contains: vi.fn(async () => origins.length > 0), getAll: vi.fn(async () => ({ origins })) },
		scripting: { executeScript: vi.fn(async () => [{ result: { value: 4 } }]) },
	};
}
describe("native browser command boundary", () => {
	it("refuses remote navigation without a grant and accepts a granted host", async () => {
		// Catches: optional host permissions ignored or silently bypassed.
		const chrome = browser();
		await expect(
			browserCommand({ action: "navigate", args: { url: "https://example.com/" } }, chrome, 7),
		).rejects.toThrow("host_denied");
		expect(chrome.tabs.update).not.toHaveBeenCalled();
		chrome.permissions.contains.mockResolvedValue(true);
		await browserCommand({ action: "navigate", args: { url: "https://example.com/" } }, chrome, 7);
		expect(chrome.tabs.update).toHaveBeenCalledWith(7, { url: "https://example.com/" });
		await expect(
			browserCommand({ action: "navigate", args: { url: "javascript:alert(1)" } }, chrome, 7),
		).rejects.toThrow("host_denied");
	});
	it("requires explicit eval consent even on static localhost access", async () => {
		// Catches: ordinary local inspection permission treated as eval consent.
		const chrome = browser(["http://localhost/*"]);
		await expect(browserCommand({ action: "eval", args: { script: "2+2" } }, chrome, 7)).rejects.toThrow(
			"eval_not_granted",
		);
		expect(chrome.scripting.executeScript).not.toHaveBeenCalled();
		await expect(
			browserCommand({ action: "eval", args: { script: "2+2" } }, browser(["<all_urls>"]), 7),
		).rejects.toThrow("eval_not_granted");
		const granted = browser(["http://*/*", "https://*/*"]);
		expect(await browserCommand({ action: "eval", args: { script: "2+2" } }, granted, 7)).toEqual({ value: 4 });
		expect(granted.scripting.executeScript).toHaveBeenCalledWith(
			expect.objectContaining({ world: "MAIN", target: { tabId: 7 } }),
		);
	});
	it("refuses hidden-tab screenshots rather than capturing a different page", async () => {
		// Catches: captureVisibleTab returning another tab as the reviewed screenshot.
		const chrome = browser();
		chrome.tabs.get.mockResolvedValue({ id: 7, active: false, windowId: 2, url: "http://localhost:5173/" });
		await expect(browserCommand({ action: "screenshot" }, chrome, 7)).rejects.toThrow("tab_not_visible");
		expect(chrome.tabs.captureVisibleTab).not.toHaveBeenCalled();
	});
	it("returns errors from the reviewed tab and refuses missing tab identity", async () => {
		// Catches: browser commands falling back to an unrelated active tab.
		const errors = vi.fn(async () => [{ message: "broken" }]);
		expect(await browserCommand({ action: "errors" }, browser(), 7, errors)).toEqual({
			errors: [{ message: "broken" }],
		});
		expect(errors).toHaveBeenCalledWith(7);
		await expect(browserCommand({ action: "reload" }, browser(), undefined)).rejects.toThrow("no_reviewed_tab");
	});
	it("does not report an empty error list for a remote page that only has the capture grant", async () => {
		// Catches: <all_urls> capture consent passing the host gate for `errors`, while remote error injection
		// is never registered for it, so the agent reads "no errors" from a page nothing observed.
		const chrome = browser(["<all_urls>"]);
		chrome.tabs.get.mockResolvedValue({ id: 7, active: true, windowId: 2, url: "https://example.com/" });
		const errors = vi.fn(async () => []);
		await expect(browserCommand({ action: "errors" }, chrome, 7, errors)).rejects.toThrow("host_denied");
		expect(errors).not.toHaveBeenCalled();
		chrome.permissions.getAll.mockResolvedValue({ origins: ["*://example.com/*"] });
		await expect(browserCommand({ action: "errors" }, chrome, 7, errors)).resolves.toEqual({ errors: [] });
	});
	it("translates recorded Chrome capture denial into actionable option guidance", async () => {
		// Catches: raw Chrome permission denial escaping without a typed recovery.
		const recorded = JSON.parse(readFileSync("scripts/fixtures/design-mode-chrome149-capture-denial.json", "utf8"));
		const chrome = browser();
		chrome.tabs.captureVisibleTab.mockRejectedValue(new Error(JSON.parse(recorded.result.content[0].text).error));
		await expect(browserCommand({ action: "screenshot" }, chrome, 7)).rejects.toThrow(
			/^capture_permission_required:.*Let agents capture any site/,
		);
		chrome.tabs.captureVisibleTab.mockRejectedValue(new Error("capture backend failed"));
		await expect(browserCommand({ action: "screenshot" }, chrome, 7)).rejects.toThrow("capture backend failed");
	});
});
