import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "vite";
import solid from "vite-plugin-solid";

const directory = path.dirname(new URL(import.meta.url).pathname);
const root = path.resolve(directory, "../../..");
const runtime = path.join(directory, ".runtime");
const profile = path.join(runtime, "chrome");
await fs.mkdir(profile, { recursive: true });
const chrome =
	process.env.TUIC_EVIDENCE_CHROME ??
	path.join(
		os.homedir(),
		".agent-browser/browsers/chrome-149.0.7827.54/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
	);
const server = await createServer({
	configFile: false,
	root,
	define: { __APP_VERSION__: JSON.stringify("visual evidence") },
	plugins: [solid()],
	server: { host: "127.0.0.1", port: 14569, strictPort: true, watch: { ignored: ["**/.runtime/**"] } },
});
let browser;
let socket;
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function until(description, work) {
	const end = Date.now() + 20000;
	while (Date.now() < end) {
		const result = await work().catch(() => null);
		if (result) return result;
		await pause(100);
	}
	throw new Error("Timed out: " + description);
}
try {
	await server.listen();
	const stderr = await fs.open(path.join(runtime, "chrome.log"), "w");
	await fs.rm(path.join(profile, "DevToolsActivePort"), { force: true });
	browser = spawn(
		chrome,
		[
			"--headless=new",
			"--remote-debugging-port=0",
			"--no-first-run",
			"--no-default-browser-check",
			"--use-mock-keychain",
			"--password-store=basic",
			"--disable-background-networking",
			"--disable-extensions",
			"--disable-gpu",
			"--user-data-dir=" + profile,
			"about:blank",
		],
		{ detached: true, stdio: ["ignore", "ignore", stderr.fd] },
	);
	const port = await until("Chrome CDP port", async () =>
		Number((await fs.readFile(path.join(profile, "DevToolsActivePort"), "utf8")).split("\n")[0]),
	);
	const tabs = await until("Chrome CDP page", async () => {
		const response = await fetch("http://127.0.0.1:" + port + "/json/list");
		const pages = await response.json();
		return pages.some((page) => page.type === "page") ? pages : null;
	});
	socket = new WebSocket(tabs.find((tab) => tab.type === "page").webSocketDebuggerUrl);
	await new Promise((resolve, reject) => {
		socket.onopen = resolve;
		socket.onerror = reject;
	});
	let sequence = 0;
	const pending = new Map();
	socket.onmessage = (event) => {
		const message = JSON.parse(event.data);
		const item = pending.get(message.id);
		if (!item) return;
		pending.delete(message.id);
		clearTimeout(item.timer);
		message.error ? item.reject(new Error(JSON.stringify(message.error))) : item.resolve(message.result);
	};
	const call = (method, params = {}) =>
		new Promise((resolve, reject) => {
			const id = ++sequence;
			const timer = setTimeout(() => {
				pending.delete(id);
				reject(new Error("CDP timeout: " + method));
			}, 15000);
			pending.set(id, { resolve, reject, timer });
			socket.send(JSON.stringify({ id, method, params }));
		});
	const evaluate = async (expression) => {
		const result = await call("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
		if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
		return result.result.value;
	};
	await call("Page.enable");
	await call("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
	await call("Emulation.setTouchEmulationEnabled", { enabled: true });
	await call("Page.navigate", { url: "http://127.0.0.1:14569/docs/evidence/voice-mute-1659/index.html" });
	await until("real mobile voice checkbox", () =>
		evaluate('!!document.querySelector("input[type=checkbox]") && window.voiceEvidence?.state().armed === true'),
	);
	await evaluate("document.fonts.ready.then(() => true)");
	const measurements = [];
	async function capture(name, checked, armed) {
		await until(name + " persisted state", () =>
			evaluate(
				'document.querySelector("input[type=checkbox]")?.checked === ' +
					checked +
					" && window.voiceEvidence.state().persisted === " +
					checked,
			),
		);
		const state = await evaluate(
			'(() => { const input=document.querySelector("input[type=checkbox]"); const label=input.closest("label"); const rect=label.getBoundingClientRect(); const css=getComputedStyle(input); return { checked:input.checked, disabled:input.disabled, inputWidth:css.width, inputHeight:css.height, label:label.textContent.trim(), bounds:{x:rect.x,y:rect.y,width:rect.width,height:rect.height,right:rect.right,bottom:rect.bottom}, viewport:{width:innerWidth,height:innerHeight}, ...window.voiceEvidence.state() }; })()',
		);
		if (state.armed !== armed) throw new Error(name + ": unexpected armed state");
		if (state.bounds.width <= 0 || state.bounds.height <= 0 || state.bounds.right > 390 || state.bounds.bottom > 844)
			throw new Error(name + ": clipped or invisible control");
		const shot = await call("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
		await fs.writeFile(path.join(directory, name + ".png"), Buffer.from(shot.data, "base64"));
		measurements.push({ name, ...state, requests: [...state.requests] });
		console.log(
			name,
			JSON.stringify({ checked: state.checked, persisted: state.persisted, armed: state.armed, bounds: state.bounds }),
		);
	}
	async function tapToggle() {
		const point = await evaluate(
			'(() => { const r=document.querySelector("input[type=checkbox]").getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()',
		);
		await call("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [point] });
		await call("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
	}
	await capture("phone-spoken-replies-on", true, true);
	await tapToggle();
	await capture("phone-spoken-replies-off", false, true);
	await tapToggle();
	await capture("phone-spoken-replies-restored", true, true);
	await evaluate("window.voiceEvidence.settings()");
	await until("Settings voice section", () => evaluate('!!document.querySelector("input[type=checkbox]")'));
	await capture("phone-settings-on", true, false);
	await tapToggle();
	await capture("phone-settings-off", false, false);
	await fs.writeFile(
		path.join(directory, "visual-measurements.json"),
		JSON.stringify(
			{
				method: "Native macOS headless Chrome 149 CDP; worktree Vite; real UI/store; owned HTTP fixtures only",
				sourceCommit: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
				measurements,
			},
			null,
			2,
		) + "\n",
	);
} finally {
	socket?.close();
	if (browser?.pid) {
		try {
			process.kill(-browser.pid, "SIGTERM");
		} catch (error) {
			if (error.code !== "ESRCH") throw error;
		}
		await Promise.race([new Promise((resolve) => browser.once("exit", resolve)), pause(3000)]);
	}
	await server.close();
}
