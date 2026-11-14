import { execFile } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { resolve } from "node:path";
import { promisify } from "node:util";
import { build, type PreviewServer, preview } from "vite";
import solid from "vite-plugin-solid";
import { afterAll, beforeAll, expect, it } from "vitest";

// Dedicated browser config keeps workstation browser checks out of happy-dom runs.
const run = promisify(execFile);
const wrapper = resolve(homedir(), "Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh");
const evidence = resolve(process.env.TUIC_COMPOSE_EVIDENCE ?? `${homedir()}/Gits/.tmp/results/2697`);
const environment = { ...process.env, AB_SESSION: "tuic-2697-compose", AB_KEEP_FOCUS: "1" };
let server: PreviewServer;

async function browser(...args: string[]) {
	const { stdout } = await run("perl", ["-e", "alarm 50; exec @ARGV", wrapper, ...args], { env: environment });
	return stdout.trim();
}

async function evaluate(expression: string) {
	const output = await browser("--", "eval", expression);
	return output === "undefined" ? null : JSON.parse(output);
}

async function read(label: string) {
	const value = await evaluate("window.composeReservation.read()");
	await writeFile(resolve(evidence, `${label}.json`), JSON.stringify(value, null, 2));
	await browser("--", "screenshot", resolve(evidence, `${label}.png`));
	return value;
}

beforeAll(async () => {
	await mkdir(evidence, { recursive: true });
	const outDir = resolve(".tmp/compose-reservation-dist");
	await build({
		configFile: false,
		plugins: [
			solid(),
			{
				name: "compose-proof-transport",
				enforce: "pre",
				resolveId(source, importer) {
					if (source === "./canvasTerminalTransport" && importer?.endsWith("/CanvasTerminal.tsx"))
						return resolve("src/__tests__/browser/composeReservationTransport.ts");
				},
			},
		],
		logLevel: "error",
		cacheDir: resolve(".tmp/compose-reservation-vite"),
		build: { outDir, rollupOptions: { input: resolve("src/__tests__/browser/composeReservation.html") } },
	});
	server = await preview({
		configFile: false,
		logLevel: "error",
		build: { outDir },
		preview: { host: "127.0.0.1", port: 0 },
	});
	const address = server.httpServer?.address();
	if (!address || typeof address === "string") throw new Error("Vite did not bind a port");
	await browser(`http://127.0.0.1:${address.port}/src/__tests__/browser/composeReservation.html`);
	await browser("--", "bringtofront");
	await browser("--", "set", "viewport", "1400", "900");
	expect(await evaluate("typeof navigator.webdriver")).toBe("undefined");
	await evaluate("window.composeReservation.ready()");
});

afterAll(async () => {
	try {
		await browser("--", "tab", "close");
	} finally {
		await new Promise<void>((resolve, reject) =>
			server?.httpServer.close((error) => (error ? reject(error) : resolve())),
		);
	}
});

// Catches: an absolute composer leaves the terminal's bottom rows underneath it.
it("opening CLI Compose reserves its minimum and closing restores the terminal", async () => {
	await evaluate("window.composeReservation.close()");
	const closed = await read("closed");
	await evaluate("window.composeReservation.open()");
	const opened = await read("open");
	expect(opened.editorHeight).toBe(36);
	expect(opened.terminalBottom).toBe(opened.panelTop);
	expect(opened.editorTop - opened.terminalBottom).toBe(1);
	expect(closed.terminalHeight - opened.terminalHeight).toBe(opened.panelHeight);
	expect(opened.heights.length - closed.heights.length).toBe(1);
	await evaluate("window.composeReservation.draft(30)");
	const grown = await read("unpinned-grown");
	expect(grown.terminalHeight).toBe(opened.terminalHeight);
	expect(grown.heights).toEqual(opened.heights);
	expect(grown.editorTop).toBeLessThan(grown.terminalBottom);
	await evaluate("window.composeReservation.draft(1)");
	await evaluate("window.composeReservation.close()");
	const restored = await read("restored");
	expect(restored.terminalHeight).toBe(closed.terminalHeight);
	expect(restored.heights.length - opened.heights.length).toBe(1);
});

// Catches: pinned Compose's intrinsic draft height continuously shrinks the PTY viewport.
it("growing and shrinking pinned CLI Compose overlays without resizing the terminal", async () => {
	await evaluate("window.composeReservation.close()");
	await evaluate("window.composeReservation.open(true)");
	const opened = await read("pinned-open");
	await evaluate("window.composeReservation.draft(30)");
	const grown = await read("grown");
	expect(grown.editorHeight).toBe(96);
	expect(grown.terminalHeight).toBe(opened.terminalHeight);
	expect(grown.heights).toEqual(opened.heights);
	expect(grown.editorTop).toBeLessThan(grown.terminalBottom);
	await evaluate("window.composeReservation.draft(1)");
	const shrunk = await read("shrunk");
	expect(shrunk.terminalHeight).toBe(opened.terminalHeight);
	expect(shrunk.heights).toEqual(opened.heights);
	await evaluate("window.composeReservation.close()");
});

// Catches: minimum reservation leaking into Chat docking or the mobile overlay.
it("keeps Chat fully docked and mobile CLI overlaid", async () => {
	await evaluate("window.composeReservation.open(false, true)");
	const chat = await read("chat-open");
	await evaluate("window.composeReservation.draft(30)");
	const grownChat = await read("chat-grown");
	expect(chat.terminalHeight - grownChat.terminalHeight).toBe(60);
	await evaluate("window.composeReservation.close()");
	await browser("--", "set", "viewport", "390", "900");
	const mobile = await read("mobile-closed");
	await evaluate("window.composeReservation.open()");
	const mobileOpen = await read("mobile-open");
	expect(mobileOpen.editorHeight).toBe(44);
	expect(mobileOpen.terminalHeight).toBe(mobile.terminalHeight);
	await evaluate("window.composeReservation.close()");
});

// Catches: a pinned multiline draft hides the shell's input row after terminal focus returns.
it("keeps the terminal input row visible beneath a grown pinned draft", async () => {
	await browser("--", "set", "viewport", "1400", "900");
	await evaluate("window.composeReservation.close()");
	await evaluate("window.composeReservation.open(true)");
	await evaluate("window.composeReservation.draft(30)");
	const inputRowVisible = await evaluate(`(() => {
		const terminal = document.querySelector('[data-terminal]');
		terminal.tabIndex = 0;
		terminal.focus();
		const row = terminal.querySelector('pre:last-child');
		const box = row.getBoundingClientRect();
		const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
		return terminal.contains(hit);
	})()`);
	await read("pinned-terminal-input-row");
	expect(inputRowVisible, "Pinned Compose covers the terminal input row after terminal focus returns").toBe(true);
});

// Catches: changes in draft size or focus dispatching real CanvasTerminal PTY resizes.
it.each([false, true])("dispatches one PTY resize on open and close only (pinned=%s)", async (pinned) => {
	await evaluate("window.composeReservation.close()");
	await evaluate("window.composeReservation.settle()");
	const baseline = await evaluate("window.composeReservation.read().resizeCalls.length");
	await evaluate(`window.composeReservation.open(${pinned})`);
	await evaluate("window.composeReservation.settle()");
	const opened = await read(`resize-${pinned}-open`);
	expect(opened.resizeCalls.length - baseline).toBe(1);
	const growth = await evaluate(`(async () => {
		const trace = [];
		for (let lines = 1; lines <= 30; lines++) {
			await window.composeReservation.draft(lines);
			await window.composeReservation.settle();
			trace.push({lines, calls:window.composeReservation.read().resizeCalls.length});
		}
		return trace;
	})()`);
	await writeFile(resolve(evidence, `resize-${pinned}-line-by-line.json`), JSON.stringify(growth, null, 2));
	expect(growth).toHaveLength(30);
	for (const step of growth) expect(step.calls - baseline).toBe(1);
	const grown = await read(`resize-${pinned}-grown`);
	expect(grown.editorHeight).toBe(96);
	await browser("--", "click", '[data-terminal-container] canvas[tabindex="0"]');
	await evaluate("window.composeReservation.settle()");
	const blurred = await read(`resize-${pinned}-blurred`);
	expect(blurred.terminalFocused).toBe(true);
	expect(blurred.editorHeight).toBe(36);
	expect(blurred.draft).toBe(grown.draft);
	expect(blurred.scroller.scroll).toBeGreaterThan(blurred.scroller.height);
	expect(
		await evaluate(`(() => {
		const scroller = document.querySelector('.cm-scroller');
		scroller.scrollTop = scroller.scrollHeight;
		return scroller.scrollTop;
	})()`),
	).toBeGreaterThan(0);
	expect(blurred.resizeCalls.length - baseline).toBe(1);
	await evaluate("window.composeReservation.focusEditor()");
	const refocused = await read(`resize-${pinned}-refocused`);
	expect(refocused.editorHeight).toBe(96);
	expect(refocused.draft).toBe(grown.draft);
	expect(refocused.resizeCalls.length - baseline).toBe(1);
	if (pinned) {
		await browser("--", "press", "Escape");
		await evaluate("window.composeReservation.settle()");
		const escaped = await read("resize-pinned-escaped");
		expect(escaped.terminalFocused).toBe(true);
		expect(escaped.editorHeight).toBe(36);
		expect(escaped.resizeCalls.length - baseline).toBe(1);
		await evaluate("window.composeReservation.focusEditor()");
	}
	await evaluate("window.composeReservation.draft(1)");
	await evaluate("window.composeReservation.settle()");
	const shrunk = await read(`resize-${pinned}-shrunk`);
	expect(shrunk.resizeCalls.length - baseline).toBe(1);
	await evaluate("window.composeReservation.close()");
	await evaluate("window.composeReservation.settle()");
	const closed = await read(`resize-${pinned}-closed`);
	expect(closed.resizeCalls.length - shrunk.resizeCalls.length).toBe(1);
	expect(closed.resizeCalls.length - baseline).toBe(2);
});
