import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";

const source = readFileSync(resolve(__dirname, "../../../public/sw.js"), "utf8");
type Listener = (event: unknown) => void;

function worker(cached = true, response?: Response) {
	const listeners: Record<string, Listener> = {};
	const stored = new Map<string, Response>();
	if (cached) stored.set("/mobile.html", new Response("CACHED APP SHELL"));
	const cache = {
		put: async (key: Request | string, value: Response) => stored.set(typeof key === "string" ? key : key.url, value),
	};
	const self = {
		addEventListener: (name: string, fn: Listener) => {
			listeners[name] = fn;
		},
		skipWaiting: vi.fn(),
	};
	const fetch = vi.fn(async () => {
		if (response) return response;
		throw new TypeError("Failed to fetch");
	});
	new Function("self", "caches", "fetch", "clients", "crypto", "Request", "Response", source)(
		self,
		{ open: async () => cache, match: async (key: string) => stored.get(key) },
		fetch,
		{},
		{},
		class {
			url: string;
			constructor(url: string) {
				this.url = url;
			}
		},
		Response,
	);
	return { listeners, stored };
}

async function navigate(listeners: Record<string, Listener>) {
	let pending!: Promise<Response>;
	listeners.fetch({
		request: { method: "GET", mode: "navigate", url: "https://tuic.test/mobile/session/abc" },
		respondWith: (response: Promise<Response>) => {
			pending = response;
		},
	});
	return (await pending).text();
}

describe("offline connection landing", () => {
	// Catches: successful navigation keeps writing a shell that offline recovery never reads.
	it("does not cache the mobile shell after successful navigation", async () => {
		const { listeners, stored } = worker(false, new Response("APP SHELL"));
		expect(await navigate(listeners)).toBe("APP SHELL");
		expect(stored.has("/mobile.html")).toBe(false);
	});
	// Catches: an installed cached shell wins over the self-contained landing and boots into a blank SPA.
	it("serves the connection landing instead of a cached shell on failed navigation", async () => {
		const { listeners } = worker();
		expect(await navigate(listeners)).toContain("Server Unreachable");
	});
	// Catches: no network or shell cache leaves a failed navigation without any useful document.
	it("serves the landing even when the offline cache is missing", async () => {
		const { listeners } = worker(false);
		expect(await navigate(listeners)).toContain("Server Unreachable");
	});
	// Catches: install depends on another network request instead of retaining the complete landing.
	it("pre-caches a self-contained connection test on install", async () => {
		const { listeners, stored } = worker(false);
		let pending!: Promise<unknown>;
		listeners.install({
			waitUntil: (value: Promise<unknown>) => {
				pending = value;
			},
		});
		await pending;
		expect(await stored.get("/_offline")!.text()).toContain('fetch("/api/version"');
	});
	// Catches: an upstream outage returns a useless 503 page instead of the retry surface.
	it("uses the connection landing on a server error", async () => {
		const { listeners } = worker(true, new Response("upstream unavailable", { status: 503 }));
		expect(await navigate(listeners)).toContain("Server Unreachable");
	});
});
