import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Replace only OutputView's PTY subscription; render the actual component and CSS.
export default defineConfig({
	plugins: [
		{
			name: "recorded-dot-transport",
			enforce: "pre",
			resolveId(id, importer) {
				if (id === "../../transport" && importer?.endsWith("/mobile/components/OutputView.tsx"))
					return "\0dot-fixture-transport";
			},
			load(id) {
				if (id === "\0dot-fixture-transport")
					return `export async function subscribePty(_id, _data, _exit, options) {
    window.emitDotRows = options.onScreenRows;
    return Object.assign(() => {}, { pause() {}, resume() {} });
   }`;
			},
		},
		solid(),
	],
	server: { host: "127.0.0.1", port: 14352, strictPort: true },
});
