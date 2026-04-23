import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import checker from "vite-plugin-checker";
import { visualizer } from "rollup-plugin-visualizer";
import purgecss from "vite-plugin-purgecss";
import { Features } from "lightningcss";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// Read app version from tauri.conf.json
const tauriConf = JSON.parse(readFileSync("./src-tauri/tauri.conf.json", "utf-8"));

// Git hash for PWA version checks
const gitHash = (() => {
  try { return execSync("git rev-parse --short HEAD").toString().trim(); }
  catch { return "dev"; }
})();

// https://vite.dev/config/
export default defineConfig(async () => ({
  define: {
    __APP_VERSION__: JSON.stringify(tauriConf.version),
    __BUILD_GIT_HASH__: JSON.stringify(gitHash),
  },
  plugins: [
    solid(),
    checker({ typescript: true }),
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
        /^split-/,
        /^awaiting-/,
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
    rollupOptions: {
      input: {
        main: "index.html",
        mobile: "mobile.html",
      },
      output: {
        manualChunks: {
          xterm: ["@xterm/xterm", "@xterm/addon-fit", "@xterm/addon-webgl", "@xterm/addon-unicode11"],
          codemirror: ["@codemirror/state", "@codemirror/view", "@codemirror/language", "@codemirror/language-data", "@codemirror/commands", "@codemirror/search", "@lezer/common", "@lezer/highlight"],
          "diff-view": ["@git-diff-view/core", "@git-diff-view/solid"],
          markdown: ["marked", "dompurify"],
        },
      },
    },
  },


  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
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
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
