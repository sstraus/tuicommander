import { readdirSync, readFileSync } from "node:fs";
import { isAbsolute, join, relative, sep } from "node:path";
import { execSync } from "node:child_process";
import { defineConfig, type Plugin, type ViteDevServer } from "vite";
import solid from "vite-plugin-solid";
import checker from "vite-plugin-checker";
import { visualizer } from "rollup-plugin-visualizer";
import purgecss from "vite-plugin-purgecss";
import { Features } from "lightningcss";
import { backendBuildIdentity } from "./scripts/backend-build-identity.mjs";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// Only frontend inputs can affect Vite's output. A repository-local tooling
// checkout may contain any filename, including index.html. Ignore `.tmp/` at
// every depth so tooling output cannot reload the live WebView.
const ignoreNonFrontendFile = (path: string): boolean => {
  const file = relative(import.meta.dirname, path).split(sep).join("/");
  if (file === "") return false; // Keep the root itself watchable.
  if (file === ".." || file.startsWith("../") || isAbsolute(file)) return true;
  if (file.split("/").includes(".tmp")) return true;
  // Rust owns this grammar, but frontend imports must invalidate on edits too.
  const intentGrammar = "src-tauri/crates/tuic-terminal/src/intent_marker.json";
  if (file === intentGrammar || intentGrammar.startsWith(`${file}/`)) return false;
  if (file === "src" || file.startsWith("src/")) return false;
  if (file === "public" || file.startsWith("public/")) return false;
  if (!file.includes("/") && file.endsWith(".html")) return false;
  return !["vite.config.ts", "tsconfig.json", "package.json", "pnpm-lock.yaml"].includes(file);
};
// Read app version from tauri.conf.json
const tauriConf = JSON.parse(readFileSync(join(import.meta.dirname, "src-tauri/tauri.conf.json"), "utf-8"));

// Git hash for PWA version checks
const gitHash = (() => {
  try { return execSync("git rev-parse --short HEAD").toString().trim(); }
  catch { return "dev"; }
})();

// Identity endpoint: lets `scripts/dev-server.mjs` tell THIS checkout's dev
// server apart from another one squatting on port 1421 (a worktree, a stray
// process). Registered via `server.middlewares.use` so it runs before Vite's
// internal SPA fallback, which would otherwise answer with index.html.
const devServerIdentity = (): Plugin => ({
  name: "tuic-dev-identity",
  apply: "serve",
  configureServer(server: ViteDevServer) {
    server.middlewares.use("/__tuic_backend_identity", (req, res) => {
      try {
        const query = new URL(req.url ?? "/", "http://localhost").searchParams.get("backend");
        const identity = backendBuildIdentity(server.config.root, query ? JSON.parse(query) : undefined);
        res.setHeader("content-type", "application/json");
        res.setHeader("cache-control", "no-store");
        res.end(JSON.stringify(identity));
      } catch (error) {
        server.config.logger.error(`Backend identity failed: ${error}`);
        res.setHeader("content-type", "application/json");
        res.end(JSON.stringify({ status: "unavailable", key: "unavailable" }));
      }
    });
    server.middlewares.use("/__tuic_dev_root", (_req, res) => {
      res.setHeader("content-type", "text/plain");
      res.end(server.config.root);
    });
  },
});

// Theme gallery (`theme-gallery.html`): the bundled theme JSONs, read fresh on
// every request. An `import.meta.glob` would be cached by the module graph and
// never invalidated, because `server.watch.ignored` excludes `src-tauri`; a
// theme edit would then stay invisible until a dev-server restart.
const themeSources = (): Plugin => ({
  name: "tuic-theme-sources",
  apply: "serve",
  configureServer(server: ViteDevServer) {
    server.middlewares.use("/__tuic_themes", (_req, res) => {
      const dir = join(server.config.root, "src-tauri/src/themes");
      const themes = readdirSync(dir)
        .filter((f) => f.endsWith(".json"))
        .sort()
        .map((f) => ({ key: f.slice(0, -".json".length), ...JSON.parse(readFileSync(join(dir, f), "utf-8")) }));
      res.setHeader("content-type", "application/json");
      res.setHeader("cache-control", "no-store");
      res.end(JSON.stringify(themes));
    });
  },
});

// https://vite.dev/config/
export default defineConfig(async ({ command }) => ({
  define: {
    __APP_VERSION__: JSON.stringify(tauriConf.version),
    __BUILD_GIT_HASH__: JSON.stringify(gitHash),
  },
  plugins: [
    solid(),
    // The type-check overlay is only useful in the dev server (`vite` / `tauri
    // dev`). One-shot builds run `tsc` up front (`pnpm build` = `tsc && vite
    // build`, and `make check` runs it too), so keeping the checker in build
    // mode just type-checks twice — skip it. Speeds up make dev/preview/build.
    ...(command === "serve"
      ? [
          devServerIdentity(),
          themeSources(),
          checker({
            // vite-plugin-checker 0.14.5+ detects TypeScript 7 and runs its
            // native CLI because TS7 no longer exposes the JavaScript compiler API.
            typescript: true,
          }),
        ]
      : []),
    visualizer({ filename: "dist/bundle-stats.html", gzipSize: true }),
    purgecss({
      // Do NOT pass `content` — the plugin auto-scans the bundled JS output.
      // A user-supplied `content` overrides the auto-scan (via ...options spread),
      // which causes PurgeCSS to scan raw source files where CSS-module hashed
      // class names (e.g. _3see_q_popover) don't exist, silently purging them.
      safelist: [
        // xterm.js classes (generated at runtime by the library)
        /^xterm/,
        // CodeMirror 6 classes (generated at runtime by the library)
        /^cm-/,
        /^ͼ/,
        // Dynamic classList patterns used via SolidJS classList={{}}
        /^platform-/,
      ],
    }),
  ],

  // Lightning CSS: minify only, no vendor prefixes or syntax lowering.
  // Tauri webviews (WKWebView, WebView2, WebKitGTK) are all modern engines.
  css: {
    transformer: "lightningcss",
    lightningcss: {
      include: Features.Nesting,
      exclude: Features.VendorPrefixes,
    },
  },
  resolve: {
    dedupe: [
      "@codemirror/state",
      "@codemirror/view",
      "@codemirror/language",
      "@codemirror/language-data",
      "@codemirror/commands",
      "@codemirror/search",
      "@lezer/common",
      "@lezer/highlight",
    ],
  },
  optimizeDeps: {
    include: [
      "@codemirror/state",
      "@codemirror/view",
      "@codemirror/language",
      "@codemirror/language-data",
    ],
  },
  build: {
    target: "esnext",
    cssMinify: "lightningcss",
    rolldownOptions: {
      input: {
        main: "index.html",
        mobile: "mobile.html",
      },
    },
  },


  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1421,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. watch only frontend inputs. Mutation tooling clones a full checkout
      //    into `.tmp/`; its index.html files caused live WebView reloads.
      //    This also excludes other tooling, docs, builds and worktree trees.
      ignored: ignoreNonFrontendFile,
    },
  },
}));
