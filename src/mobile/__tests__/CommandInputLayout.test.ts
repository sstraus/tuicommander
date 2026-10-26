import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../components/CommandInput.module.css"), "utf-8");
// All blocks for a selector (including the shared `.send, .dictate` one), in source order.
const rule = (name: string) =>
	[...css.matchAll(new RegExp(`\\.${name}\\s*\\{([^}]*)\\}`, "g"))].map((m) => m[1]).join(";\n");
const form = rule("form");

describe("mobile command input layout (send outside the box)", () => {
	// Touch targets, overlap and the >=160px usable text budget are measured in
	// mobileCommandInputGeometry at 320px/390px against the mounted shared controls.
	// Catches: a fixed px width/max-width on the textarea or its box, which would stop it filling
	// the row on wide viewports (iPad 820/1024px) - the 286px figure is an iPhone result, not a value.
	it("keeps the textarea and its box fluid in the row", () => {
		for (const name of ["input", "composer"]) {
			const block = rule(name);
			expect(block, name).not.toMatch(/(?:^|[;\s])(?:max-)?width:/);
			expect(block, name).toMatch(/flex:\s*1\b/);
			expect(block, name).toMatch(/min-width:\s*0\b/);
		}
		expect(form).not.toMatch(/(?:^|[;\s])(?:max-)?width:/);
	});
});
