import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { ACTION_NAMES } from "../../keybindingDefaults";

/**
 * The user-facing doc promises "the action table below for all available action
 * names". This test enforces that promise: every canonical action in
 * ACTION_NAMES must be documented, so adding an action without a doc row turns
 * the suite red (the same drift that left actions missing from both the panel
 * and this table).
 */
describe("keyboard-shortcuts.md action reference", () => {
	const doc = readFileSync(join(process.cwd(), "docs/user-guide/keyboard-shortcuts.md"), "utf8");

	// Numbered actions are documented as collapsed ranges, not nine rows each.
	const isNumbered = (a: string) => /^switch-(tab|branch)-[1-9]$/.test(a);

	it("documents every canonical action name", () => {
		const undocumented = ACTION_NAMES.filter((a) => !isNumbered(a) && !doc.includes(`\`${a}\``));
		expect(undocumented).toEqual([]);
	});

	it("documents the numbered tab/branch ranges", () => {
		expect(doc).toContain("`switch-tab-1..9`");
		expect(doc).toContain("`switch-branch-1..9`");
	});
});
