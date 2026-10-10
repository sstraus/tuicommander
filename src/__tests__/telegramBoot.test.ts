import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("Telegram boot wiring", () => {
	// Catches: desktop accepts Telegram setup but never starts the shared poller.
	it("starts the shared Telegram supervisor at desktop and daemon boot", () => {
		const source = readFileSync("src-tauri/src/lib.rs", "utf8");
		for (const entry of ["pub fn run()", "pub async fn run_remote("]) {
			const body = source.split(entry)[1]?.split("\n}\n")[0];
			expect(body, entry).toContain("telegram::start(");
		}
	});
});
