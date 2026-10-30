// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { buildAnswersTurn } from "../../components/Terminal/answersTurn";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";
import fixture from "./fixtures/chat-answer-parity.json";

afterEach(cleanup);

function transcript(entries: AcpTranscriptEntry[]) {
	return render(() => (
		<Transcript entries={() => entries} busy={() => false} emptyMessage="Empty" observeToolDuration={false} />
	));
}

describe("transcript presentation", () => {
	// Catches: harness-authored interruptions/inbox wakes impersonating a human prompt.
	it("renders harness notices separately from user bubbles and leaves quoted notices alone", () => {
		const { getAllByRole, container } = transcript([
			{ id: "wake", kind: "user", text: "[TUIC] message available — read it with: agent action=inbox" },
			{ id: "interrupt", kind: "user", text: "[Request interrupted by user for tool use]" },
			{ id: "human", kind: "user", text: "Explain [Request interrupted by user for tool use]" },
		]);
		expect(getAllByRole("note").map((note) => note.textContent)).toEqual([
			"[TUIC] message available — read it with: agent action=inbox",
			"[Request interrupted by user for tool use]",
		]);
		expect(container.querySelectorAll('button[aria-label="Copy user message"]')).toHaveLength(1);
	});

	// Catches: loading a completed tool history inventing a 0.0s execution duration.
	it("shows historical tool status without a duration measured since mounting", () => {
		const { container } = transcript([
			{ id: "tool", kind: "tool", call: { toolCallId: "tool", title: "Read", status: "completed" } },
		]);
		expect(container.querySelector("summary")?.textContent).toContain("Completed");
		expect(container.querySelector("summary")?.textContent).not.toMatch(/\d+\.\d+s observed/);
	});

	// Catches: empty and successive thinking blocks reserving multiple disclosure rows.
	it("omits empty thoughts and merges adjacent thoughts without crossing user turns", () => {
		const { container } = transcript([
			{ id: "empty", kind: "thought", text: " \n" },
			{ id: "t1", kind: "thought", text: "First thought" },
			{ id: "hidden", kind: "agent", text: " " },
			{ id: "t2", kind: "thought", text: "Second thought" },
			{ id: "u", kind: "user", text: "Next turn" },
			{ id: "t3", kind: "thought", text: "New turn thought" },
		]);
		expect(container.querySelectorAll("details")).toHaveLength(2);
		expect(container.querySelector("details")?.textContent).toContain("First thought\n\nSecond thought");
	});

	// Catches: hidden protocol-only/whitespace assistant entries leaving blank gaps and orphan Copy buttons.
	it("allocates no message row or copy action for invisible assistant entries", () => {
		const { container } = transcript([
			{ id: "a", kind: "agent", text: "Visible paragraph" },
			{ id: "empty", kind: "agent", text: " \n" },
			{ id: "suggest", kind: "agent", text: "suggest: [ One | Two | Three ]" },
			{ id: "ack", kind: "agent", text: "TUICommander v1.8.0 is connected." },
			{ id: "u", kind: "user", text: "Actual prompt" },
		]);
		expect(container.querySelectorAll('button[aria-label="Copy assistant message"]')).toHaveLength(1);
		expect(container.querySelector('[aria-label="Chat transcript"]')?.children).toHaveLength(2);
	});

	// Catches: Claude image markers being printed verbatim rather than presented as attachments.
	it("renders numbered and unnumbered image markers as chips while preserving other text", () => {
		const { container, getAllByRole } = transcript([
			{ id: "u", kind: "user", text: "Review this [Image #1]\n[image]\n[image] and keep [image processing]" },
		]);
		expect(getAllByRole("img").map((chip) => chip.getAttribute("aria-label"))).toEqual([
			"Image attachment 1",
			"Image attachment",
		]);
		expect(container.textContent).toContain("Review this");
		expect(container.textContent).toContain("[image processing]");
		expect(container.textContent).not.toContain("[image]");
		expect(container.textContent).not.toContain("[Image #1]");
	});

	// Catches: TUIC answer markers appearing as raw emoji with no CLI-equivalent highlighting.
	it("highlights marked answer paragraphs and preserves marker examples in code and ordinary prose", async () => {
		const [entries, setEntries] = createSignal<AcpTranscriptEntry[]>([
			{
				id: "a",
				kind: "agent",
				text: "💬 **Answer** with a link https://example.com.\n\n```text\n💬 example\n```\n\nMention 💬 inside prose.",
			},
		]);
		const { container } = render(() => <Transcript entries={entries} busy={() => false} emptyMessage="Empty" />);
		await Promise.resolve();
		expect(container.querySelector("[data-tuic-answer]")?.textContent).toBe("Answer with a link https://example.com.");
		expect(container.querySelector("pre")?.textContent).toContain("💬 example");
		expect(container.textContent).toContain("Mention 💬 inside prose.");
		setEntries([{ id: "a", kind: "agent", text: "💬 **Updated answer**" }]);
		await Promise.resolve();
		expect(container.querySelector("[data-tuic-answer]")?.textContent).toBe("Updated answer");
	});

	// Catches: blank lines dropped (paragraphs fold into the list above), only the first paragraph of an
	// answer tinted, and the second 💬 left raw — the chat view disagreeing with the CLI grid.
	it("shows each 💬 answer over the extent the CLI grid highlights", async () => {
		const gridRows = fixture.text
			.split("\n")
			.map((line, i) => ({ text: i === 0 ? `● ${line}` : line && `  ${line}`, isWrapped: false }));
		const grid = buildAnswersTurn(gridRows, false).answers;
		const { container } = render(() => (
			<Transcript
				entries={() => [{ id: "a", kind: "agent", text: fixture.text }]}
				busy={() => false}
				emptyMessage="Empty"
				observeToolDuration={false}
			/>
		));
		await Promise.resolve();
		const blocks = Array.from(container.querySelectorAll<HTMLElement>("#markdown-content > div > *"));
		expect(blocks.map((block) => block.tagName)).toEqual(["P", "UL", "P", "UL", "P", "P"]);
		expect(blocks.map((block) => block.getAttribute("data-tuic-answer"))).toEqual(["", "", "", "end", null, null]);
		const heads = blocks.filter((block) => block.hasAttribute("data-tuic-answer-start"));
		expect(heads.map((head) => head.textContent)).toEqual([
			"Sì, l'avevamo valutata e scartata nel piano originale di Design Mode, a settembre. Non funziona per due motivi:",
			"Il plugin secondo me è la strada giusta, come estensione di Chrome:",
		]);
		expect(container.textContent).not.toContain("💬");
		expect(grid).toHaveLength(1);
		expect(grid[0]).not.toContain(blocks[4].textContent ?? "");
	});
});

// Catches: old suggested replies remaining actionable after a newer answer.
it("offers suggestions only beneath the latest answer", () => {
	const { getAllByRole } = render(() => (
		<Transcript
			busy={() => false}
			emptyMessage=""
			onSuggestion={() => {}}
			entries={() => [
				{ id: "a", kind: "agent", text: "First answer\n\nsuggest: [ Old one | Old two | Old three ]" },
				{ id: "u", kind: "user", text: "Next" },
				{ id: "b", kind: "agent", text: "Latest answer\n\nsuggest: [ New one | New two | New three ]" },
			]}
		/>
	));
	expect(
		getAllByRole("button")
			.filter((button) => button.closest('[aria-label="Suggested replies"]'))
			.map((button) => button.textContent),
	).toEqual(["New one", "New two", "New three"]);
});
// TEST-CHANGE (#1970-e1f4): Boss wants failed output closed by default, superseding #1958's expansion.
// Catches: process intent using a separate row, or failed group/call details opening automatically.
it("merges intent into the activity line and keeps failed group and call closed", () => {
	const { container } = transcript([
		{ id: "intent", kind: "agent", text: "intent: Reading the source (Source check)" },
		{
			id: "tool",
			kind: "tool",
			call: {
				toolCallId: "read",
				kind: "read",
				title: "Read source",
				status: "failed",
				content: [{ type: "content", content: { type: "text", text: "File unavailable" } }],
			},
		},
	]);
	const activity = container.querySelector<HTMLDetailsElement>("details[class*=toolActivity]")!;
	expect([activity.open, activity.querySelector("details")?.open]).toEqual([false, false]);
	expect(activity.querySelector("summary")?.textContent).toContain("Failed");
	expect(activity.querySelector("summary span[class*=toolCallFailure]")).not.toBeNull();
	activity.open = true;
	const call = activity.querySelector("details")!;
	expect(call.open).toBe(false);
	call.open = true;
	expect(call.textContent).toContain("File unavailable");
	expect(activity.querySelector("summary")?.textContent).toContain("Source check");
	expect(activity.querySelector("summary")?.textContent).toContain("Reading the source");
	expect(activity.querySelector("summary")?.textContent).toContain("1 read");
	expect(container.querySelector('[aria-label="Agent intent"]')).toBeNull();
});

// Catches: an ACP message boundary leaving the final suggestion-only chunk invisible.
it("keeps suggestions in a separate final message actionable without reviving old chips", () => {
	const { container } = render(() => (
		<Transcript
			entries={() => [
				{ id: "answer", kind: "agent", text: "Answer\n\nsuggest: [ Old A | Old B | Old C ]" },
				{ id: "suggestions", kind: "agent", text: "suggest: [ Next A | Next B | Next C ]" },
			]}
			busy={() => false}
			emptyMessage=""
			onSuggestion={() => {}}
		/>
	));
	expect(
		[...container.querySelectorAll('[aria-label="Suggested replies"] button')].map((button) => button.textContent),
	).toEqual(["Next A", "Next B", "Next C"]);
});

// Catches: pending styling being forced on acknowledged messages or removed from queued prompts.
it("marks only pending user bubbles for pending styling", () => {
	const { container, getByLabelText } = transcript([
		{ id: "received", kind: "user", text: "Acknowledged prompt" },
		{ id: "pending", kind: "user", text: "Queued prompt", pending: true },
	]);
	const pending = getByLabelText("Pending prompt");
	expect(pending.getAttribute("data-pending")).toBe("true");
	expect(container.querySelectorAll('[data-pending="true"]')).toHaveLength(1);
	const received = [...container.querySelectorAll('button[aria-label="Copy user message"]')]
		.map((button) => button.parentElement!)
		.find((bubble) => bubble.textContent?.includes("Acknowledged prompt"))!;
	expect(received.hasAttribute("data-pending")).toBe(false);
});
