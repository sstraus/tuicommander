import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { expect, it } from "vitest";

it("accepts a shared Android file and opens the mobile chat with a recoverable draft", async () => {
	const manifest = JSON.parse(readFileSync("public/mobile-manifest.json", "utf8"));
	expect(manifest.share_target).toMatchObject({
		action: "/mobile/share",
		method: "POST",
		enctype: "multipart/form-data",
		params: { files: [{ name: "attachment" }] },
	});
	const handlers = new Map<string, (event: unknown) => void>();
	const stored = new Map<string, Response>();
	runInNewContext(readFileSync("public/sw.js", "utf8"), {
		self: { addEventListener: (type: string, handler: (event: unknown) => void) => handlers.set(type, handler) },
		caches: {
			open: async () => ({
				put: async (key: string, value: Response) => {
					stored.set(key, value);
				},
			}),
		},
		Request,
		Response,
		URL,
		crypto: { randomUUID: () => "shared-1" },
	});
	let response: Promise<Response> | undefined;
	handlers.get("fetch")?.({
		request: {
			url: "https://tuic.example/mobile/share",
			method: "POST",
			mode: "navigate",
			formData: async () => new Map([["attachment", new File(["report"], "report.pdf", { type: "application/pdf" })]]),
		},
		respondWith: (result: Promise<Response>) => {
			response = result;
		},
	});
	expect(response).toBeDefined();
	expect((await response)?.headers.get("location")).toBe("/mobile?shared=shared-1");
	expect(stored.get("/_shared/shared-1")?.headers.get("x-file-name")).toBe("report.pdf");
	expect(await stored.get("/_shared/shared-1")?.text()).toBe("report");
});

// Catches: keeping obsolete v1 data after the shell cache upgrade.
it("deletes the old v1 shell cache on activation while keeping the current shared files", async () => {
	const handlers = new Map<string, (event: unknown) => void>();
	const names = new Set(["tuic-shell-v1", "tuic-shell-v2"]);
	let claimed = false;
	runInNewContext(readFileSync("public/sw.js", "utf8"), {
		self: {
			addEventListener: (type: string, handler: (event: unknown) => void) => handlers.set(type, handler),
			clients: {
				claim: async () => {
					claimed = true;
				},
			},
		},
		caches: {
			keys: async () => [...names],
			delete: async (name: string) => names.delete(name),
		},
	});
	let pending: Promise<unknown> | undefined;
	handlers.get("activate")!({
		waitUntil: (result: Promise<unknown>) => {
			pending = result;
		},
	});
	await pending;
	expect([...names]).toEqual(["tuic-shell-v2"]);
	expect(claimed).toBe(true);
});
