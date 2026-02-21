import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";
import path from "node:path";

export default defineConfig({
  plugins: [solid()],
  define: {
    __APP_VERSION__: JSON.stringify("0.3.0"),
  },
  resolve: {
    conditions: ["development", "browser"],
    alias: {
      // Mock SVG imports in tests
      "^.+\\.svg$": path.resolve(__dirname, "src/__tests__/mocks/svg.ts"),
    },
  },
  test: {
    environment: "happy-dom",
    globals: true,
    setupFiles: ["src/__tests__/setup.ts"],
    alias: {
      "\\.svg$": path.resolve(__dirname, "src/__tests__/mocks/svg.ts"),
    },
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/__tests__/**",
        "src/index.tsx",
        "src/types/**",
        "src/**/index.ts",
        // Untestable without runtime: Tauri APIs, xterm.js, complex Tauri IPC
        "src/App.tsx",
        "src/components/Terminal/Terminal.tsx",
        "src/components/IdeLauncher/IdeLauncher.tsx",
        "src/components/PromptDrawer/PromptDrawer.tsx",
      ],
      thresholds: {
        lines: 80,
        functions: 80,
        branches: 80,
        statements: 80,
      },
    },
  },
});
