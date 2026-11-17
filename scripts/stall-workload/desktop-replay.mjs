#!/usr/bin/env node
/** Replay in an already-running make agent-test instance. Never launches/restarts an app. */
import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { createServer, request } from "node:http";
import { resolve } from "node:path";

const [input, output, instance = "agent-test", port = "19876", socketPath] = process.argv.slice(2);
if (!input || !output || !/^[a-z0-9][a-z0-9-]*$/.test(instance) || Number(port) < 19000 || Number(port) > 65535)
	throw new Error(
		"usage: node scripts/stall-workload/desktop-replay.mjs RECORDING OUTPUT [agent-test-instance] [port>=19000] [instance-socket]",
	);
if (resolve(input) === resolve(output)) throw new Error("Output must not replace the recording");
const contents = readFileSync(input, "utf8");
const recording = JSON.parse(contents);
if (
	Buffer.byteLength(contents) > 6 * 1024 * 1024 ||
	recording.schema_version !== 1 ||
	!recording.events?.some((event) => event.kind === "paint")
)
	throw new Error("Unsupported recording");
const endpoint = `http://127.0.0.1:${Number(port)}`;
const hash = createHash("sha256").update(instance).digest("hex").slice(0, 16);
if (socketPath && !resolve(socketPath).endsWith(`/tuic-at/${hash.slice(0, 8)}/tuic-mcp-${hash}.sock`))
	throw new Error("Socket must belong to the named make agent-test instance");
const call = (path, options = {}) => {
	if (!socketPath)
		return fetch(`${endpoint}${path}`, { ...options, signal: AbortSignal.timeout(5000), redirect: "error" });
	return new Promise((resolve, reject) => {
		const req = request(
			{
				socketPath,
				path,
				method: options.method ?? "GET",
				headers: options.headers,
				signal: AbortSignal.timeout(5000),
			},
			(response) => {
				const chunks = [];
				response.on("data", (chunk) => chunks.push(chunk));
				response.on("error", reject);
				response.on("end", () =>
					resolve({
						ok: response.statusCode >= 200 && response.statusCode < 300,
						status: response.statusCode,
						json: async () => JSON.parse(Buffer.concat(chunks).toString()),
					}),
				);
			},
		);
		req.on("error", reject);
		req.end(options.body);
	});
};
const get = async (path) => {
	const response = await call(path);
	if (!response.ok) throw new Error(`agent-test ${path}: ${response.status}`);
	return response.json();
};
// Fail closed: verify the named harness socket AND the actual listener executable.
// A non-production port alone is not evidence that this is the isolated binary.
const health = await get("/health");
if (!health.ok || !String(health.socket_path).endsWith(`/tuic-at/${hash.slice(0, 8)}/tuic-mcp-${hash}.sock`))
	throw new Error("Target is not the named make agent-test instance");
const listeners = spawnSync("/usr/sbin/lsof", ["-nP", `-iTCP:${port}`, "-sTCP:LISTEN", "-Fp"], { encoding: "utf8" });
const pids = [
	...new Set(
		listeners.stdout
			.split("\n")
			.filter((line) => /^p\d+$/.test(line))
			.map((line) => line.slice(1)),
	),
];
if (listeners.status !== 0 || pids.length !== 1) throw new Error("Cannot identify one agent-test listener");
const processInfo = spawnSync("ps", ["-p", pids[0], "-o", "command="], { encoding: "utf8" });
if (
	processInfo.status !== 0 ||
	!new RegExp(`/tuic-agent-test/[0-9a-f]{8}/${instance}/tuicommander-agent-test$`).test(processInfo.stdout.trim())
)
	throw new Error("Listener executable is not tuicommander-agent-test");
let accept;
const done = new Promise((resolve) => {
	accept = resolve;
});
const nonce = randomUUID();
const server = createServer(async (req, res) => {
	res.setHeader("Access-Control-Allow-Origin", "*");
	if (req.url === `/${nonce}/recording`) {
		res.setHeader("Content-Type", "application/json");
		res.end(contents);
		return;
	}
	if (req.url === `/${nonce}/result` && req.method === "POST") {
		const chunks = [];
		let bytes = 0;
		for await (const chunk of req) {
			bytes += chunk.length;
			if (bytes > 2 * 1024 * 1024) {
				res.writeHead(413).end();
				return;
			}
			chunks.push(chunk);
		}
		try {
			const report = JSON.parse(Buffer.concat(chunks).toString());
			res.end("ok");
			accept(report);
		} catch {
			res.writeHead(400).end();
		}
		return;
	}
	res.writeHead(404).end();
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const url = `http://127.0.0.1:${server.address().port}/${nonce}`;
const backend = [];
let probing = false;
const timer = setInterval(async () => {
	if (probing) return;
	probing = true;
	const start = performance.now();
	const wall_ms = Date.now();
	try {
		await get("/sessions");
		backend.push({ wall_ms, latency_ms: performance.now() - start, ok: true });
	} catch (error) {
		backend.push({ wall_ms, latency_ms: performance.now() - start, ok: false, error: String(error) });
	} finally {
		probing = false;
	}
}, 100);
const script = `
const endpoint = ${JSON.stringify(url)};
const heartbeat = [], raf = [], ipc = [], canvases = new Map();
let timer, animation, last = performance.now(), lastRaf = last, pending = false;
try {
  if (!window.__TUIC__?.frameWorkload?.replay || !window.__TAURI__?.core?.invoke) throw new Error('Desktop replay API unavailable');
  await window.__TAURI__.core.invoke('push_log', {level:'info',source:'desktop_replay',message:JSON.stringify({stage:'start',visibility:document.visibilityState,userAgent:navigator.userAgent}),dataJson:null});
  const recording = await (await fetch(endpoint + '/recording')).json();
  await window.__TAURI__.core.invoke('push_log', {level:'info',source:'desktop_replay',message:JSON.stringify({stage:'recording-loaded',visibility:document.visibilityState}),dataJson:null});
  timer = setInterval(async () => {
    const now = performance.now(); heartbeat.push({wall_ms:Date.now(), gap_ms:now-last}); last=now;
    if (pending) return; pending=true; const start=performance.now();
    try { await window.__TAURI__.core.invoke('frontend_heartbeat'); ipc.push({wall_ms:Date.now(),latency_ms:performance.now()-start,ok:true}); }
    catch(error) { ipc.push({wall_ms:Date.now(),error:String(error),ok:false}); } finally {pending=false;}
  },100);
  const beat = now => {raf.push({wall_ms:Date.now(),gap_ms:now-lastRaf});lastRaf=now;animation=requestAnimationFrame(beat);};
  animation=requestAnimationFrame(beat);
  const reports=[];
  for(let repeat=0;repeat<3;repeat++) {
    const report=await window.__TUIC__.frameWorkload.replay(recording, session => {
      if(!canvases.has(session)) {const canvas=document.createElement('canvas');document.body.append(canvas);canvases.set(session,canvas);}return canvases.get(session);
    },true);
    reports.push(report);
  }
  await new Promise(resolve=>setTimeout(resolve,250));
  await fetch(endpoint+'/result',{method:'POST',body:JSON.stringify({reports,heartbeat,raf,ipc,userAgent:navigator.userAgent,visibility:document.visibilityState,claim:'WKWebView paint-only replay plus timer/native heartbeat round trips; no original PTY arrival replay or input-continuity proof'})});
} catch(error) {await fetch(endpoint+'/result',{method:'POST',body:JSON.stringify({error:String(error),heartbeat,raf,ipc})});}
finally {clearInterval(timer);cancelAnimationFrame(animation);for(const canvas of canvases.values())canvas.remove();}
`;
let outer;
let receipt;
let report;
try {
	const response = await call("/debug/invoke_js", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ script }),
		signal: AbortSignal.timeout(5000),
		redirect: "error",
	});
	receipt = await response.json();
	if (!response.ok || !receipt.ok) throw new Error(`invoke rejected: ${JSON.stringify(receipt)}`);
	report = await Promise.race([
		done,
		new Promise((_, reject) => {
			outer = setTimeout(
				() => reject(new Error("No WebView result within 180 seconds; acceptance is not completion")),
				180000,
			);
		}),
	]);
	writeFileSync(
		resolve(output),
		JSON.stringify(
			{
				...report,
				backend,
				instance,
				listener_pid: pids[0],
				transport: socketPath ? "unix-socket" : "tcp",
				recording_sha256: createHash("sha256").update(contents).digest("hex"),
			},
			null,
			2,
		),
	);
	if (report.error) throw new Error(report.error);
	console.log(`Saved native replay report: ${resolve(output)}`);
} catch (error) {
	let logs;
	try {
		logs = await get("/logs?limit=100");
	} catch (logError) {
		logs = { error: String(logError) };
	}
	writeFileSync(
		resolve(output),
		JSON.stringify(
			{
				...report,
				error: String(error),
				receipt,
				backend,
				logs,
				instance,
				listener_pid: pids[0],
				transport: socketPath ? "unix-socket" : "tcp",
				recording_sha256: createHash("sha256").update(contents).digest("hex"),
				claim: "Incomplete replay; eval acceptance and backend probes do not establish WebView liveness",
			},
			null,
			2,
		),
	);
	throw error;
} finally {
	clearTimeout(outer);
	clearInterval(timer);
	server.closeAllConnections();
	await new Promise((resolve) => server.close(resolve));
}
