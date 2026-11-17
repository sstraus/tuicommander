// Run on rb with its shared Playwright install; no application dependency is added.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { spawn } from "node:child_process";
import { once } from "node:events";

const require = createRequire(import.meta.url);
function playwrightDir() {
	const dir = process.env.PLAYWRIGHT_DIR;
	if (!dir) throw new Error("Set PLAYWRIGHT_MODULE (path to playwright-core) or PLAYWRIGHT_DIR (a directory with node_modules/playwright-core); there is no default location.");
	return dir;
}
const { chromium, webkit, devices } = require(process.env.PLAYWRIGHT_MODULE ?? `${playwrightDir()}/node_modules/playwright-core`);
const artifacts = resolve("reports/visual/setting-fields");
await mkdir(artifacts, { recursive: true });
const server = spawn(process.execPath, ["node_modules/vite/bin/vite.js", "--config", "src/components/SettingsPanel/fields/preview.config.ts", "--host", "127.0.0.1", "--port", "0"], { stdio: ["ignore", "pipe", "pipe"] });
let output = "";
const url = await new Promise((resolveUrl, reject) => {
	const timer = setTimeout(() => reject(new Error(`Vite failed to start: ${output}`)), 30000);
	server.stdout.on("data", (chunk) => {
		output += chunk.toString();
		const match = output.match(/http:\/\/127\.0\.0\.1:\d+\//);
		if (match) { clearTimeout(timer); resolveUrl(match[0]); }
	});
	server.stderr.on("data", (chunk) => { output += chunk.toString(); });
	server.once("exit", (code) => { clearTimeout(timer); reject(new Error(`Vite exited ${code}: ${output}`)); });
}).catch((error) => { server.kill("SIGTERM"); throw error; });
const results = [];
try {
	for (const [name, engine, options] of [
		["chromium-desktop", chromium, { viewport: { width: 1280, height: 900 } }],
		["webkit-iphone", webkit, devices["iPhone 13"]],
	]) {
		const browser = await engine.launch({ headless: true });
		try {
			const page = await browser.newPage(options);
			const errors = [];
			page.on("pageerror", (error) => errors.push(error.message));
			await page.goto(`${url}src/components/SettingsPanel/fields/preview.html`);
			await page.getByRole("option", { name: "Work account" }).waitFor({ state: "attached" });
			assert.equal(await page.getByLabel("Account", { exact: true }).inputValue(), "work");
			assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, "controls overflow viewport");
			await page.screenshot({ path: `${artifacts}/${name}.png`, fullPage: true });
			await page.getByRole("button", { name: "Remove row core" }).click();
			assert.equal(await page.getByRole("group", { name: "Row core", exact: true }).count(), 0);
			assert.equal(await page.getByRole("group", { name: "Row mobile", exact: true }).getByLabel("Name", { exact: true }).inputValue(), "Mobile");
			await page.getByRole("button", { name: "Add row", exact: true }).click();
			assert.equal(await page.getByRole("group", { name: "Row new-1", exact: true }).getByLabel("Name", { exact: true }).inputValue(), "New project");
			await page.getByLabel("Feature", { exact: true }).check();
			assert.equal(await page.getByLabel("Bug", { exact: true }).isChecked(), true);
			await page.getByRole("button", { name: "Test connection", exact: true }).click();
			assert.equal(await page.getByRole("status", { name: "Connection status" }).textContent(), "Connection checked");
			await page.screenshot({ path: `${artifacts}/${name}-edited.png`, fullPage: true });
			assert.deepEqual(errors, []);
			results.push({ name, result: "passed", checks: ["account value", "no horizontal overflow", "stable row removal", "add row", "multiselect", "button/status", "no page errors"] });
		} finally { await browser.close(); }
	}
	await writeFile(`${artifacts}/results.json`, JSON.stringify(results, null, 2));
	process.stdout.write(`${JSON.stringify(results)}\n`);
} finally {
	server.kill("SIGTERM");
	await once(server, "exit");
}
