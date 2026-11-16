import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import test from "node:test";

const script = new URL("./github-work-desktop-e2e.mjs", import.meta.url);
// The socket layout make agent-test hands out (scripts/agent-test.py).
const agentTestSocket = `${tmpdir()}/tuic-at/0123abcd/tuic-mcp-0123abcd89abcdef.sock`;

// Catches: the desktop E2E contacts a daily/external socket or runs without isolated-instance consent.
test("registered-provider E2E rejects unsafe sockets and missing consent before desktop access", () => {
	for (const args of [
		["/tmp/tuic-mcp.sock", "sstraus", "1", "2", "--isolated-desktop"],
		[`${tmpdir()}/tuic-at/../tuic-mcp.sock`, "sstraus", "1", "2", "--isolated-desktop"],
		[`${tmpdir()}/tuic-at/scope/app.sock`, "sstraus", "1", "2", "--isolated-desktop"],
		[`${tmpdir()}/tuic-at/0123abcd/tuic-mcp-fedcba9876543210.sock`, "sstraus", "1", "2", "--isolated-desktop"],
		[agentTestSocket, "sstraus", "1", "2"],
		[agentTestSocket, "sstraus", "0", "2", "--isolated-desktop"],
	]) {
		const result = spawnSync(process.execPath, [script.pathname, ...args], { encoding: "utf8", timeout: 2000 });
		assert.equal(result.status, 2);
		assert.equal(result.stdout, "");
		assert.match(result.stderr, /^Usage:/);
	}
});
