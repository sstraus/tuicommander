import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
// @ts-expect-error plain MV3 module
import { browserCommand } from "../../../browser-extension/design-mode/browserCommands.js";

function browser(origins: string[] = [], consents: string[] = []) {
	const session: Record<string, unknown> = { hostConsents: consents };
	return {
		tabs: {
			get: vi.fn(async () => ({ id: 7, active: true, windowId: 2, url: "http://localhost:5173/" })),
			update: vi.fn(async () => {}),
			reload: vi.fn(async () => {}),
			captureVisibleTab: vi.fn(async () => "data:image/png;base64,iVBORw0KGgo="),
		},
		permissions: { contains: vi.fn(async () => origins.length > 0), getAll: vi.fn(async () => ({ origins })) },
		scripting: {
			executeScript: vi.fn(
				async (_injection: { func: (...a: unknown[]) => unknown; args: unknown[] }): Promise<{ result: unknown }[]> => [
					{ result: { value: 4 } },
				],
			),
		},
		storage: {
			session: {
				get: vi.fn(async (key: string) => ({ [key]: session[key] })),
				set: vi.fn(async (value: Record<string, unknown>) => Object.assign(session, value)),
			},
		},
	};
}
// One owner consent record, as the sidebar writes it for the armed session.
const consent = (sessionId: string, hostname: string) => JSON.stringify([sessionId, hostname]);
function stubCanvas(sizes: (width: number, height: number) => number) {
	const dimensions: number[][] = [];
	vi.stubGlobal("fetch", async () => ({ blob: async () => new Blob() }));
	vi.stubGlobal("createImageBitmap", async () => ({ width: 3840, height: 2160, close: vi.fn() }));
	vi.stubGlobal(
		"OffscreenCanvas",
		class {
			constructor(
				public width: number,
				public height: number,
			) {
				dimensions.push([width, height]);
			}
			getContext() {
				return { drawImage: () => {} };
			}
			convertToBlob = async ({ type }: { type: string }) =>
				new Blob([new Uint8Array(sizes(this.width, this.height))], { type });
		},
	);
	return dimensions;
}
describe("native browser command boundary", () => {
	afterEach(() => vi.unstubAllGlobals());
	// Catches: UTF-16 length used for a UTF-8 byte bound, so a large CJK result times out instead of result_too_large.
	it("eval result_too_large is measured in UTF-8 bytes", async () => {
		const chrome = browser([], [consent("A", "localhost")]);
		chrome.scripting.executeScript.mockImplementation(async (injection) => {
			vi.stubGlobal("location", { hostname: "localhost" });
			return [{ result: await injection.func(...injection.args) }];
		});
		// 300K CJK characters: 300K UTF-16 units (under 640 Ki) but 900 KB of UTF-8 (over the 768 KiB reply bound).
		await expect(
			browserCommand({ sessionId: "A", action: "eval", args: { script: '"\u4e2d".repeat(300000)' } }, chrome, 7),
		).rejects.toThrow(/^result_too_large:/);
	});
	// Catches: default PNG, ignored quality/downsize, upscaling, or re-encoding PNG lossily.
	it("encodes each screenshot with the requested format, quality and pixel bounds", async () => {
		const draw = vi.fn();
		const encode = vi.fn(async ({ type }: { type: string }) => new Blob([new Uint8Array([1, 2, 3])], { type }));
		const dimensions: number[][] = [];
		vi.stubGlobal("fetch", async () => ({ blob: async () => new Blob() }));
		vi.stubGlobal("createImageBitmap", async () => ({ width: 1600, height: 900, close: () => {} }));
		vi.stubGlobal(
			"OffscreenCanvas",
			class {
				constructor(
					public width: number,
					public height: number,
				) {
					dimensions.push([width, height]);
				}
				getContext() {
					return { drawImage: draw };
				}
				convertToBlob = encode;
			},
		);
		const defaults = await browserCommand({ action: "screenshot" }, browser(), 7);
		expect(defaults).toMatchObject({
			format: "webp",
			quality: 25,
			bytes: 3,
			width: 1600,
			height: 900,
			image: "data:image/webp;base64,AQID",
		});
		expect(encode).toHaveBeenLastCalledWith({ type: "image/webp", quality: 0.25 });
		const smaller = await browserCommand(
			{ action: "screenshot", args: { quality: 55, max_long_edge: 800 } },
			browser(),
			7,
		);
		expect(smaller).toMatchObject({ quality: 55, width: 800, height: 450 });
		expect(encode).toHaveBeenLastCalledWith({ type: "image/webp", quality: 0.55 });
		expect(draw.mock.calls[1].slice(1)).toEqual([0, 0, 1600, 900, 0, 0, 800, 450]);
		const png = await browserCommand(
			{ action: "screenshot", args: { format: "png", max_long_edge: 2000 } },
			browser(),
			7,
		);
		expect(png).toMatchObject({ format: "png", quality: null, image: "data:image/png;base64,AQID" });
		expect(encode).toHaveBeenLastCalledWith({ type: "image/png" });
		expect(dimensions).toEqual([
			[1600, 900],
			[800, 450],
			[1600, 900],
		]);
		for (const args of [
			{ format: "jpeg" },
			{ quality: 0 },
			{ quality: 101 },
			{ quality: 1.5 },
			{ max_long_edge: 0 },
			{ max_long_edge: 1.5 },
		]) {
			const chrome = browser();
			await expect(browserCommand({ action: "screenshot", args }, chrome, 7)).rejects.toThrow(/invalid_/);
			expect(chrome.tabs.captureVisibleTab).not.toHaveBeenCalled();
		}
	});
	it("refuses remote navigation without consent and accepts a consented host", async () => {
		// Catches: remote navigation proceeding without the owner's host consent.
		const chrome = browser();
		await expect(
			browserCommand({ action: "navigate", args: { url: "https://example.com/" } }, chrome, 7),
		).rejects.toThrow("host_denied");
		expect(chrome.tabs.update).not.toHaveBeenCalled();
		await chrome.storage.session.set({ hostConsents: [consent("A", "example.com")] });
		await browserCommand({ sessionId: "A", action: "navigate", args: { url: "https://example.com/" } }, chrome, 7);
		expect(chrome.tabs.update).toHaveBeenCalledWith(7, { url: "https://example.com/" });
		await expect(
			browserCommand({ action: "navigate", args: { url: "javascript:alert(1)" } }, chrome, 7),
		).rejects.toThrow("host_denied");
	});
	it("design_host_session_consent_avoids_regrant_and_downscales", async () => {
		// Catches: owner repeats capture/eval consent or loses valid large screenshots.
		const consents = [consent("A", "localhost"), consent("A", "example.com")];
		const chrome = browser(["<all_urls>"], consents);
		// One recorded consent with Chrome's capture-only grant enables eval on localhost: no second host grant.
		expect(await browserCommand({ sessionId: "A", action: "eval", args: { script: "2+2" } }, chrome, 7)).toEqual({
			value: 4,
		});
		expect(chrome.scripting.executeScript).toHaveBeenCalledWith(
			expect.objectContaining({ world: "MAIN", target: { tabId: 7 } }),
		);
		// The same consent covers navigation, eval and errors on a remote host without a host-only Chrome origin.
		await browserCommand({ sessionId: "A", action: "navigate", args: { url: "https://example.com/a" } }, chrome, 7);
		chrome.tabs.get.mockResolvedValue({ id: 7, active: true, windowId: 2, url: "https://example.com/a" });
		expect(await browserCommand({ sessionId: "A", action: "eval", args: { script: "2+2" } }, chrome, 7)).toEqual({
			value: 4,
		});
		const errors = vi.fn(async () => [{ message: "remote" }]);
		expect(await browserCommand({ sessionId: "A", action: "errors" }, chrome, 7, errors)).toEqual({
			errors: [{ message: "remote" }],
		});
		// Scripts larger than the old 16 KiB extension bound reach the page; the host owns the command bound.
		const script = `"${"x".repeat(40 * 1024)}".length`;
		await browserCommand({ sessionId: "A", action: "eval", args: { script } }, chrome, 7);
		expect(chrome.scripting.executeScript).toHaveBeenLastCalledWith(
			expect.objectContaining({ args: [script, "example.com", 640 * 1024] }),
		);
		// A 4K viewport whose first encoding is far above the transport bound is downscaled, not refused.
		const dimensions = stubCanvas((width, height) => Math.round(width * height * 0.6));
		const shot = await browserCommand({ sessionId: "A", action: "screenshot", args: { format: "png" } }, chrome, 7);
		expect(dimensions[0]).toEqual([3840, 2160]);
		expect(dimensions.length).toBeGreaterThan(1);
		expect(shot.downscaled).toBe(true);
		expect(shot.bytes).toBeLessThanOrEqual(512 * 1024);
		expect(shot.bytes).toBe(Math.round(shot.width * shot.height * 0.6));
		expect(shot.width / shot.height).toBeCloseTo(16 / 9, 1);
		expect(shot.url).toBe("https://example.com/a");
		expect(chrome.tabs.captureVisibleTab).toHaveBeenCalledTimes(1);
		// A capture that already fits keeps its pixels and says so.
		stubCanvas(() => 1000);
		const small = await browserCommand({ sessionId: "A", action: "screenshot" }, chrome, 7);
		expect(small).toMatchObject({ width: 3840, height: 2160, bytes: 1000, downscaled: false });
	});
	it("design_unreviewed_tab_and_unconsented_host_stay_blocked", async () => {
		// Catches: untrusted page or injected request inspects another tab or expands host/session scope.
		const remote = { id: 7, active: true, windowId: 2, url: "https://example.com/" };
		// Wildcard and capture-only Chrome grants are not TUIC consent.
		for (const origins of [["<all_urls>"], ["http://*/*", "https://*/*"], ["*://example.com/*"]]) {
			const chrome = browser(origins);
			await expect(
				browserCommand({ sessionId: "A", action: "eval", args: { script: "1" } }, chrome, 7),
			).rejects.toThrow(/^eval_not_granted:.*Allow agents on this host/);
			chrome.tabs.get.mockResolvedValue(remote);
			for (const action of ["screenshot", "errors", "reload"]) {
				await expect(browserCommand({ sessionId: "A", action }, chrome, 7, async () => [])).rejects.toThrow(
					/^host_denied:.*Allow agents on this host/,
				);
			}
			await expect(
				browserCommand({ sessionId: "A", action: "navigate", args: { url: "https://other.example/" } }, chrome, 7),
			).rejects.toThrow("host_denied");
			expect(chrome.scripting.executeScript).not.toHaveBeenCalled();
			expect(chrome.tabs.captureVisibleTab).not.toHaveBeenCalled();
			expect(chrome.tabs.reload).not.toHaveBeenCalled();
			expect(chrome.tabs.update).not.toHaveBeenCalled();
		}
		// Consent belongs to one session and one host.
		const chrome = browser(["<all_urls>"], [consent("A", "example.com")]);
		chrome.tabs.get.mockResolvedValue(remote);
		await expect(browserCommand({ sessionId: "B", action: "eval", args: { script: "1" } }, chrome, 7)).rejects.toThrow(
			"eval_not_granted",
		);
		await expect(
			browserCommand({ sessionId: "A", action: "navigate", args: { url: "https://evil.example.com/" } }, chrome, 7),
		).rejects.toThrow("host_denied");
		await expect(browserCommand({ action: "eval", args: { script: "1" } }, chrome, 7)).rejects.toThrow(
			"eval_not_granted",
		);
		// A command cannot pick a different tab; only the reviewed tab is touched.
		await browserCommand({ sessionId: "A", action: "eval", args: { script: "1", tabId: 99 } }, chrome, 7);
		expect(chrome.tabs.get).toHaveBeenCalledWith(7);
		expect(chrome.tabs.get).not.toHaveBeenCalledWith(99);
		expect(chrome.scripting.executeScript).toHaveBeenCalledWith(expect.objectContaining({ target: { tabId: 7 } }));
		await expect(
			browserCommand({ sessionId: "A", action: "eval", args: { script: "1" } }, chrome, undefined),
		).rejects.toThrow("no_reviewed_tab");
		// The page passes the consented host to the injected function, which refuses a document that moved away.
		chrome.scripting.executeScript.mockImplementation(async ({ func, args }) => {
			vi.stubGlobal("location", { hostname: "other.example" });
			return [{ result: await func(...args) }];
		});
		await expect(browserCommand({ sessionId: "A", action: "eval", args: { script: "1" } }, chrome, 7)).rejects.toThrow(
			"page_changed",
		);
		// The reviewed page changing host during a capture discards the image.
		stubCanvas(() => 1000);
		chrome.tabs.get.mockResolvedValueOnce(remote).mockResolvedValueOnce({ ...remote, url: "https://other.example/" });
		await expect(browserCommand({ sessionId: "A", action: "screenshot" }, chrome, 7)).rejects.toThrow("page_changed");
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
	it("translates recorded Chrome capture denial into actionable option guidance", async () => {
		// Catches: raw Chrome permission denial escaping without a typed recovery.
		const recorded = JSON.parse(readFileSync("scripts/fixtures/design-mode-chrome149-capture-denial.json", "utf8"));
		const chrome = browser();
		chrome.tabs.captureVisibleTab.mockRejectedValue(new Error(JSON.parse(recorded.result.content[0].text).error));
		await expect(browserCommand({ action: "screenshot" }, chrome, 7)).rejects.toThrow(
			/^capture_permission_required:.*Allow agents on this host/,
		);
		chrome.tabs.captureVisibleTab.mockRejectedValue(new Error("capture backend failed"));
		await expect(browserCommand({ action: "screenshot" }, chrome, 7)).rejects.toThrow("capture backend failed");
	});
});
