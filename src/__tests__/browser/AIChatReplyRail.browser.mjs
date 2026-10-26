import { execFile } from "node:child_process";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { promisify } from "node:util";
import { createServer } from "vite";
import solid from "vite-plugin-solid";
import { after, before, it } from "node:test";
import assert from "node:assert/strict";

// Real browser measurements: happy-dom cannot evaluate pseudo-elements or layout.
// Run: TUIC_TEST_CHROME=<binary> node --test src/__tests__/browser/AIChatReplyRail.browser.mjs
// Browser-only integration test: isolated from Vitest workers and native Vite resources.
let server;
const appearances = [];
before(async () => {
	const chrome = process.env.TUIC_TEST_CHROME;
	if (!chrome) throw new Error("TUIC_TEST_CHROME is required for rendered AI Chat appearance tests");
	const root = resolve(".tmp/aichat-appearance");
	await mkdir(root, { recursive: true });
	server = await createServer({
		configFile: false,
		plugins: [solid()],
		cacheDir: resolve(".tmp/aichat-appearance/vite-cache"),
		logLevel: "error",
		server: { host: "127.0.0.1", port: 0 },
		define: { __APP_VERSION__: '"test"' },
	});
	await server.listen();
	const address = server.httpServer.address();
	if (!address || typeof address === "string") throw new Error("No browser fixture port");
	for (const theme of ["dark", "light"]) {
		const args = [
			"--headless=new",
			"--disable-gpu",
			"--no-first-run",
			`--user-data-dir=${root}/chrome-${theme}`,
			"--window-size=1400,900",
			"--virtual-time-budget=8000",
			"--dump-dom",
		];
		if (process.env.TUIC_TEST_SCREENSHOT_DIR)
			args.push(`--screenshot=${process.env.TUIC_TEST_SCREENSHOT_DIR}/2026-10-16-aichat-${theme}.png`);
		args.push(`http://127.0.0.1:${address.port}/src/__tests__/browser/aiChatAppearance.html?theme=${theme}`);
		const { stdout } = await promisify(execFile)(chrome, args, { timeout: 30000, maxBuffer: 4 * 1024 * 1024 });
		const match = stdout.match(/<pre id="appearance-result">([^<]+)<\/pre>/);
		assert.ok(match, `Browser fixture did not finish: ${stdout}`);
		appearances.push(JSON.parse(match[1]));
	}
});
after(async () => {
	await server?.close();
});

it("renders borderless answers but retains quotation and terminal answer borders — catches answer rail leakage", () => {
	assert.equal(appearances.length, 2);
	for (const appearance of appearances) {
		assert.ok(["none", "normal"].includes(appearance.answer.content));
		assert.equal(parseFloat(appearance.answer.border), 0);
		assert.ok(parseFloat(appearance.quoteBorder) > 0);
		assert.equal(parseFloat(appearance.terminalBorder), 0);
		assert.equal(parseFloat(appearance.terminalAnswerBorder), 3);
	}
});
it("aligns rendered Stop, Park and Send bounds with the input — catches Stop offset mismatch", () => {
	assert.equal(appearances.length, 2);
	for (const appearance of appearances) {
		for (const controls of [appearance.busy, appearance.idle]) {
			for (const control of Object.values(controls)) {
				assert.ok(Math.abs(control.height - controls.input.height) < 0.1, JSON.stringify(controls));
				assert.ok(Math.abs(control.center - controls.input.center) < 0.1, JSON.stringify(controls));
			}
		}
	}
});
