import { defineConfig } from "vitest/config";

export default defineConfig({
	test: {
		include: ["src/__tests__/browser/ComposeReservation.browser.ts"],
		environment: "node",
		maxWorkers: 1,
		testTimeout: 60000,
		hookTimeout: 60000,
	},
});
