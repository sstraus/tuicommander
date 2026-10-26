import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../components/SessionCard.module.css"), "utf-8");
const rule = (name: string) => css.match(new RegExp(`\\.${name}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
const margins = (block: string) => {
	const values = (block.match(/(?:^|[;\s])margin:\s*([^;]+);/)?.[1] ?? "0")
		.trim()
		.split(/\s+/)
		.map((v) => Number.parseInt(v, 10));
	const [top, right = top, bottom = top, left = right] = values;
	return { top, right, bottom, left };
};

describe("session card kill button", () => {
	const kill = rule("killBtn");

	// Mounted mobileSessionCardGeometry.jsx checks the shared touch target and28px footprint.
	// Catches: the enlarged touch target reaching left over the card's open-session button,
	// so a tap near the card body kills the session.
	it("does not extend its touch target toward the open-session button", () => {
		expect(margins(kill).left).toBeGreaterThanOrEqual(0);
	});
});
