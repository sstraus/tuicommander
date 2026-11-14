// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import recordedCodex from "../../components/Terminal/__tests__/fixtures/codex-answer-paragraphs-1872.json";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

// Isolated renders do not run App startup, which settles the persisted-store promise.
beforeAll(async () => {
	const { activityStore } = await import("../../stores/activityStore");
	await activityStore.hydrate();
});
afterEach(cleanup);

async function show(text: string) {
	const view = render(() => (
		<Transcript
			entries={() => [{ id: "a", kind: "agent", text }]}
			busy={() => false}
			emptyMessage="Empty"
			observeToolDuration={false}
		/>
	));
	await Promise.resolve();
	const blocks = Array.from(view.container.querySelectorAll<HTMLElement>("#markdown-content > div > *"));
	return { ...view, blocks, marks: blocks.map((block) => block.getAttribute("data-tuic-answer")) };
}

describe("chat answer shapes follow the CLI grid extent", () => {
	// Catches: a single-line answer left raw or untinted.
	it("tints and strips a one-line answer", async () => {
		const { blocks, marks } = await show("💬 Yes.");
		expect(marks).toEqual(["end"]);
		expect(blocks[0].textContent).toBe("Yes.");
	});

	// Catches: a table, nested list or code fence ending the answer early (tint stops, tail untinted).
	it("keeps table, nested list and a fenced marker example inside the answer", async () => {
		const text =
			"💬 Result:\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- x\n  - nested\n\n```text\n💬 literal\n```\n\nClosing words.";
		const { blocks, marks, container } = await show(text);
		expect(blocks.map((b) => b.tagName)).toEqual(["P", "TABLE", "UL", "PRE", "P"]);
		expect(marks).toEqual(["", "", "", "end", null]);
		expect(container.querySelectorAll("[data-tuic-answer-start]")).toHaveLength(1);
		expect(container.querySelector("pre")?.textContent).toContain("💬 literal");
	});

	// Catches: prose before the first marker being swept into the answer.
	it("does not tint text before the first marker", async () => {
		const { marks } = await show("Intro.\n\n💬 Answer.\n\nMore.");
		expect(marks).toEqual([null, "end", null]);
	});

	// Catches: streaming a second answer onto a finished one leaving a raw 💬 or a stale "end" mark.
	it("extends the region when a second marked paragraph streams in", async () => {
		const [entries, setEntries] = createSignal<AcpTranscriptEntry[]>([
			{ id: "a", kind: "agent", text: "💬 One.\nMore." },
		]);
		const { container } = render(() => <Transcript entries={entries} busy={() => false} emptyMessage="Empty" />);
		await Promise.resolve();
		setEntries([{ id: "a", kind: "agent", text: "💬 One.\nMore.\n\n💬 Two.\n\nEnd." }]);
		await Promise.resolve();
		await Promise.resolve();
		const blocks = Array.from(container.querySelectorAll<HTMLElement>("#markdown-content > div > *"));
		expect(blocks.map((b) => b.getAttribute("data-tuic-answer"))).toEqual(["", "end", null]);
		expect(container.textContent).not.toContain("💬");
	});

	// Catches: the shared range merging while the chat still draws gaps or leaves later glyphs raw.
	it("merges the recorded Codex paragraphs into one region and strips each marker", async () => {
		const text = recordedCodex.answerRows.map((row) => row.text.replace(/^[• ]+/, "")).join("\n");
		const { container, blocks, marks } = await show(text);
		expect(marks).toEqual(["", "", "end"]);
		expect(container.querySelectorAll("[data-tuic-answer-region-start]")).toHaveLength(1);
		expect(blocks.map((block) => block.textContent)).toEqual([
			"The answer highlight runs continuously across the full response.",
			"Blank paragraph spacing stays preserved.",
			"Raw copy text remains untouched.",
		]);
	});

	// Catches: a 💬 on its own line inside one paragraph (soft break) staying raw, though the grid row starts an answer.
	it("strips a 💬 that starts a line inside a paragraph", async () => {
		const { container } = await show("Intro line.\n💬 The answer.");
		expect(container.textContent).not.toContain("💬");
	});
});

// Catches: only paragraphs opening answers, or a marked list tinting unmarked siblings.
it.each(["-", "*", "1."])("rules only the marked %s list item including nested content", async (bullet) => {
	const { container } = await show(
		`${bullet} 💬 **Answer.**\n   continued\n   - nested detail\n${bullet} Unmarked sibling.`,
	);
	const items = Array.from(
		container.querySelectorAll<HTMLElement>("#markdown-content > div > ul > li, #markdown-content > div > ol > li"),
	);
	expect(items).toHaveLength(2);
	expect(items.map((item) => item.getAttribute("data-tuic-answer"))).toEqual(["end", null]);
	expect(items[0].textContent).toContain("nested detail");
	expect(items[0].textContent).not.toContain("💬");
});

// Catches: a literal nested marker making its unmarked parent an answer.
it("rules a nested marked item without ruling its parent or sibling", async () => {
	const { container } = await show(
		"- Unmarked parent\n  - 💬 Nested answer\n  - Unmarked nested sibling\n\n> 💬 quotation\n>\n> - 💬 quoted item\n\n```text\n- 💬 literal\n```",
	);
	const items = Array.from(container.querySelectorAll<HTMLElement>("#markdown-content > div > ul li"));
	expect(items.map((item) => item.getAttribute("data-tuic-answer"))).toEqual([null, "end", null]);
	expect(container.querySelector("blockquote")?.textContent).toContain("💬 quotation");
	expect(container.querySelector("blockquote li")?.hasAttribute("data-tuic-answer")).toBe(false);
	expect(container.querySelector("blockquote li")?.textContent).toContain("💬 quoted item");
	expect(container.querySelector("pre")?.textContent).toContain("💬 literal");
});

// Catches: independently marked items retaining inner rules rather than the list gutter.
it.each(["-", "1."])("shares one outer rule for an entirely marked %s list", async (bullet) => {
	const { container } = await show(`${bullet} 💬 First answer.\n${bullet} 💬 Second answer.`);
	const list = container.querySelector("ul, ol")!;
	expect(list.getAttribute("data-tuic-answer")).toBe("end");
	expect(Array.from(list.children).map((item) => item.hasAttribute("data-tuic-answer"))).toEqual([false, false]);
	expect(list.textContent).not.toContain("💬");
});
