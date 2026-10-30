// @vitest-environment node
import { type ChildProcess, execFile, spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { promisify } from "node:util";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const run = promisify(execFile);
const wrapper = `${process.env.HOME}/Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh`;
const evidence = `${process.env.HOME}/Gits/personal/orchestrator/reports/tuicommander/img/2026-10-30-mobile-sessions`;
// Like FormFocus.test.ts, live-browser checks are opt-in: ordinary Vitest must not
// launch a browser on rb/CI. Requires the managed agent-browser stealth wrapper.
describe.skipIf(process.env.TUIC_MOBILE_BROWSER !== "1")("mobile browser list geometry", () => {
	let server: ChildProcess;
	const browser = (...args: string[]) =>
		run(wrapper, args, { env: { ...process.env, AB_SESSION: "tuic-mobile-long-names" }, timeout: 30000 });
	beforeAll(async () => {
		await new Promise<void>((resolve, reject) => {
			server = spawn(process.execPath, ["src/mobile/__tests__/browser/longNames.server.mjs"], {
				stdio: ["ignore", "pipe", "pipe"],
			});
			let errors = "";
			server.stderr?.on("data", (data) => {
				errors += data;
			});
			server.stdout?.on("data", (data) => {
				if (String(data).includes("fixture-ready")) resolve();
			});
			server.once("error", reject);
			server.once("exit", (code) => {
				if (code) reject(new Error(errors));
			});
		});
		await mkdir(evidence, { recursive: true });
		await browser("http://127.0.0.1:14369/mobile.html");
		await browser("--", "set", "viewport", "390", "844");
	}, 60000);
	afterAll(async () => {
		try {
			await browser("--", "tab", "close");
		} finally {
			if (server && server.exitCode === null)
				await new Promise<void>((resolve) => {
					server.once("exit", () => resolve());
					server.kill("SIGTERM");
				});
		}
	});
	// Catches: an unbroken flex-item name escapes its card and displaces the badge.
	it("mobile_long_identity_stays_inside_viewport", async () => {
		await browser("--", "wait", "--text", "gate-auto-");
		await browser("--", "screenshot", `${evidence}/${process.env.MOBILE_LAYOUT_PHASE ?? "after"}.png`);
		const { stdout } = await browser(
			"--",
			"eval",
			`JSON.stringify((() => {
 const main = document.querySelector('button[data-control-layout="row"][aria-label^="Open session"]');
 const card = main.parentElement; const badge = [...main.querySelectorAll('span')].find(e => e.textContent === 'Idle');
 const kill = document.querySelector('[data-testid="kill-btn"]'); const b = badge.getBoundingClientRect(); const k = kill.getBoundingClientRect();
 return {viewport: innerWidth, screenWidth: document.getElementById('mobile-app').scrollWidth, cardWidth: card.scrollWidth, cardClient: card.clientWidth, badgeRight:b.right, killLeft:k.left, badgeWidth:b.width};
 })())`,
		);
		// agent-browser emits a JSON string result for this expression.
		const line = stdout.trim();
		const first = JSON.parse(line);
		const result = typeof first === "string" ? JSON.parse(first) : first;
		await writeFile(`${evidence}/${process.env.MOBILE_LAYOUT_PHASE ?? "after"}.json`, JSON.stringify(result, null, 2));
		expect(result.viewport).toBe(390);
		expect(result.cardClient).toBeGreaterThan(0);
		expect(result.badgeWidth).toBeGreaterThan(0);
		expect(result.screenWidth).toBeLessThanOrEqual(result.viewport);
		expect(result.cardWidth).toBeLessThanOrEqual(result.cardClient);
		expect(result.badgeRight).toBeLessThanOrEqual(result.killLeft);
	}, 60000);

	// Catches: the same unbroken identity escapes sibling file, progress or activity rows.
	it.each(["files", "activity", "progress"])(
		"keeps %s list text within the mobile viewport",
		async (mode) => {
			await browser(`http://127.0.0.1:14369/mobile.html?mode=${mode}`);
			await browser("--", "wait", "--text", "aaaaaaaaaa");
			await browser("--", "screenshot", `${evidence}/${mode}.png`);
			const { stdout } = await browser(
				"--",
				"eval",
				"JSON.stringify({width:innerWidth,scroll:document.getElementById('mobile-app').scrollWidth})",
			);
			const first = JSON.parse(stdout.trim());
			const result = typeof first === "string" ? JSON.parse(first) : first;
			await writeFile(`${evidence}/${mode}.json`, JSON.stringify(result, null, 2));
			expect(result.width).toBe(390);
			expect(result.scroll).toBeLessThanOrEqual(result.width);
		},
		30000,
	);
});
