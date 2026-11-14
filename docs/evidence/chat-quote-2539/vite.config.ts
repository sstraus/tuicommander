import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Evidence host: no backend proxy, app boot or type-check worker.
export default defineConfig({ plugins: [solid()], server: { host: "127.0.0.1", port: 18440, strictPort: true } });
