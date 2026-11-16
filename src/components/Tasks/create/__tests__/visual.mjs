// Run with rb's existing Playwright installation; never add a project dependency.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
const playwrightDir = process.env.PLAYWRIGHT_DIR;
if (!playwrightDir) throw new Error("Set PLAYWRIGHT_DIR to a directory with node_modules/playwright-core installed (browsers default to $PLAYWRIGHT_DIR/browsers); there is no default location.");
const require = createRequire(`${playwrightDir}/`);
const { chromium, webkit, devices } = require("playwright-core");
const url = "http://127.0.0.1:4180/src/components/Tasks/create/__tests__/preview.html";
await mkdir("reports/visual", { recursive: true });
for (const [name, engine, options] of [
	["chromium-desktop", chromium, { viewport: { width: 1280, height: 900 } }],
	["webkit-iphone", webkit, devices["iPhone 13"]],
]) {
	// rb lacks WebKit's system libraries; the list stream's launcher loads the workspace copies.
	const executablePath = engine === webkit ? resolve("src/components/Tasks/list/webkit-run.sh") : undefined;
	const browser = await engine.launch({ headless: true, executablePath });
	try {
		const page = await browser.newPage(options);
		const errors = [];
		page.on("pageerror", (error) => errors.push(error.message));
		await page.goto(url);
		await page.getByRole("heading", { name: "New task in Local demo" }).waitFor();
		const create = page.getByRole("button", { name: "Create", exact: true });
		await page.getByLabel("Type *").selectOption("incident");
		await page.getByLabel("Due date *").waitFor();
		await page.getByLabel("Title *").fill("Terminal loses lines on resize");
		await page.getByLabel("Priority *").selectOption("high");
		assert.equal(await create.isDisabled(), true, "Create enabled with an empty required date");
		await page.getByText("Required: Due date").waitFor();
		await page.screenshot({ path: `reports/visual/tasks-create-${name}-incident.png`, fullPage: true });
		await page.getByLabel("Type *").selectOption("issue");
		await page.getByLabel("Due date *").waitFor({ state: "detached" });
		assert.equal(await page.getByLabel("Priority *").inputValue(), "high", "shared field lost on type change");
		assert.equal(await create.isDisabled(), false, "Create still blocked by the old type's field");
		assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, "form overflows viewport");
		await page.screenshot({ path: `reports/visual/tasks-create-${name}-issue.png`, fullPage: true });
		await create.click();
		await page.getByRole("heading", { name: "Terminal loses lines on resize" }).waitFor();
		await page.screenshot({ path: `reports/visual/tasks-create-${name}-opened.png`, fullPage: true });
		assert.deepEqual(errors, [], "browser runtime errors");
		console.log(`${name}: type switch, required gate, create and open passed`);
	} finally {
		await browser.close();
	}
}
