import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import commander from "../../../src-tauri/src/themes/commander.json";
import paper from "../../../src-tauri/src/themes/vscode-light.json";
import { contrastRatio } from "../../themes";

const css = readFileSync("src/mobile/components/SessionCard.module.css", "utf8");
const palette = readFileSync("src/mobile/mobile.css", "utf8");
const rule = (source: string, selector: string) =>
	[...source.matchAll(/([^{}]+)\{([^{}]*)\}/g)].find((match) => match[1].trim() === selector)?.[2] ?? "";
const color = (block: string) => block.match(/(?:^|[;\s])color:\s*var\((--[\w-]+)\);/)?.[1];
const defaults = Object.fromEntries([...palette.matchAll(/(--[\w-]+):\s*(#[\da-f]+);/gi)].map((m) => [m[1], m[2]]));
const tokens = (theme: typeof commander) => ({
	"--bg-secondary": theme.appChrome.surface,
	"--fg-primary": theme.appChrome.foreground,
	"--fg-secondary": theme.appChrome.foregroundSecondary,
	"--fg-muted": theme.appChrome.mutedForeground,
});

describe("mobile session repository contrast", () => {
	// Catches: the repository inheriting muted metadata text, unreadable on Commander cards.
	it.each([
		["Commander", tokens(commander)],
		["Paper", tokens(paper)],
		["before theme load", defaults],
	] as const)("keeps the repository theme-colored and AA-readable in %s", (_name, theme) => {
		const token = color(rule(css, ".project")) ?? color(rule(css, ".meta"));
		expect(token, "repository must use a theme text token").toBeDefined();
		const foreground = theme[token! as keyof typeof theme];
		expect(foreground).toBeDefined();
		expect(contrastRatio(foreground, theme["--bg-secondary"])).toBeGreaterThanOrEqual(4.5);
	});
});
