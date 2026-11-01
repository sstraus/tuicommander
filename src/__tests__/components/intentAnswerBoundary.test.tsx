// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, expect, it } from "vitest";
import { projectChatProtocolText } from "../../components/AIChatPanel/protocolText";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { buildAnswersTurn } from "../../components/Terminal/answersTurn";
import { type DecodedRow, rowText } from "../../components/Terminal/canvasTerminalUtils";
import { paintOverlayBlocks, planSuggestOverlay, type RowSnapshot } from "../../components/Terminal/suggestOverlay";
import { ViewportProjection } from "../../components/Terminal/viewportProjection";
import captured from "./fixtures/ego-joined-intent-answer-2612.json";

const reported = "intent: Ti indico i passaggi ... (Configurazione Claude)💬 Per configurare la tua installazione ...";
afterEach(cleanup);

// Catches: an answer joined to an intent gets no answer gutter in the real terminal overlay.
it("renders the reported Italian same-line answer outside the intent tint", () => {
	const plan = planSuggestOverlay(1, () => ({ text: reported, isWrapped: false }));
	const overlay = document.createElement("div");
	paintOverlayBlocks(overlay, plan.blocks, 20, 10);
	expect(overlay.children).toHaveLength(2);
	expect((overlay.children[0] as HTMLElement).style.width).toBe("560px");
	expect((overlay.children[1] as HTMLElement).style.boxShadow).toContain("var(--answer-rule)");
	expect((overlay.children[1] as HTMLElement).style.left).toBe("560px");
});

// Catches: answer parentheses replace the intent title, or Chat loses the answer after projection.
it.each(["", " (default)"])("keeps the reported answer in the real Chat renderer: %s", async (suffix) => {
	const text = reported + suffix;
	expect(projectChatProtocolText(text).intent).toEqual({
		text: "Ti indico i passaggi ...",
		title: "Configurazione Claude",
	});
	const { container } = render(() => (
		<Transcript
			entries={() => [{ id: "reported", kind: "agent", text }]}
			busy={() => false}
			emptyMessage=""
			terminalAgent="ego"
			observeToolDuration={false}
		/>
	));
	await Promise.resolve();
	expect(container.querySelector("[data-tuic-answer]")?.textContent).toBe(
		"Per configurare la tua installazione ..." + suffix,
	);
});

// Catches: a literal description marker is treated as an answer, and untitled intents gain a guessed boundary.
it.each([
	["intent: Explain 💬 markers (Markers)", false],
	["intent: Explain 💬 markers", false],
	["intent: Explain (Markers)💬 One 💬 two", true],
	["- 💬 List answer", true],
])("keeps marker scope for %s", async (text, answer) => {
	const plan = planSuggestOverlay(1, () => ({ text, isWrapped: false }));
	expect(plan.blocks.some((block) => block.kind === "answer")).toBe(answer);
	const { container } = render(() => (
		<Transcript
			entries={() => [{ id: "sibling", kind: "agent", text }]}
			busy={() => false}
			emptyMessage=""
			terminalAgent="ego"
			observeToolDuration={false}
		/>
	));
	await Promise.resolve();
	expect(container.querySelector("[data-tuic-answer]") !== null).toBe(answer);
	if (text.includes("One")) expect(container.querySelector("[data-tuic-answer]")?.textContent).toBe("One 💬 two");
});

// Catches: recovery working on a short synthetic line but dropping the original long answer.
it("classifies the retained ego row and wrapped siblings without consuming answer parentheses", () => {
	for (const width of [40, 1000]) {
		const rows: RowSnapshot[] = [];
		for (let offset = 0; offset < captured.text.length; offset += width)
			rows.push({ text: captured.text.slice(offset, offset + width), isWrapped: offset > 0 });
		const plan = planSuggestOverlay(rows.length, (i) => rows[i] ?? null);
		expect(plan.blocks.some((block) => block.kind === "answer")).toBe(true);
		expect(buildAnswersTurn(rows, false).answers).toEqual([
			"💬 Per configurare la tua installazione come default, non basta copiare verify.py: è uno script di confronto, non un’estensione installabile. Il valore chiamato «header» viene inserito nel corpo JSON della richiesta e il CCH va ricalcolato sui byte finali a ogni invio. Gli script di cattura non servono nell’installazione.",
		]);
	}
});

// Catches: omitting the first literal bubble in the intent instead of the answer marker.
it("projects only the answer marker when the description also contains a bubble", () => {
	const text = "intent: Explain 💬 syntax (Markers)💬 Answer 💬 literal";
	const codepoints = Uint32Array.from(
		[...text].flatMap((char) => (char === "💬" ? [0x1f4ac, 0] : [char.codePointAt(0)!])),
	);
	const row: DecodedRow = {
		index: 0,
		count: codepoints.length,
		codepoints,
		fg: new Uint32Array(codepoints.length),
		bg: new Uint32Array(codepoints.length),
		attrs: new Uint8Array(codepoints.length),
		wrapped: false,
	};
	const plan = planSuggestOverlay(1, () => ({ text, isWrapped: false }));
	const projection = new ViewportProjection(new Map([[0, row]]), new Set(), plan.answerMarkers);
	expect(rowText(projection.projectRow(row)!)).toBe("intent: Explain 💬  syntax (Markers)Answer 💬  literal");
});
