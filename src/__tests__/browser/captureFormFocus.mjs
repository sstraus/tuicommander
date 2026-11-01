import { execFile } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";

// Separate Node process keeps real browser I/O outside Vitest's inert transport mocks.
const [baseUrl, directory] = process.argv.slice(2);
mkdirSync(join(directory, "ab-empty"), { recursive: true });
const wrapper = join(homedir(), "Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh");
const run = async (...args) => (await promisify(execFile)("perl", ["-e", "alarm 40; exec @ARGV", wrapper, ...args], {
	env: { ...process.env, AB_SESSION: `tuic-focus-${process.pid}`, AB_AGENT_BROWSER_DIR: join(directory, "ab-empty") },
	encoding: "utf8", timeout: 45000,
})).stdout;
const fixtureUrl = `${baseUrl}/src/__tests__/browser/formFocus.html`;
let socket;
let setFocusEmulation;
try {
	await run("mac-chrome", fixtureUrl);
	await run("--", "set", "viewport", "1400", "1100");
	await run("--", "wait", "input[aria-label='Title']");
	const endpoint = new URL((await run("--", "get", "cdp-url")).trim());
	endpoint.protocol = endpoint.protocol.startsWith("wss") || endpoint.protocol === "https:" ? "https:" : "http:";
	endpoint.pathname = "/json/list";
	const pages = await (await fetch(endpoint)).json();
	const page = pages.find((page) => page.url === fixtureUrl);
	if (!page) throw new Error("Own focus fixture target is missing");
	socket = new WebSocket(page.webSocketDebuggerUrl);
	await new Promise((resolve, reject) => {
		socket.addEventListener("open", resolve, { once: true });
		socket.addEventListener("error", reject, { once: true });
	});
	let sequence = 0;
	setFocusEmulation = (enabled) => new Promise((resolve, reject) => {
		const id = ++sequence;
		const listener = (event) => {
			const response = JSON.parse(event.data);
			if (response.id !== id) return;
			clearTimeout(timer);
			socket.removeEventListener("message", listener);
			response.error ? reject(new Error(JSON.stringify(response.error))) : resolve();
		};
		const timer = setTimeout(() => {
			socket.removeEventListener("message", listener);
			reject(new Error("Focus emulation did not respond"));
		}, 10000);
		socket.addEventListener("message", listener);
		socket.send(JSON.stringify({ id, method: "Emulation.setFocusEmulationEnabled", params: { enabled } }));
	});
	// Shared Chrome activation can remove native Select :focus-visible without
	// changing activeElement. Emulate foreground focus on THIS target only; real
	// trusted Tab input and every focus/ring assertion remain unchanged.
	await setFocusEmulation(true);
	const sample = async () => JSON.parse(await run("--", "eval", `(() => {
		const node = document.activeElement;
		const ring = node.getAttribute('role') === 'switch' ? node.parentElement : node;
		const style = getComputedStyle(ring);
		const color = (token) => {
			const probe = document.createElement('span');
			probe.style.color = 'var(' + token + ')';
			document.body.append(probe);
			const value = getComputedStyle(probe).color;
			probe.remove();
			return value;
		};
		return { name: node.getAttribute('aria-label') || document.querySelector('label[for="' + node.id + '"]')?.textContent || node.textContent,
			focused: node.matches(':focus-visible'), outline: style.outlineStyle,
			offset: style.outlineOffset, border: style.borderTopColor, shadow: style.boxShadow,
			accent: color('--accent'), error: color('--error') };
	})()`));
	const samples = [];
	for (let index = 0; index < 13; index++) {
		// Probe each consumer before loading the next stylesheet: CSS Modules
		// composes can re-inject shared rules and mask the previous consumer.
		if (index === 9 || index === 11) {
			const [context, path, key] = index === 9
				? ['stories', 'StoriesDialog/StoriesDialog.module.css', 'form']
				: ['workflow', 'WorkflowDesigner/WorkflowDesigner.module.css', 'designer'];
			await run("--", "eval", `(async () => {
				const { default: styles } = await import('/src/components/${path}');
				document.querySelector('[data-focus-context="${context}"]').className = styles['${key}'];
			})()`);
		}
		await run("--", "press", "Tab");
		samples.push(await sample());
		if (index === 10 || index === 12) {
			const context = index === 10 ? "stories" : "workflow";
			await run("--", "screenshot", `[data-focus-context="${context}"]`, join(directory, `${context}-focus-1400.png`));
		}
	}
	await run("--", "click", ".tweak-highlight");
	await run("--", "wait", "[data-tweak-popover] textarea");
	samples.push({ ...await sample(), name: "Comment popover" });
	await run("--", "screenshot", "[data-tweak-popover]", join(directory, "comment-popover-1400.png"));
	await run("--", "press", "Escape");
 await run("--", "eval", "window.mountSelectFocus()");
 await run("--", "click", "button[aria-label='Focus start']");
 for (let index = 0; index < 6; index++) {
  await run("--", "press", "Tab");
  samples.push(await sample());
 }
 await run("--", "eval", "window.mountPromptFocus()");
 await run("--", "click", "button[title='Edit']");
 await run("--", "wait", "[role='dialog'][aria-label='Edit Prompt']");
 // Click the preceding Placement checkbox, then traverse to Execution with
 // trusted keyboard input (not a synthetic focus-visible state).
 await run("--", "eval", "document.querySelectorAll('[role=dialog] input[type=checkbox]')[7].setAttribute('data-focus-before-execution', '')");
 await run("--", "click", "[data-focus-before-execution]");
 await run("--", "press", "Tab");
 samples.push(await sample());
 await run("--", "screenshot", "[role='dialog'][aria-label='Edit Prompt']", join(directory, "edit-prompt-execution-1400.png"));
 const rows = JSON.parse(await run("--", "eval", `(async () => {
  const { default: styles } = await import('/src/components/SettingsPanel/Settings.module.css');
  const dialog = document.querySelector('[role=dialog]');
  const rows = [...dialog.querySelectorAll('.' + styles.settingRow)];
  const last = rows.at(-1);
  return { lastLabel: last.querySelector('label').textContent,
   lastBorder: getComputedStyle(last).borderBottomWidth,
   previousBorder: getComputedStyle(rows.at(-2)).borderBottomWidth,
   trailingBorders: [...last.parentElement.children].slice([...last.parentElement.children].indexOf(last) + 1).map(node => getComputedStyle(node).borderTopWidth) };
 })()`));
 writeFileSync(join(directory, "prompt-rows.json"), JSON.stringify(rows, null, 2));
 writeFileSync(join(directory, "computed-focus.json"), JSON.stringify(samples, null, 2));
} finally {
	try {
		if (setFocusEmulation) await setFocusEmulation(false);
	} finally {
		socket?.close();
		await run("--", "tab", "close");
	}
}
