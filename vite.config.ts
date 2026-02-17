import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import checker from "vite-plugin-checker";
import { visualizer } from "rollup-plugin-visualizer";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    solid(),
    checker({ typescript: true }),
    visualizer({ filename: "dist/bundle-stats.html", gzipSize: true }),
  ],

  // Use Lightning CSS for CSS processing (minification, vendor prefixing, modern syntax)
  css: {
    transformer: "lightningcss",
  },
  build: {
    cssMinify: "lightningcss",
  },

  // Exclude broken beta package from dep optimization (mismatched entry points)
  optimizeDeps: {
    exclude: ["@xterm/addon-canvas"],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
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
