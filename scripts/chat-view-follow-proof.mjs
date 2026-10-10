// Invoked by the opt-in Rust HTTP fixture. Uses real production stores,
// TerminalChatView, snapshots and SSE; only transcript binding is fixture-owned.
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import { createServer } from "vite";
import solid from "vite-plugin-solid";

const run = promisify(execFile);
const [backend, evidence] = process.argv.slice(2);
assert(backend && evidence, "pass backend URL and evidence directory");
const root = process.cwd();
assert(evidence.startsWith(`${root}${path.sep}`), "evidence must be inside the worktree");
const wrapper = path.join(process.env.HOME, "Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh");
const env = { ...process.env, AB_SESSION: "tuic-chat-follow-proof", AB_KEEP_FOCUS: "0" };
const relative = path.relative(root, evidence).split(path.sep).join("/");
await mkdir(evidence, { recursive: true });
await writeFile(path.join(evidence, "index.html"), `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Chat follow proof</title></head><body><div id="root"></div><script type="module" src="/${relative}/proof.tsx"></script></body></html>`);
await writeFile(path.join(evidence, "proof.tsx"), `
import { render } from "solid-js/web";
import { Show } from "solid-js";
import { TerminalChatView, ViewModeToggle } from "/src/components/Terminal/TerminalChatView";
import { terminalsStore } from "/src/stores/terminals";
import "/src/global.css";
const ids = ["proof-one", "proof-two"].map(sessionId => {
  const id = terminalsStore.add({sessionId, fontSize:14, name:sessionId, cwd:"/fixture", awaitingInput:null, agentType:"claude", agentSessionId:sessionId});
  terminalsStore.setViewMode(id,"chat");
  return id;
});
render(() => <main style={{padding:"24px",height:"100vh",display:"grid","grid-template-columns":"1fr 1fr",gap:"24px"}}>
{ids.map((id,index) => <section data-testid={"proof-"+index} style={{display:"flex","flex-direction":"column","min-height":"0"}}>
<h1>Terminal {index+1}</h1><ViewModeToggle terminalId={id}/>
<Show when={terminalsStore.get(id)?.viewMode === "chat"} fallback={<p>CLI view</p>}>
<TerminalChatView terminalId={id} sessionId={index === 0 ? "proof-one" : "proof-two"}/>
</Show></section>)}</main>, document.getElementById("root")!);
`);
const server = await createServer({
	configFile: false,
	root,
	plugins: [solid()],
	define: { __APP_VERSION__: JSON.stringify("chat-follow-proof") },
	server: {
		host: "127.0.0.1", port: 0,
		proxy: Object.fromEntries(["/sessions", "/events", "/logs", "/proof"].map(prefix => [prefix, { target: backend }])),
	},
});
let opened = false;
let proofUrl;
let screenshotNumber = 0;
async function browser(...args) {
	const { stdout } = await run("perl", ["-e", "alarm 20; exec @ARGV", wrapper, ...args], { env, timeout: 22000, maxBuffer: 1024 * 1024 });
	return stdout;
}
async function capture(name) {
	const chrome = path.join(process.env.HOME, ".agent-browser/browsers/chrome-149.0.7827.54/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing");
	const output = await run(chrome, ["--headless=new", `--user-data-dir=${path.join(evidence, `headless-profile-${screenshotNumber++}`)}`,
		"--no-first-run", "--no-default-browser-check", "--disable-crash-reporter", "--window-size=1400,900",
		"--virtual-time-budget=3000", `--screenshot=${path.join(evidence, name)}`, proofUrl], { timeout: 20000, maxBuffer: 1024 * 1024 });
	await writeFile(path.join(evidence, `${name}.capture.log`), output.stdout + output.stderr);
	const artifact = await stat(path.join(evidence, name)).catch(() => null);
	assert(artifact && artifact.size > 0, `Headless Chrome returned without ${name}; see ${name}.capture.log`);
}
async function evaluate(code) {
	const result = JSON.parse(await browser("--", "--json", "eval", code));
	assert(result.success, JSON.stringify(result));
	return result.data.result;
}
async function waitFor(text, section = 0) {
	await browser("--", "wait", "--fn", `document.querySelector('[data-testid="proof-${section}"]')?.innerText.includes(${JSON.stringify(text)})`);
}
async function change(session, operation, text) {
	const result = await fetch(`${backend}/proof/${session}/${operation}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ text }) });
	assert(result.ok, `fixture ${operation} failed: ${result.status}`);
}
try {
	await server.listen();
	const address = server.httpServer.address();
	assert(address && typeof address !== "string");
	proofUrl = `http://127.0.0.1:${address.port}/${relative}/index.html`;
	await browser(proofUrl);
	opened = true;
	assert.equal(await evaluate("typeof navigator.webdriver"), "undefined");
	await waitFor("Initial transcript one");
	await waitFor("Initial transcript two", 1);
	await capture("before.png");
	await change("proof-one", "append", "Live append one");
	await change("proof-two", "append", "Live append two");
	await waitFor("Live append one");
	await waitFor("Live append two", 1);
	await capture("after-append.png");
	// Same-path atomic replacement must invalidate old history even if larger.
	await change("proof-one", "replace", "Replacement transcript with a deliberately longer first answer than the old transcript rows combined. ".repeat(4));
	await waitFor("Replacement transcript");
	assert.equal(await evaluate(`document.querySelector('[data-testid="proof-0"]').innerText.includes("Initial transcript one")`), false);
	await capture("after-replacement.png");
	// Switch through the real toggle and remount the real Chat component.
	await browser("--", "find", "nth", "0", '[data-testid="proof-0"] button', "click");
	await change("proof-one", "append", "Written while CLI is visible");
	await browser("--", "find", "nth", "1", '[data-testid="proof-0"] button', "click");
	await waitFor("Written while CLI is visible");
	await change("proof-one", "rebind", "Resumed into a new JSONL file");
	await waitFor("Resumed into a new JSONL file");
	assert.equal(await evaluate(`document.querySelector('[data-testid="proof-0"]').innerText.includes("Replacement transcript")`), false);
	await capture("after-rebind.png");
	const proof = { backend, captureMode: "isolated headless Chrome snapshots; live store assertions in wrapper tab", cases: ["initial history", "append to two terminals", "larger same-path replacement", "CLI-to-Chat remount", "new JSONL binding"], screenshots: ["before.png", "after-append.png", "after-replacement.png", "after-rebind.png"] };
	await writeFile(path.join(evidence, "proof.json"), JSON.stringify(proof, null, 2));
	process.stdout.write(`${JSON.stringify(proof)}\n`);
} finally {
	if (opened) await browser("--", "tab", "close").catch(error => process.stderr.write(`Could not close proof tab: ${error.message}\n`));
	await server.close();
}
