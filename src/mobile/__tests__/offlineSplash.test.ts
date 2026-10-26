// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const source = readFileSync(resolve(__dirname, "../../../public/sw.js"), "utf8");
const html = new Function("self", `${source}\nreturn OFFLINE_SPLASH_HTML;`)({ addEventListener() {} }) as string;
let dispose: (() => void)[];
beforeEach(() => {
	vi.useFakeTimers();
	dispose = [];
});
afterEach(() => {
	dispose.forEach((fn) => fn());
	document.body.innerHTML = "";
	vi.clearAllTimers();
	vi.useRealTimers();
});

function splash(fetch: ReturnType<typeof vi.fn>) {
	const parsed = new DOMParser().parseFromString(html, "text/html");
	document.body.innerHTML = parsed.body.innerHTML;
	const replace = vi.fn();
	const href = "https://tuic.test/mobile/session/abc?view=chat";
	const pageWindow = {
		addEventListener: (name: string, listener: EventListener) => {
			window.addEventListener(name, listener);
			dispose.push(() => window.removeEventListener(name, listener));
		},
	};
	// The production inline script drives the visible DOM; only the owned HTTP response and navigation are substituted.
	const pageDocument = {
		getElementById: (id: string) => document.getElementById(id),
		get hidden() {
			return document.hidden;
		},
		addEventListener: (name: string, listener: EventListener) => {
			document.addEventListener(name, listener);
			dispose.push(() => document.removeEventListener(name, listener));
		},
	};
	new Function("location", "fetch", "document", "window", parsed.querySelector("script")!.textContent!)(
		{ origin: "https://tuic.test", href, replace },
		fetch,
		pageDocument,
		pageWindow,
	);
	return { replace, href };
}

describe("offline splash connection test", () => {
	// Catches: a healthy version endpoint reloads an unavailable mobile route forever.
	it("does not reload repeatedly when version is reachable but the app document still fails", async () => {
		const fetch = vi.fn(async (url: string) =>
			url === "/api/version" ? { ok: true, json: async () => ({ version: "test" }) } : { ok: false, status: 503 },
		);
		const { replace } = splash(fetch);
		await vi.advanceTimersByTimeAsync(5_000);
		expect(replace).not.toHaveBeenCalled();
		expect(document.getElementById("state")!.textContent).toBe("Server unreachable");
	});

	// Catches: a returned server needs a manual reload or loses the original session deep link.
	it("shows the address and error, then automatically opens the original destination", async () => {
		const fetch = vi
			.fn()
			.mockRejectedValueOnce(new TypeError("Failed to fetch"))
			.mockResolvedValue({ ok: true, json: async () => ({ version: "test" }) });
		const { replace, href } = splash(fetch);
		await vi.advanceTimersByTimeAsync(0);
		expect(document.getElementById("server")!.textContent).toBe("https://tuic.test");
		expect(document.getElementById("state")!.textContent).toBe("Server unreachable");
		expect(document.getElementById("error")!.textContent).toContain("Failed to fetch");
		await vi.advanceTimersByTimeAsync(5_000);
		expect(replace).toHaveBeenCalledWith(href);
	});
	// Catches: a captive portal's HTML 200 is mistaken for a healthy version endpoint and reloads forever.
	it("does not navigate on an invalid version response and permits immediate retry", async () => {
		const fetch = vi
			.fn()
			.mockResolvedValueOnce({ ok: true, json: async () => ({}) })
			.mockResolvedValue({ ok: true, json: async () => ({ version: "test" }) });
		const { replace } = splash(fetch);
		await vi.advanceTimersByTimeAsync(0);
		expect(replace).not.toHaveBeenCalled();
		expect(document.getElementById("error")!.textContent).toContain("Invalid server response");
		document.getElementById("retry")!.click();
		await vi.advanceTimersByTimeAsync(0);
		expect(replace).toHaveBeenCalledOnce();
	});
	// Catches: a blackholed connection leaves the splash Testing forever and disables all future probes.
	it("bounds a hanging probe and schedules the next test", async () => {
		const fetch = vi
			.fn()
			.mockImplementationOnce(
				(_url, options: RequestInit) =>
					new Promise((_resolve, reject) => {
						options.signal!.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")));
					}),
			)
			.mockResolvedValue({ ok: true, json: async () => ({ version: "test" }) });
		const { replace, href } = splash(fetch);
		await vi.advanceTimersByTimeAsync(5_000);
		expect(document.getElementById("state")!.textContent).toBe("Server unreachable");
		expect(document.getElementById("error")!.textContent).toContain("Connection timed out");
		await vi.advanceTimersByTimeAsync(5_000);
		expect(fetch).toHaveBeenCalledTimes(3);
		expect(replace).toHaveBeenCalledWith(href);
	});
});
