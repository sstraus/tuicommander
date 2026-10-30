// @vitest-environment node
import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

interface Box {
	top: number;
	bottom: number;
	left: number;
	right: number;
	width: number;
	height: number;
}
interface Geometry {
	name: string;
	pane: Box;
	toggle: Box;
	search: Box;
	controls: { title: string; box: Box; hit: boolean }[];
}

// Opt-in real-browser component test, like ChatReadability.test.ts. No DOM shim
// can measure CSS layout. Uses this checkout's Vite and the approved named tab.
const directory = process.env.TUIC_SEARCH_GEOMETRY_DIR;
describe.skipIf(!directory)("terminal search geometry", () => {
	let vite: ChildProcess | undefined;
	const samples: Geometry[] = [];
	let closeWorks = false;
	const wrapper = join(homedir(), "Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh");
	const run = (...args: string[]) =>
		execFileSync("perl", ["-e", "alarm 40; exec @ARGV", wrapper, ...args], {
			encoding: "utf8",
			timeout: 45000,
			env: { ...process.env, AB_SESSION: "tuic-search-overlap", AB_AGENT_BROWSER_DIR: join(directory!, "ab-empty") },
			stdio: ["ignore", "pipe", "pipe"],
		});
	beforeAll(async () => {
		mkdirSync(join(directory!, "ab-empty"), { recursive: true });
		vite = spawn("pnpm", ["exec", "vite", "--host", "127.0.0.1", "--port", "15381", "--strictPort"], {
			cwd: process.cwd(),
			stdio: ["ignore", "pipe", "pipe"],
			detached: true,
		});
		await new Promise<void>((resolve, reject) => {
			vite!.stdout!.on("data", (chunk) => {
				if (String(chunk).includes("http://127.0.0.1:15381")) resolve();
			});
			vite!.once("exit", (code) => reject(new Error(`Fixture Vite exited ${code}`)));
			vite!.once("error", reject);
		});
		try {
			run("mac-chrome", "http://127.0.0.1:15381/src/__tests__/browser/terminalSearchGeometry.html");
			run(
				"--",
				"wait",
				"--fn",
				"typeof window.terminalSearchGeometry === 'function' || window.fixtureErrors?.length > 0",
			);
			const errors = JSON.parse(run("--", "eval", "window.fixtureErrors")) as string[];
			if (errors.length) throw new Error(errors.join("\n"));
			for (const width of [1400, 390]) {
				run("--", "set", "viewport", String(width), "1100");
				const geometry = JSON.parse(run("--", "eval", "window.terminalSearchGeometry()")) as Geometry[];
				samples.push(...geometry);
				writeFileSync(join(directory!, `${width}.json`), JSON.stringify(geometry, null, 2));
				run("--", "screenshot", join(directory!, `${width}.png`), "--full");
			}
			closeWorks = JSON.parse(run("--", "eval", "window.terminalSearchCloseAndReopen()"));
		} catch (error) {
			writeFileSync(join(directory!, "browser-errors.txt"), run("--", "eval", "window.fixtureErrors"));
			writeFileSync(join(directory!, "browser-snapshot.txt"), run("--", "snapshot"));
			throw error;
		} finally {
			run("--", "tab", "close");
		}
	}, 240000);
	afterAll(async () => {
		// Only the process group started by this test, never another Vite/app.
		if (vite?.pid && vite.exitCode === null) {
			const closed = new Promise<void>((resolve) => vite!.once("close", () => resolve()));
			process.kill(-vite.pid, "SIGTERM");
			await closed;
		}
	});
	it("keeps search clear of the switch — catches CLI Chat covering navigation and close", () => {
		expect(samples).toHaveLength(8);
		for (const sample of samples) {
			const { search, toggle } = sample;
			expect
				.soft(
					search.bottom <= toggle.top ||
						toggle.bottom <= search.top ||
						search.right <= toggle.left ||
						toggle.right <= search.left,
					JSON.stringify(sample),
				)
				.toBe(true);
		}
	});
	it("keeps all search and view controls hittable — catches invisible overlay interception", () => {
		for (const sample of samples) {
			expect(sample.controls.length).toBeGreaterThanOrEqual(8);
			for (const control of sample.controls) expect.soft(control.hit, `${sample.name}: ${control.title}`).toBe(true);
		}
	});
	it("fits search within narrow panes and closes and reopens — catches off-pane controls or broken close", () => {
		expect(closeWorks).toBe(true);
		for (const sample of samples) {
			expect.soft(sample.search.left).toBeGreaterThanOrEqual(sample.pane.left);
			expect.soft(sample.search.right).toBeLessThanOrEqual(sample.pane.right);
			expect.soft(sample.search.width).toBeGreaterThan(0);
		}
	});
});
