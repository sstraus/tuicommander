// Keep Vite's native compiler resources in a child process, outside Vitest workers.
import { createServer } from "vite";
const server = await createServer({ configFile: "src/mobile/__tests__/browser/longNames.vite.config.ts", logLevel: "error" });
await server.listen();
process.stdout.write("fixture-ready\n");
process.on("SIGTERM", async () => { await server.close(); process.exit(0); });
