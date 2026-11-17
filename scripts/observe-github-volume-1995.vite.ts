import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { readFileSync, appendFileSync } from "node:fs";
// The driver (observe-github-volume-1995.py) names the trace file, under its scratch root.
const tracePath = process.env.TUIC_1995_TRACE;
if (!tracePath) throw new Error("TUIC_1995_TRACE must name the frontend trace file; run via observe-github-volume-1995.py");
const roots = [
	...new Set(
		readFileSync("src-tauri/src/mcp_http/command_table_paths.txt", "utf8")
			.split("\n")
			.filter((s) => s.startsWith("/"))
			.map((s) => s.split("/")[1]),
	),
];
const proxy = Object.fromEntries(
	[...roots, "events", "health", "logs", "ws"].map((root) => [
		"^/" + root + "(?:/|\\?|$)",
		{ target: "http://127.0.0.1:14360", ws: true, changeOrigin: false },
	]),
);
export default defineConfig({
	define: {
		__APP_VERSION__: JSON.stringify("1995-observation"),
		__BUILD_GIT_HASH__: JSON.stringify("641348de7-with-live-harness"),
	},
	plugins: [
		solid(),
		{
			name: "observe-browser-api",
			configureServer(server) {
				server.middlewares.use((req, _res, next) => {
					if (roots.some((root) => req.url?.startsWith("/" + root)))
						appendFileSync(
							tracePath,
							JSON.stringify({ at: new Date().toISOString(), method: req.method, url: req.url }) + "\n",
						);
					next();
				});
			},
		},
	],
	server: { host: "127.0.0.1", port: 14361, strictPort: true, proxy },
	resolve: { dedupe: ["solid-js"] },
});
