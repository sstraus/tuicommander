import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import test from "node:test";

const script = new URL("./github-work-desktop-e2e.mjs", import.meta.url);

// Catches: the desktop E2E contacts a daily/external socket or runs without isolated-instance consent.
test("registered-provider E2E rejects unsafe sockets and missing consent before desktop access", () => {
	for (const args of [
		["/tmp/tuic-mcp.sock", "sstraus", "1", "2", "--isolated-desktop"],
		[`${homedir()}/Gits/.tmp/tuic-at/../tuic-mcp.sock`, "sstraus", "1", "2", "--isolated-desktop"],
		[`${homedir()}/Gits/.tmp/tuic-at/scope/app.sock`, "sstraus", "1", "2"],
		[`${homedir()}/Gits/.tmp/tuic-at/scope/app.sock`, "sstraus", "0", "2", "--isolated-desktop"],
	]) {
		const result = spawnSync(process.execPath, [script.pathname, ...args], { encoding: "utf8", timeout: 2000 });
		assert.equal(result.status, 2);
		assert.equal(result.stdout, "");
		assert.match(result.stderr, /^Usage:/);
	}
});
