#!/usr/bin/env node
/** One bounded run; owns and cleans up only the processes and PTYs it creates. */
import { spawn, spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { createWriteStream, existsSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import solid from "vite-plugin-solid";

const root = fileURLToPath(new URL("../..", import.meta.url));
process.chdir(root);
const evidence = resolve(process.argv[2] ?? resolve(tmpdir(), `stall-workload-${Date.now()}`));
if (existsSync(evidence)) throw new Error(`Evidence directory already exists: ${evidence}`);
mkdirSync(evidence, { recursive: true });
const home = resolve(evidence, "home");
mkdirSync(home);
mkdirSync(resolve(evidence, "tmp"));
// Revision exports live beside the evidence (outside the checkout); bare imports in them
// resolve through a node_modules link, and Vite is allowed to serve the directory below.
const snapshots = resolve(evidence, "revisions");
mkdirSync(snapshots, { recursive: true });
symlinkSync(resolve(root, "node_modules"), resolve(snapshots, "node_modules"), "dir");
const git = (...args) => {
	const result = spawnSync("git", args, { encoding: "utf8" });
	if (result.status !== 0) throw new Error(result.stderr);
	return result.stdout.trim();
};
const refs = {
	main: git("rev-parse", process.env.STALL_MAIN ?? "main"),
	"pre-fix": git("rev-parse", process.env.STALL_PRE_FIX ?? "950715de69"),
};
for (const [label, ref] of Object.entries(refs)) {
	const dest = resolve(snapshots, label);
	mkdirSync(dest, { recursive: true });
	const jsonInputs = git("ls-tree", "-r", "--name-only", ref, "src-tauri")
		.split("\n")
		.filter((path) => path.endsWith(".json") && !path.includes("fixtures/"));
	const archive = spawnSync("git", ["archive", ref, "src", ...jsonInputs], { maxBuffer: 100 * 1024 * 1024 });
	if (archive.status !== 0) throw new Error(String(archive.stderr));
	const unpack = spawnSync("tar", ["-xf", "-", "-C", dest], { input: archive.stdout });
	if (unpack.status !== 0) throw new Error(String(unpack.stderr));
}
const seconds = Number(process.env.STALL_SECONDS ?? 15);
const sessionsCount = Number(process.env.STALL_SESSIONS ?? 4);
const visible = Number(process.env.STALL_VISIBLE ?? 1);
if (
	!(
		seconds >= 1 &&
		seconds <= 30 &&
		sessionsCount >= 1 &&
		sessionsCount <= 4 &&
		visible >= 1 &&
		visible <= sessionsCount
	)
)
	throw new Error("Use 1-30 seconds, 1-4 sessions, and 1..sessions visible");
const backendPort = Number(process.env.STALL_BACKEND_PORT ?? 19838);
const vitePort = Number(process.env.STALL_VITE_PORT ?? 14838);
if (
	![backendPort, vitePort].every((port) => Number.isInteger(port) && port >= 19000 && port <= 65535) &&
	!(backendPort === 19838 && vitePort === 14838)
)
	throw new Error("Use isolated ports >=19000 (or harness defaults)");
const base = `http://127.0.0.1:${backendPort}`;
const token = randomUUID();
const wrapper = "/Users/stefano.straus/Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh";
const session = `stall-1638-${Date.now()}`;
const browserEnv = { ...process.env, AB_SESSION: session, AB_AGENT_BROWSER_DIR: resolve(evidence, "reap-scope") };
mkdirSync(browserEnv.AB_AGENT_BROWSER_DIR);
let browserDaemon;
let backend;
let vite;
let interval;
let phase = "setup";
let recording;
let resolveReport;
const completed = new Promise((resolve) => {
	resolveReport = resolve;
});
const cancellation = new AbortController();
const sessions = [];
const backendSamples = [];
const api = async (path, method = "GET", body, cleanup = false) => {
	const url = new URL(path, base);
	url.searchParams.set("token", token);
	const response = await fetch(url, {
		method,
		headers: body === undefined ? {} : { "Content-Type": "application/json" },
		body: body === undefined ? undefined : JSON.stringify(body),
		signal: cleanup
			? AbortSignal.timeout(10000)
			: AbortSignal.any([cancellation.signal, AbortSignal.timeout(10000)]),
	});
	const text = await response.text();
	if (!response.ok) throw new Error(`${method} ${path}: ${response.status} ${text}`);
	return text ? JSON.parse(text) : null;
};
const browser = (args, cleanup = false) =>
	new Promise((resolve, reject) => {
		if (!cleanup) cancellation.signal.throwIfAborted();
		const child = spawn(wrapper, args, { env: browserEnv });
		const timeout = setTimeout(() => child.kill("SIGTERM"), 45000);
		// Let an in-flight attach publish its named-session identity before teardown.
		// Killing the wrapper here could strand a tab before browserDaemon is known.
		let output = "";
		child.stdout.on("data", (data) => {
			output += data;
		});
		child.stderr.on("data", (data) => {
			output += data;
		});
		child.on("error", reject);
		child.on("close", (code) => {
			clearTimeout(timeout);
			writeFileSync(
				resolvePath("browser-transcript.txt"),
				`${readIfPresent("browser-transcript.txt")}${args.join(" ")}\n${output}\nexit=${code}\n`,
			);
			const match = output.match(/\[ab-stealth\] session=.*? Chrome=(\d+)/);
			if (match) browserDaemon = `ab-shared-${session}-${match[1]}`;
			if (!cleanup && cancellation.signal.aborted) reject(cancellation.signal.reason);
			else code === 0 ? resolve(output) : reject(new Error(`Browser command exit ${code}: ${output}`));
		});
	});
function resolvePath(name) {
	return resolve(evidence, name);
}
function readIfPresent(name) {
	return existsSync(resolvePath(name)) ? readFileSync(resolvePath(name), "utf8") : "";
}
async function body(req) {
	const chunks = [];
	let bytes = 0;
	for await (const chunk of req) {
		bytes += chunk.length;
		if (bytes > 7 * 1024 * 1024) throw new Error("request body too large");
		chunks.push(chunk);
	}
	return chunks.length ? JSON.parse(Buffer.concat(chunks).toString()) : {};
}
function json(res, value) {
	res.setHeader("Content-Type", "application/json");
	res.end(JSON.stringify(value));
}
// Signal handlers request cancellation; only the existing finally block owns teardown.
// Keep handlers installed through teardown so a second signal cannot interrupt it.
const cancel = (signal) => {
	if (cancellation.signal.aborted) return;
	process.exitCode = signal === "SIGINT" ? 130 : 143;
	const error = new Error(`Workload cancelled by ${signal}`);
	cancellation.abort(error);
	resolveReport({ error: error.message });
};
process.on("SIGINT", cancel);
process.on("SIGTERM", cancel);
try {
	const binary = resolve(process.env.STALL_BACKEND_BIN ?? "src-tauri/target/debug/tuic-remote");
	const log = createWriteStream(resolvePath("backend.log"));
	backend = spawn(binary, ["--instance", "stall-workload", "--bind", "127.0.0.1", "--no-agent-configs"], {
		env: {
			...process.env,
			HOME: home,
			TMPDIR: resolve(evidence, "tmp"),
			XDG_CONFIG_HOME: resolve(home, ".config"),
			TUIC_PORT: String(backendPort),
			TUIC_PAIRING_TOKEN: token,
			TUIC_CAPTURE_DIR: resolve(evidence, "captures"),
			STALL_SECONDS: String(seconds),
			STALL_COLUMNS: "220",
			STALL_LINES_PER_TICK: "12",
		},
		stdio: ["ignore", "pipe", "pipe"],
	});
	backend.stdout.pipe(log);
	backend.stderr.pipe(log);
	backend.on("error", (error) => resolveReport({ error: String(error) }));
	// Startup retry is a prerequisite, not a timing assertion about liveness.
	for (let attempt = 0; ; attempt++) {
		try {
			await api("/health");
			break;
		} catch (error) {
			if (cancellation.signal.aborted || attempt >= 100 || backend.exitCode !== null) throw error;
			await new Promise((resolve) => setTimeout(resolve, 100));
		}
	}
	await api("/diagnostics/capture", "POST", { enabled: true });
	vite = await createServer({
		configFile: false,
		root,
		cacheDir: resolve(evidence, "vite-cache"),
		plugins: [
			solid(),
			{
				name: "isolated-stall-harness",
				configureServer(server) {
					server.middlewares.use(async (req, res, next) => {
						if (!req.url?.startsWith("/__stall/") && req.url !== "/debug/frame-workload") return next();
						try {
							if (req.url === "/__stall/config") return json(res, { seconds, visible, snapshots });
							if (req.url === "/__stall/phase") {
								phase = (await body(req)).phase;
								return json(res, {});
							}
							if (req.url === "/__stall/sessions") {
								for (let i = 0; i < sessionsCount; i++) {
									const result = await api("/sessions", "POST", {
										rows: 70,
										cols: 220,
										shell: `/usr/bin/python3 ${resolve(root, "scripts/stall-workload/output.py")}`,
										cwd: home,
									});
									sessions.push(result.session_id);
								}
								return json(res, { sessions });
							}
							if (req.url === "/__stall/delete-sessions") {
								for (const id of sessions.splice(0)) await api(`/sessions/${id}`, "DELETE");
								return json(res, {});
							}
							if (req.url === "/debug/frame-workload") {
								const input = await body(req);
								recording = JSON.parse(input.recording);
								writeFileSync(resolvePath("frame-workload.json"), input.recording);
								// The headless remote router does not publish this desktop debug route.
								// Persist evidence in this harness, without claiming backend writer coverage.
								return json(res, resolvePath("frame-workload.json"));
							}
							if (req.url === "/__stall/recording") return json(res, recording);
							if (req.url === "/__stall/report") {
								const report = await body(req);
								json(res, {});
								resolveReport(report);
								return;
							}
							res.statusCode = 404;
							json(res, { error: "unknown harness route" });
						} catch (error) {
							res.statusCode = 500;
							json(res, { error: String(error) });
						}
					});
				},
			},
		],
		define: {
			__APP_VERSION__: JSON.stringify("isolated-stall-harness"),
			__BUILD_GIT_HASH__: JSON.stringify(git("rev-parse", "HEAD")),
		},
		server: {
			host: "127.0.0.1",
			port: vitePort,
			strictPort: true,
			fs: { allow: [root, snapshots] },
			proxy: {
				"/sessions": {
					target: base,
					ws: true,
					configure(proxy) {
						proxy.on("proxyReq", (request) => request.setHeader("Cookie", `tui-session=${token}`));
						proxy.on("proxyReqWs", (request) => request.setHeader("Cookie", `tui-session=${token}`));
					},
				},
				"/events": { target: base, headers: { Cookie: `tui-session=${token}` } },
				"/logs": { target: base, headers: { Cookie: `tui-session=${token}` } },
				"/config": { target: base, headers: { Cookie: `tui-session=${token}` } },
			},
		},
	});
	cancellation.signal.throwIfAborted();
	await vite.listen();
	cancellation.signal.throwIfAborted();
	let probing = false;
	interval = setInterval(async () => {
		if (probing) return;
		probing = true;
		const start = performance.now();
		const wall_ms = Date.now();
		const atPhase = phase;
		try {
			await api("/sessions");
			backendSamples.push({ phase: atPhase, wall_ms, latency_ms: performance.now() - start, ok: true });
		} catch (error) {
			backendSamples.push({
				phase: atPhase,
				wall_ms,
				latency_ms: performance.now() - start,
				ok: false,
				error: String(error),
			});
		} finally {
			probing = false;
		}
	}, 100);
	await browser([`http://127.0.0.1:${vitePort}/scripts/stall-workload/page.html`]);
	let outer;
	const report = await Promise.race([
		completed,
		new Promise((_, reject) => {
			outer = setTimeout(
				() => reject(new Error("Outer harness exceeded 180 seconds; inspect backend/browser logs")),
				180000,
			);
		}),
	]).finally(() => clearTimeout(outer));
	writeFileSync(resolvePath("report.json"), JSON.stringify({ ...report, backend: backendSamples }, null, 2));
	const provenance = {
		schema_version: 1,
		generated: true,
		historical_stall: false,
		refs,
		harness_head: git("rev-parse", "HEAD"),
		seconds,
		sessions: sessionsCount,
		visible,
		requested_geometry: { rows: 70, columns: 220 },
		harness_sha256: Object.fromEntries(
			["run.mjs", "page.tsx", "output.py"].map((name) => [
				name,
				createHash("sha256")
					.update(readFileSync(`scripts/stall-workload/${name}`))
					.digest("hex"),
			]),
		),
		writer_sha256: createHash("sha256").update(readFileSync("scripts/stall-workload/output.py")).digest("hex"),
		recording_sha256: recording
			? createHash("sha256")
					.update(readFileSync(resolvePath("frame-workload.json")))
					.digest("hex")
			: null,
		source:
			"Paced synthetic Python output captured through real isolated PTYs and production CanvasTerminal; not output from an AI agent or a recorded historical stall",
		comparison:
			"Exact git-exported renderers/projections only, using identical captured rows; headless backend and capture frontend stay at harness revision",
		desktop_limit:
			"Chrome timer/rAF sentinel is not WKWebView frontend_heartbeat, Tauri IPC, or independent input continuity",
		started_backend_pid: backend.pid,
	};
	writeFileSync(resolvePath("provenance.json"), JSON.stringify(provenance, null, 2));
	if (report.error) throw new Error(report.error);
	console.log(`Saved workload and distributions: ${evidence}`);
} catch (error) {
	if (!cancellation.signal.aborted) throw error;
	console.error(cancellation.signal.reason.message);
} finally {
	clearInterval(interval);
	for (const id of sessions) {
		try {
			await api(`/sessions/${id}`, "DELETE", undefined, true);
		} catch {}
	}
	if (browserDaemon) {
		try {
			await browser(["--", "tab", "close"], true);
		} catch (error) {
			console.error(`Tab cleanup: ${error}`);
		}
		spawnSync("agent-browser", ["--session", browserDaemon, "close"], { timeout: 10000 });
	}
	if (vite) await vite.close();
	if (backend && backend.exitCode === null) {
		backend.kill("SIGTERM");
		await new Promise((resolve) => {
			const force = setTimeout(() => backend.kill("SIGKILL"), 5000);
			backend.once("close", () => {
				clearTimeout(force);
				resolve();
			});
		});
	}
	rmSync(snapshots, { recursive: true, force: true });
	rmSync(resolve(evidence, "vite-cache"), { recursive: true, force: true });
	console.log("Owned PTYs, Vite, headless backend and named browser session cleaned up.");
	process.removeListener("SIGINT", cancel);
	process.removeListener("SIGTERM", cancel);
}
