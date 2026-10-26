// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

const clipboard = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: clipboard }));
afterEach(() => {
	cleanup();
	clipboard.mockClear();
});

function chat(entries: AcpTranscriptEntry[], agent = "claude") {
	return render(() => (
		<Transcript entries={() => entries} busy={() => false} emptyMessage="Empty" terminalAgent={agent} />
	));
}

// Catches: ordinary multiline prose mistaken for terminal output, or real pasted output left uncollapsed.
it("keeps ordinary multiline prose as prose while collapsing and expanding terminal output", async () => {
	const prose = "Please read this carefully.\nThen describe the result.\nKeep the explanation short.";
	const { queryByRole, container } = chat([{ id: "u", kind: "user", text: prose }]);
	expect(queryByRole("button", { name: /more lines/ })).toBeNull();
	expect(container.textContent).toContain(prose);
	cleanup();
	const output = chat([
		{
			id: "u",
			kind: "user",
			text: "Check this:\nBash(cat result.txt)\n  ⎿ RESULT complete\n  next_since=123\n  exit=0",
		},
	]);
	expect(output.container.querySelector("pre")?.textContent).toBe("Bash(cat result.txt)\n  ⎿ RESULT complete");
	await fireEvent.click(output.getByRole("button", { name: "Show 2 more lines" }));
	expect(output.container.querySelector("pre")?.textContent).toContain("exit=0");
});

// Catches: separate answer blocks creating separate cards, Copy dropping later paragraphs, or merging across human turns.
it("copies all marked answer blocks in one turn without merging the next turn", async () => {
	const { getAllByRole, container } = chat([
		{ id: "a", kind: "agent", text: "💬 First answer." },
		{ id: "t", kind: "tool", call: { toolCallId: "t", title: "Read: result.txt", status: "completed" } },
		{ id: "b", kind: "agent", text: "💬 Second answer.\n\n💬 Third paragraph." },
		{ id: "u", kind: "user", text: "Next question" },
		{ id: "c", kind: "agent", text: "💬 Next answer." },
	]);
	await Promise.resolve();
	expect(container.querySelectorAll("[data-answer-card]")).toHaveLength(2);
	const copies = getAllByRole("button", { name: "Copy assistant message" });
	expect(copies).toHaveLength(2);
	await fireEvent.click(copies[0]);
	expect(clipboard).toHaveBeenCalledWith("First answer.\n\nSecond answer.\n\nThird paragraph.");
});

// Catches: raw Bash commands obscuring descriptions or non-Claude tools borrowing Claude descriptions.
it("keeps the collapsed tool row while using Claude Bash descriptions and non-Claude title fallbacks", () => {
	const entries: AcpTranscriptEntry[] = [
		{
			id: "t0",
			kind: "tool",
			call: {
				toolCallId: "t0",
				title: "Bash: cat result.txt",
				rawInput: { description: "Inspect result" },
				status: "completed",
			},
		},
		{
			id: "t1",
			kind: "tool",
			call: {
				toolCallId: "t1",
				title: "Read: report.md",
				rawInput: { description: "Do not use this" },
				status: "completed",
			},
		},
	];
	const claude = chat(entries);
	expect(claude.container.querySelector("summary")?.textContent).toContain(
		"2 tool calls ›Inspect result · Read: report.md",
	);
	expect(claude.container.querySelector("details")?.open).toBe(false);
	cleanup();
	const grok = chat(entries, "grok");
	expect(grok.container.querySelector("summary")?.textContent).toContain("Bash: cat result.txt · Read: report.md");
	expect(grok.container.querySelector("summary")?.textContent).not.toContain("Inspect result");
});

// Catches: mail wakes leaving visible chat rows, or hiding interruptions and real user text with them.
it("hides the exact inbox wake and leaves other notices and quoted text alone", () => {
	const notice = chat([{ id: "n", kind: "user", text: "[TUIC] message available — read it with: agent action=inbox" }]);
	expect(notice.container.querySelector('[aria-label="Chat transcript"]')?.textContent).toBe("");
	expect(notice.container.querySelector('[aria-label="Chat transcript"]')?.children).toHaveLength(0);
	cleanup();
	const { getAllByRole, container } = chat([
		{ id: "i1", kind: "user", text: "[Request interrupted by user for tool use]" },
		{ id: "i2", kind: "user", text: "[Request interrupted by user]" },
		{ id: "u", kind: "user", text: "Explain [TUIC] message available" },
	]);
	expect(getAllByRole("note").map((note) => note.textContent)).toEqual([
		"[Request interrupted by user for tool use]",
		"[Request interrupted by user]",
	]);
	expect(container.textContent).toContain("Explain [TUIC] message available");
	expect(container.querySelectorAll('button[aria-label="Copy user message"]')).toHaveLength(1);
});

// Catches: inventing tool descriptions when Bash has no usable description.
it.each([undefined, "", "   ", 42])("keeps Bash's backend title for unusable description %s", (description) => {
	const { container } = chat([
		{
			id: "t",
			kind: "tool",
			call: { toolCallId: "t", title: "Bash: pwd", rawInput: { description }, status: "completed" },
		},
	]);
	expect(container.querySelector("summary")?.textContent).toContain("Bash: pwd");
});

// Catches: requiring Claude markers for other adapters, or folding intermediate status into the answer copy.
it("groups unmarked Codex replies while keeping tool-followed status outside the card", async () => {
	const { container, getAllByRole } = chat(
		[
			{ id: "s", kind: "agent", text: "Checking files." },
			{ id: "t", kind: "tool", call: { toolCallId: "t", title: "shell: pwd", status: "completed" } },
			{ id: "a", kind: "agent", text: "First result." },
			{ id: "b", kind: "agent", text: "Second result." },
		],
		"codex",
	);
	const card = container.querySelector("[data-answer-card]");
	expect(card?.textContent).toContain("First result.");
	expect(card?.textContent).toContain("Second result.");
	expect(card?.textContent).not.toContain("Checking files.");
	expect(container.querySelectorAll('[role="separator"]')).toHaveLength(0);
	await fireEvent.click(getAllByRole("button", { name: "Copy assistant message" }).at(-1)!);
	expect(clipboard).toHaveBeenCalledWith("First result.\n\nSecond result.");
});

// Catches: terminalAnswerRows handing <For> a fresh row object per recompute, so every earlier answer is rebuilt on each append.
it("keeps an earlier answer's DOM node when the transcript grows, while the last answer keeps streaming", async () => {
	const first: AcpTranscriptEntry = { id: "a", kind: "agent", text: "💬 First answer." };
	const second: AcpTranscriptEntry = { id: "b", kind: "agent", text: "💬 Second" };
	const [entries, setEntries] = createSignal<AcpTranscriptEntry[]>([
		first,
		{ id: "u", kind: "user", text: "Next question" },
		second,
	]);
	const { container } = render(() => (
		<Transcript entries={entries} busy={() => false} emptyMessage="Empty" terminalAgent="claude" />
	));
	await Promise.resolve();
	const before = container.querySelector("#markdown-content");
	expect(before?.textContent).toContain("First answer.");
	setEntries([
		first,
		{ id: "u", kind: "user", text: "Next question" },
		{ ...second, text: "💬 Second answer, longer." },
	]);
	await Promise.resolve();
	const roots = container.querySelectorAll("#markdown-content");
	expect(roots[0]).toBe(before);
	expect(roots[1].textContent).toContain("Second answer, longer.");
});
