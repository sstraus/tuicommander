import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { promptKeyAction } from "../../utils/promptKeybinding";

describe("prompt keyboard model", () => {
	// Catches: IME confirmation or an unrelated shortcut being treated as send/park.
	it.each([{ key: "Enter", isComposing: true }, { key: "s", ctrlKey: true, altKey: true }, { key: "s" }])(
		"ignores unrelated/composing keys %o",
		(init) => {
			expect(promptKeyAction(new KeyboardEvent("keydown", init))).toBeNull();
		},
	);
});

// Catches: a composer added to the inventory bypasses the shared keyboard model.
it("every inventoried prompt composer uses the shared keybinding owner", () => {
	const inventory = readFileSync("docs/frontend/prompt-input-keyboard-inventory.md", "utf8");
	const paths = [...inventory.matchAll(/<!-- prompt-composer: (.+?) -->/g)].map((match) => match[1]);
	expect(paths.length).toBeGreaterThan(0);
	for (const path of paths) {
		const source = readFileSync(path, "utf8");
		expect(source, path).toContain("promptKeyAction");
		expect(source, path).toMatch(/import \{ promptKeyAction \} from/);
	}
});
