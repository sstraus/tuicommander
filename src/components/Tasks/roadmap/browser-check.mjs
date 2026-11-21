import { spawn } from "node:child_process";
import { mkdirSync, openSync } from "node:fs";
import { createServer } from "node:net";
import { once } from "node:events";
import { resolve } from "node:path";

const playwrightDir = process.env.PLAYWRIGHT_DIR;
if (!playwrightDir) throw new Error("Set PLAYWRIGHT_DIR to a directory with node_modules/playwright-core installed (browsers default to $PLAYWRIGHT_DIR/browsers); there is no default location.");
const out = resolve("reports/visual/tasks-roadmap");
mkdirSync(out, { recursive: true });
const probe = createServer();
probe.listen(0, "127.0.0.1");
await once(probe, "listening");
const address = probe.address();
if (!address || typeof address === "string") throw new Error("No ephemeral port");
const port = address.port;
await new Promise((done) => probe.close(done));
const log = openSync(`${out}/vite.txt`, "w");
const vite = spawn(process.execPath, ["node_modules/vite/bin/vite.js", "--config", "src/components/Tasks/roadmap/vite.config.ts", "--host", "127.0.0.1", "--port", String(port), "--strictPort"], { stdio: ["ignore", log, log] });
try {
	const base = `http://127.0.0.1:${port}`;
	const deadline = Date.now() + 120000;
	while (true) {
		if (vite.exitCode !== null) throw new Error(`Vite exited ${vite.exitCode}; see vite.txt`);
		try { if ((await fetch(base)).ok) break; } catch { /* Server setup is still in progress. */ }
		if (Date.now() > deadline) throw new Error("Vite did not start within setup budget; see vite.txt");
		await new Promise((done) => setTimeout(done, 100));
	}
	const capture = spawn(process.execPath, ["src/components/Tasks/roadmap/capture.mjs", base, out, playwrightDir], { stdio: "inherit", env: { ...process.env, PLAYWRIGHT_BROWSERS_PATH: process.env.PLAYWRIGHT_BROWSERS_PATH ?? `${playwrightDir}/browsers`, LD_LIBRARY_PATH: `${playwrightDir}/libs/usr/lib/aarch64-linux-gnu:${process.env.LD_LIBRARY_PATH ?? ""}` } });
	const [code] = await once(capture, "exit");
	if (code !== 0) throw new Error(`Roadmap browser check failed with exit ${code}`);
} finally {
	vite.kill("SIGTERM");
	if (vite.exitCode === null) await once(vite, "exit");
}
