import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { test } from "node:test";
import { runInNewContext } from "node:vm";

// Exercise the executable's final persisted output, without an app, socket or real timer.
const source = readFileSync(new URL("./desktop-replay.mjs", import.meta.url), "utf8")
	.replace(/^#!.*\n/, "")
	.replace(/^import .*;\n/gm, "");
const measurements = {
	error: "paint failed after native heartbeat",
	heartbeat: [{ gap_ms: 31000 }],
	raf: [{ gap_ms: 32000 }],
	ipc: [{ latency_ms: 7, ok: true }],
};

async function failureReport(callback) {
	let handler;
	let saved;
	let closed = false;
	const hash = createHash("sha256").update("agent-test").digest("hex").slice(0, 16);
	const reply = (value) => ({ ok: true, json: async () => value });
	const context = {
		Buffer,
		AbortSignal,
		URL,
		createHash,
		resolve,
		performance,
		process: { argv: ["node", "desktop-replay.mjs", "recording.json", "report.json"] },
		randomUUID: () => "nonce",
		readFileSync: () => JSON.stringify({ schema_version: 1, events: [{ kind: "paint" }] }),
		writeFileSync: (_path, value) => { saved = JSON.parse(value); },
		spawnSync: (command) => command.endsWith("lsof")
			? { status: 0, stdout: "p123\n" }
			: { status: 0, stdout: "/var/tmp-x/tuic-agent-test/0123abcd/agent-test/tuicommander-agent-test" },
		createServer: (listener) => {
			handler = listener;
			return {
				listen: (_port, _host, ready) => ready(),
				address: () => ({ port: 20000 }),
				closeAllConnections() {},
				close: (done) => { closed = true; done(); },
			};
		},
		fetch: async (url) => {
			if (url.endsWith("/health")) return reply({ ok: true, socket_path: `/tuic-at/${hash.slice(0, 8)}/tuic-mcp-${hash}.sock` });
			if (url.includes("/logs?")) return reply([{ message: "diagnostic log" }]);
			assert.ok(url.endsWith("/debug/invoke_js"));
			if (callback) {
				await handler({
					url: "/nonce/result", method: "POST",
					async *[Symbol.asyncIterator]() { yield Buffer.from(JSON.stringify(callback)); },
				}, { setHeader() {}, end() {} });
			}
			return reply({ ok: true });
		},
		setInterval: () => 1,
		clearInterval() {},
		setTimeout: (expire) => { if (!callback) queueMicrotask(expire); return 2; },
		clearTimeout() {},
	};
	await assert.rejects(runInNewContext(`(async () => {${source}\n})()`, context),
		callback ? /paint failed/ : /No WebView result within 180 seconds/);
	assert.equal(closed, true, "failure must close the callback server");
	assert.equal(saved.receipt.ok, true);
	assert.equal(saved.logs[0].message, "diagnostic log");
	return saved;
}

test("callback-error enrichment must not discard heartbeat, rAF or native-command measurements", async () => {
	const saved = await failureReport(measurements);
	for (const field of ["heartbeat", "raf", "ipc"]) assert.deepEqual(saved[field], measurements[field]);
	assert.match(saved.error, /paint failed/);
});

test("timeout without a callback must still persist its error", async () => {
	const saved = await failureReport();
	assert.match(saved.error, /No WebView result within 180 seconds/);
	assert.equal(saved.heartbeat, undefined);
});
