import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { readStylesheet } from "../../__tests__/fixtures/readStylesheet";

const chatCss = readFileSync(resolve(__dirname, "../../components/AIChatPanel/AIChatPanel.module.css"), "utf-8");
const mobileCss = readStylesheet(resolve(__dirname, "../mobile.css"));
const rule = chatCss.match(/\.assistantMsg\.intermediateMsg\s*\{([^}]*)\}/)?.[1] ?? "";

describe("muted intermediate assistant text on mobile", () => {
	// Catches: the muted size using a desktop-only token (--font-sm). The mobile shell never defines
	// it, so `font-size` became invalid and the muted block kept the full 16px size (#1769-89dc).
	it("only reads size and colour tokens that mobile defines or that carry a fallback", () => {
		expect(rule).toContain("font-size");
		const bare = [...rule.matchAll(/var\((--[\w-]+)\s*\)/g)].map((m) => m[1]);
		const undefinedOnMobile = bare.filter((token) => !new RegExp(`${token}\\s*:`).test(mobileCss));
		expect(undefinedOnMobile).toEqual([]);
	});
});
