import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer, loadConfigFromFile } from "vite";
import { testTmpRoot } from "../../../scripts/test-tmp-root.mjs";

const repo = fileURLToPath(new URL("../../../", import.meta.url));
// `.tmp` is ignored by a pure path predicate (`ignoreNonFrontendFile`), so those paths are asserted
// on the predicate and nothing is written under `.tmp`, `src/.tmp` or `public/.tmp` in the checkout.
// The `watch` case still needs real files inside the repo root (a positive control under `src/`
// and a tooling control under `tools/`) because the watcher is rooted at the checkout.
const cwdScratchRoot = testTmpRoot();
mkdirSync(cwdScratchRoot, { recursive: true });

async function otherCwd() {
	const originalCwd = process.cwd();
	const alternate = mkdtempSync(join(cwdScratchRoot, "vite-watch-cwd-"));
	try {
		mkdirSync(join(alternate, "src-tauri"));
		writeFileSync(join(alternate, "src-tauri", "tauri.conf.json"), '{"version":"wrong-checkout"}');
		process.chdir(alternate);
		const loaded = await loadConfigFromFile({ command: "serve", mode: "development" }, join(repo, "vite.config.ts"));
		assert.ok(loaded);
		const config = loaded.config;
		const ignored = config.server?.watch?.ignored;
		assert.equal(typeof ignored, "function");
		assert.equal(ignored(join(repo, "src", "App.tsx")), false);
		assert.equal(ignored(join(repo, ".tmp", "report.html")), true);
		assert.equal(ignored(join(repo, "src", ".tmp", "report.html")), true);
		assert.equal(ignored(join(repo, "public", ".tmp", "report.html")), true);
		assert.equal(ignored(join(repo, ".tmp", "vite-watch-x", "index.html")), true);
		const actualVersion = JSON.parse(readFileSync(join(repo, "src-tauri", "tauri.conf.json"), "utf-8")).version;
		assert.equal(config.define.__APP_VERSION__, JSON.stringify(actualVersion));
	} finally {
		process.chdir(originalCwd);
		rmSync(alternate, { recursive: true, force: true });
	}
}

async function watch() {
	let server;
	const watched = join(repo, "src", "vite-watch-control.html");
	const tooling = join(repo, "tools", "vite-watch-control.html");
	try {
		server = await createServer({
			configFile: join(repo, "vite.config.ts"),
			server: { port: 0, strictPort: false, ws: { port: 0 } },
			optimizeDeps: { noDiscovery: true },
		});
		// `.tmp` at every depth is ignored (pure predicate, no files written inside the checkout).
		const ignored = server.config.server.watch.ignored;
		assert.equal(typeof ignored, "function");
		for (const path of [
			join(repo, ".tmp", "vite-watch-x", "index.html"),
			join(repo, "src", ".tmp", "vite-watch-control.html"),
			join(repo, "public", ".tmp", "vite-watch-control.html"),
		]) assert.equal(ignored(path), true, path);
		const ready = new Promise((resolve) => server.watcher.once("ready", resolve));
		await server.listen();
		await ready;
		const events = [];
		const messages = [];
		server.watcher.on("all", (_event, path) => events.push(path));
		const hot = server.environments.client.hot;
		const send = hot.send.bind(hot);
		hot.send = (message) => { messages.push(message); return send(message); };
		writeFileSync(watched, "<html></html>");
		await new Promise((resolve) => setTimeout(resolve, 700));
		assert.ok(events.includes(watched));
		assert.ok(messages.some((message) => JSON.stringify(message).includes('"type":"full-reload"')));
		messages.length = 0;
		writeFileSync(tooling, "<html></html>");
		await new Promise((resolve) => setTimeout(resolve, 700));
		for (const path of [tooling]) assert.ok(!events.includes(path), path);
		assert.ok(!messages.some((message) => JSON.stringify(message).includes('"type":"full-reload"')));
	} finally {
		await server?.close();
		for (const path of [watched, tooling]) rmSync(path, { force: true });
	}
}

if (process.argv[2] === "other-cwd") await otherCwd();
else if (process.argv[2] === "watch") await watch();
else throw new Error(`Unknown probe: ${process.argv[2]}`);
