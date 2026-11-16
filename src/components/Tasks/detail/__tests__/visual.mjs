// Run with rb's existing Playwright installation; never add a project dependency.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { mkdir } from "node:fs/promises";
const playwrightDir = process.env.PLAYWRIGHT_DIR;
if (!playwrightDir) throw new Error("Set PLAYWRIGHT_DIR to a directory with node_modules/playwright-core installed (browsers default to $PLAYWRIGHT_DIR/browsers); there is no default location.");
const require = createRequire(`${playwrightDir}/`);
const { chromium, webkit, devices } = require("playwright-core");
const url = "http://127.0.0.1:4179/src/components/Tasks/detail/__tests__/preview.html";
await mkdir("reports/visual", { recursive: true });
for (const [name, engine, options] of [
	["chromium-desktop", chromium, { viewport: { width: 1280, height: 1000 } }],
	["webkit-iphone", webkit, devices["iPhone 13"]],
]) {
	const browser = await engine.launch({ headless: true });
	try {
		const page = await browser.newPage(options);
		const errors = [];
		page.on("pageerror", (error) => errors.push(error.message));
		await page.goto(url);
		await page.getByRole("heading", { name: "Preserve terminal history while an agent writes" }).waitFor();
		await page.screenshot({ path: `reports/visual/tasks-detail-${name}-top.png`, fullPage: true });
		await page.locator("summary").filter({ hasText: "Actions performed" }).click();
		assert.equal(await page.locator("details[open]").count(), 1);
		await page.screenshot({ path: `reports/visual/tasks-detail-${name}.png`, fullPage: true });
		assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, "detail overflows viewport");
		await page.getByRole("button", { name: "Refresh", exact: true }).click();
		await page.getByRole("button", { name: "Refresh", exact: true }).waitFor({ state: "visible" });
		assert.equal(await page.locator("details[open]").count(), 1);
		await page.getByLabel("Reject status changes").check();
		await page.getByRole("button", { name: "Closed", exact: true }).click();
		await page.getByRole("alert").waitFor();
		assert.equal(await page.getByRole("status").innerText(), "Open");
		await page.getByLabel("Reject status changes").uncheck();
		await page.getByRole("button", { name: "Closed", exact: true }).click();
		await page.waitForFunction(() => document.querySelector('[role="status"]')?.textContent === "Closed");
		await page.getByLabel("New comment").fill("Verified comment from Playwright");
		await page.getByRole("button", { name: "Add comment", exact: true }).click();
		await page.getByText("Verified comment from Playwright", { exact: true }).waitFor();
		assert.equal(await page.getByLabel("New comment").inputValue(), "");
		assert.deepEqual(errors, [], "browser runtime errors");
		console.log(`${name}: screenshot, disclosure refresh, rollback, transition and comment passed`);
	} finally { await browser.close(); }
}
