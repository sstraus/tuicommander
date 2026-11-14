// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterAll, afterEach, beforeAll, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

const terminalStyles = readFileSync("src/components/AIChatPanel/AIChatPanel.module.css", "utf8");
let style: HTMLStyleElement;
beforeAll(async () => {
	// App startup owns hydration; this isolated render must settle its imported store too.
	const { activityStore } = await import("../../stores/activityStore");
	await activityStore.hydrate();
	style = document.createElement("style");
	style.textContent = (terminalStyles.replace(/\/\*[\s\S]*?\*\//g, "").match(/[^{}]+\{[^{}]*\}/g) ?? [])
		.filter((rule) => rule.trim().startsWith(".messageList[data-terminal-chat]"))
		.map((rule) => rule.replace(/:global\((.*)\)/g, "$1").replace(/var\(--(?:success|answer-rule)\)/g, "green"))
		.join("\n");
	document.head.append(style);
});
afterEach(cleanup);
afterAll(() => style.remove());

function expectOuterRail(container: HTMLElement, itemCount: number) {
	const rails = Array.from(container.querySelectorAll<HTMLElement>("[data-tuic-answer-region]"));
	expect(rails.length).toBeGreaterThan(0);
	// Independent streaming segments can own adjacent fragments of one continuous rail.
	expect(rails.filter((rail) => rail.getAttribute("data-tuic-answer-region") === "end")).toHaveLength(1);
	for (const rail of rails) {
		expect(rail.parentElement?.classList.contains("md-segment")).toBe(true);
		expect(getComputedStyle(rail).borderLeftWidth).toBe("3px");
	}
	expect(rails.flatMap((rail) => Array.from(rail.querySelectorAll("li")))).toHaveLength(itemCount);
	expect(container.querySelector("li [data-tuic-answer-region]")).toBeNull();
	// jsdom reports spurious inherited border widths on descendants. Match the
	// actual border declarations, as TerminalChatAnswerScope does; Chrome owns geometry.
	const borderRules = Array.from(style.sheet!.cssRules).filter(
		(rule): rule is CSSStyleRule =>
			rule.type === CSSRule.STYLE_RULE && parseFloat((rule as CSSStyleRule).style.getPropertyValue("border-left")) > 0,
	);
	const bordered = Array.from(container.querySelectorAll("*")).filter((element) =>
		borderRules.some((rule) => element.matches(rule.selectorText)),
	);
	expect(bordered).toEqual(rails);
	expect(container.textContent).not.toContain("💬");
	return rails;
}

// Catches: item-owned borders appearing right of bullets, nested rails, or fragmented adjacent answers.
it.each<[string, string, number]>([
	["bullet", "- 💬 First `code`.\n- 💬 Second.", 2],
	["ordered", "3. 💬 First.\n4. 💬 Second.", 2],
	["mixed", "- Context.\n- 💬 Answer.\n- More context.", 3],
	["nested", "- Context parent\n  - 💬 Nested answer.\n  - Context sibling.", 3],
	["paragraph and list", "💬 Result:\n\n- First\n  - Nested\n\n💬 Done.", 2],
])("puts one outer rail around %s answer context", async (name, text, itemCount) => {
	const { container } = render(() => (
		<Transcript
			entries={() => [{ id: "a", kind: "agent", text }]}
			busy={() => false}
			emptyMessage=""
			terminalAgent="ego"
			observeToolDuration={false}
		/>
	));
	await Promise.resolve();
	const rails = expectOuterRail(container, itemCount);
	if (name === "mixed") {
		expect(Array.from(rails[0].querySelectorAll("li")).map((li) => li.hasAttribute("data-tuic-answer"))).toEqual([
			false,
			true,
			false,
		]);
	}
	if (name === "ordered") expect(rails[0].querySelector("ol")?.start).toBe(3);
});

// Catches: wrapping retained committed DOM a second time, or moving a live tail out of its renderer-owned segment.
it("rebuilds outer rails after a streamed tail changes without retaining stale content", async () => {
	const [entries, setEntries] = createSignal<AcpTranscriptEntry[]>([
		{ id: "stream", kind: "agent", text: "💬 First.\n\n- 💬 Original." },
	]);
	const { container } = render(() => (
		<Transcript entries={entries} busy={() => false} emptyMessage="" terminalAgent="ego" observeToolDuration={false} />
	));
	await Promise.resolve();
	setEntries([{ id: "stream", kind: "agent", text: "💬 First.\n\n- 💬 Original.\n- 💬 Appended.\n\nUnmarked tail." }]);
	await Promise.resolve();
	await Promise.resolve();
	expectOuterRail(container, 2);
	expect(container.querySelector("[data-tuic-answer-region] [data-tuic-answer-region]")).toBeNull();
	expect(container.textContent?.match(/Original\./g)).toHaveLength(1);
	const tail = Array.from(container.querySelectorAll("p")).find((p) => p.textContent === "Unmarked tail.");
	expect(tail?.closest("[data-tuic-answer-region]")).toBeNull();
});
