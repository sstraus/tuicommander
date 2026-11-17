// Run on rb with its existing Playwright install; no application dependencies are added.
import { createRequire } from "node:module";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { createServer } from "vite";

const playwrightDir = process.env.PLAYWRIGHT_DIR;
if (!playwrightDir) throw new Error("Set PLAYWRIGHT_DIR to a directory with node_modules/playwright-core installed (browsers default to $PLAYWRIGHT_DIR/browsers); there is no default location.");
const { chromium, webkit, devices } = createRequire(`${playwrightDir}/`)("playwright-core");
process.env.PLAYWRIGHT_BROWSERS_PATH ??= `${playwrightDir}/browsers`;
const artifacts = "reports/visual/wi-b4-board";
await mkdir(artifacts, { recursive: true });
const server = await createServer({ configFile: "vitest.config.ts", server: { host: "127.0.0.1", port: 0 } });
await server.listen();
const url = `${server.resolvedUrls.local[0]}src/components/Tasks/board/__tests__/board-fixture.html`;
const results = [];
try {
	for (const [name, engine, contextOptions] of [["chromium", chromium, { viewport: { width: 1400, height: 900 } }], ["webkit-iphone", webkit, devices["iPhone 13"]]]) {
		const browser = await engine.launch({ headless: true });
		try {
			const context = await browser.newContext(contextOptions);
			const page = await context.newPage();
			const errors = [];
			page.on("pageerror", (error) => errors.push(error.message));
			await page.goto(url);
			const high = page.getByRole("region", { name: "High", exact: true });
			const low = page.getByRole("region", { name: "Low", exact: true });
			await high.getByRole("article", { name: "DEMO-1", exact: true }).waitFor();
			await page.screenshot({ path: `${artifacts}/${name}-initial.png`, fullPage: true });
			const drag = async (origin, target) => {
				const from = await origin.getByText("Fix terminal resize", { exact: true }).boundingBox();
				const to = await target.boundingBox();
				assert.ok(from && to, "card and target have geometry");
				await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
				await page.mouse.down();
				await page.mouse.move(to.x + to.width / 2, to.y + 100, { steps: 12 });
				await page.mouse.up();
			};
			if (name === "chromium") await drag(high, low);
			else await page.getByRole("combobox", { name: "Move DEMO-1", exact: true }).selectOption("low");
			await low.getByRole("article", { name: "DEMO-1", exact: true }).waitFor();
			await page.waitForFunction(() => {
				const option = document.querySelector('select[aria-label="Move DEMO-1"] option[value="high"]');
				return option && !option.disabled;
			});
			await low.scrollIntoViewIfNeeded();
			await page.screenshot({ path: `${artifacts}/${name}-accepted.png`, fullPage: true });
			await page.getByRole("checkbox", { name: "Reject moves" }).check();
			if (name === "chromium") await drag(low, high);
			else await page.getByRole("combobox", { name: "Move DEMO-1", exact: true }).selectOption("high");
			await page.getByRole("alert").filter({ hasText: "Demo rejection" }).waitFor();
			assert.equal(await low.getByRole("article", { name: "DEMO-1", exact: true }).count(), 1);
			assert.equal(await high.getByRole("article", { name: "DEMO-1", exact: true }).count(), 0);
			assert.equal(await page.locator("[data-board-ghost]").count(), 0);
			await page.screenshot({ path: `${artifacts}/${name}-rejected.png`, fullPage: true });
			assert.deepEqual(errors, [], "no browser runtime errors");
			results.push({ browser: name, accepted: true, rejected: true, cards: await page.getByRole("article").count(), errors });
			await writeFile(`${artifacts}/results.json`, JSON.stringify(results, null, 2));
		} finally { await browser.close(); }
	}
	process.stdout.write(`${JSON.stringify(results)}\n`);
} finally { await server.close(); }
