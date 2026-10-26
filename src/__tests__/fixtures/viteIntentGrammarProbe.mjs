import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer, loadConfigFromFile, transformWithOxc } from "vite";

const repo = fileURLToPath(new URL("../../../", import.meta.url));
const grammarPath = "src-tauri/crates/tuic-terminal/src/intent_marker.json";
const scratchRoot = join(repo, ".tmp");
mkdirSync(scratchRoot, { recursive: true });
const scratch = mkdtempSync(join(scratchRoot, "vite-intent-"));
let server;
try {
	const loaded = await loadConfigFromFile({ command: "serve", mode: "development" }, join(repo, "vite.config.ts"));
	const ignored = loaded.config.server.watch.ignored;
	const grammar = JSON.parse(readFileSync(join(repo, grammarPath), "utf8"));
	const target = join(scratch, grammarPath);
	mkdirSync(dirname(target), { recursive: true });
	// Record the exact pre-2060 grammar shape cached by the live dev server.
	writeFileSync(target, JSON.stringify({ anchor: grammar.anchor }));
	const watched = Promise.withResolvers();
	const updated = Promise.withResolvers();
	server = await createServer({
		configFile: false,
		logLevel: "silent",
		root: scratch,
		plugins: [{
			name: "intent-grammar-probe",
			configureServer(server) {
				// Vite adds several watch batches. On Linux the first `ready` can
				// precede recursive discovery of the grammar; wait for its own add.
				const onAdd = (path) => {
					if (path !== target) return;
					server.watcher.off("add", onAdd);
					watched.resolve();
				};
				server.watcher.on("add", onAdd);
			},
			hotUpdate({ type, file }) {
				if (type === "update" && file === target) updated.resolve();
			},
		}],
		server: {
			port: 0,
			ws: { port: 0 },
			watch: { ignoreInitial: false, ignored: (path) => ignored(join(repo, relative(scratch, path))) },
		},
		optimizeDeps: { noDiscovery: true },
	});
	await server.listen();
	await watched.promise;
	const url = `/${grammarPath}?import`;
	const origin = `http://localhost:${server.httpServer.address().port}`;
	const before = { code: await (await fetch(origin + url)).text() };
	assert.ok(!before.code.includes("export const title"));
	writeFileSync(target, JSON.stringify(grammar));
	// Wait for Vite to invalidate the module graph, not an arbitrary quiet period.
	await updated.promise;
	const after = { code: await (await fetch(origin + url)).text() };
	assert.ok(after.code.includes("export const title"), "grammar update must invalidate Vite\'s cached JSON module");
	const source = readFileSync(join(repo, "src/components/Terminal/suggestOverlay.ts"), "utf8");
	const transformed = await transformWithOxc(source, "suggestOverlay.ts");
	const jsonModule = `data:text/javascript;base64,${Buffer.from(after.code).toString("base64")}`;
	const code = transformed.code.replace(/from\s+["'][^"']*intent_marker\.json["']/, `from ${JSON.stringify(jsonModule)}`);
	const { planSuggestOverlay } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
	const fixture = JSON.parse(readFileSync(join(repo, "src/components/Terminal/__tests__/fixtures/ego-answer-2073.json"), "utf8"));
	const plan = planSuggestOverlay(fixture.rows.length, (i) => fixture.rows[i] ?? null);
	process.stdout.write(JSON.stringify({ blocks: plan.blocks, answerMarkers: [...plan.answerMarkers] }));
} catch (error) {
	process.stderr.write(`${error.stack?.replace(/data:text\/javascript;base64,[A-Za-z0-9+/=]+/g, "<probe module>")}\n`);
	process.exitCode = 1;
} finally {
	await server?.close();
	rmSync(scratch, { recursive: true, force: true });
}
