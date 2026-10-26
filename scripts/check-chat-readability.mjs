#!/usr/bin/env node
// Local macOS landing gate: the approved stealth wrapper is not available on Ubuntu CI.
// Starts only fixture Vite (no desktop/backend), runs real-browser regressions, traps shutdown.
import { spawn } from "node:child_process";
import { accessSync, constants, mkdirSync, mkdtempSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import solid from "vite-plugin-solid";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const wrapper = process.env.TUIC_CHAT_BROWSER_WRAPPER ?? join(homedir(), "Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh");
if (process.platform !== "darwin") throw new Error("check-chat-readability requires macOS and the approved mac-chrome stealth wrapper; run this local gate at landing");
accessSync(wrapper, constants.X_OK);
mkdirSync(join(root, ".tmp"), { recursive: true });
const evidence = process.env.TUIC_CHAT_EVIDENCE_DIR
	? resolve(process.env.TUIC_CHAT_EVIDENCE_DIR)
	: mkdtempSync(join(root, ".tmp/chat-readability-"));
const server = await createServer({
	root,
	configFile: false,
	plugins: [solid()],
	cacheDir: join(root, ".tmp/chat-readability-vite-cache"),
	logLevel: "error",
	server: { host: "127.0.0.1", port: 0, strictPort: true },
	define: { __APP_VERSION__: '"test"' },
});
let test;
let interrupted = false;
const stop = () => {
	interrupted = true;
	test?.kill("SIGTERM");
	void server.close();
};
process.once("SIGINT", stop);
process.once("SIGTERM", stop);
try {
	await server.listen();
	if (interrupted) throw new Error("Chat browser gate interrupted during Vite startup");
	const address = server.httpServer.address();
	if (!address || typeof address === "string") throw new Error("Fixture Vite has no TCP address");
	const url = `http://127.0.0.1:${address.port}`;
	console.log(`Chat browser gate: ${url}; evidence=${evidence}`);
	process.exitCode = await new Promise((resolveExit, reject) => {
		test = spawn("pnpm", ["exec", "vitest", "run", "src/__tests__/browser/ChatReadability.test.ts"], {
			cwd: root,
			stdio: "inherit",
			env: {
				...process.env,
				TUIC_CHAT_BROWSER_URL: url,
				TUIC_CHAT_EVIDENCE_DIR: evidence,
				TUIC_CHAT_BROWSER_WRAPPER: wrapper,
				AB_SESSION: `chat-readability-${process.pid}`,
			},
		});
		test.once("error", reject);
		test.once("exit", (code) => resolveExit(interrupted ? 130 : (code ?? 1)));
	});
} finally {
	await server.close();
	process.removeListener("SIGINT", stop);
	process.removeListener("SIGTERM", stop);
	console.log("Chat browser gate: fixture Vite stopped");
}
