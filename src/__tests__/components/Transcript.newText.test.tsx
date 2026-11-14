// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { acpTranscript } from "../../stores/acpTranscript";
import { transcriptPreferences } from "../../stores/transcriptPreferences";

function append(text: string) {
	acpTranscript.applyFrame({
		kind: "event",
		connectionId: "test",
		generation: 1,
		sequence: 1,
		sessionId: "chat",
		turnId: "turn",
		event: { kind: "sessionUpdate", update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text } } },
	});
}

async function setup(mobile = false) {
	append("Read this earlier answer.");
	const [busy, setBusy] = createSignal(false);
	const view = render(() => (
		<Transcript mobile={mobile} entries={() => acpTranscript.entries("chat")} busy={busy} emptyMessage="Empty" />
	));
	await Promise.resolve();
	const transcript = view.getByLabelText("Chat transcript");
	// jsdom has no layout. These scroll metrics model the browser geometry;
	// the worktree Vite check separately verifies real overflow and positioning.
	Object.defineProperties(transcript, {
		clientHeight: { configurable: true, value: 200 },
		scrollHeight: { configurable: true, value: 900 },
	});
	const scroll = (top: number) => {
		transcript.scrollTop = top;
		fireEvent.scroll(transcript);
	};
	// Establish the initial followed position after installing jsdom layout metrics.
	scroll(700);
	return { ...view, transcript, scroll, setBusy };
}

beforeEach(() => {
	acpTranscript.reset();
	transcriptPreferences.setEnabled(true);
});
afterEach(() => {
	cleanup();
	window.getSelection()?.removeAllRanges();
});

describe("Transcript new text", () => {
	// Catches: streaming output drags the viewport away while the user selects text.
	it("holds the viewport through a mouse selection and follows after it ends", async () => {
		const view = await setup();
		acpTranscript.noteUserMessage("chat", "Keep this message selected.");
		await Promise.resolve();
		view.scroll(700);
		view.setBusy(true);
		const text = view.getByText("Keep this message selected.").firstChild!;
		fireEvent.mouseDown(text.parentElement!, { button: 0 });
		const range = document.createRange();
		range.setStart(text, 0);
		range.setEnd(text, 4);
		window.getSelection()!.removeAllRanges();
		window.getSelection()!.addRange(range);
		fireEvent.mouseMove(text.parentElement!, { buttons: 1 });
		append(" More output while selecting.");
		await Promise.resolve();
		expect(view.transcript.scrollTop).toBe(700);
		fireEvent.mouseUp(document, { button: 0 });
		append(" More output while selected.");
		await Promise.resolve();
		expect(view.transcript.scrollTop).toBe(700);
		window.getSelection()!.removeAllRanges();
		fireEvent(document, new Event("selectionchange"));
		expect(view.transcript.scrollTop).toBe(900);
	});

	// Catches: unread output reserving a full-width layout row instead of floating above the composer.
	it.each([false, true])(
		"floats the new-text badge inside the transcript viewport without a layout row (mobile=%s)",
		async (mobile) => {
			const style = document.createElement("style");
			style.textContent = readFileSync(
				resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
				"utf8",
			);
			document.head.append(style);
			try {
				const view = await setup(mobile);
				view.scroll(300);
				append(" Unread output.");
				await Promise.resolve();
				const badge = view.getByRole("button", { name: "New text below" });
				expect(getComputedStyle(badge).position).toBe("absolute");
				expect(getComputedStyle(badge).right).toBe("12px");
				expect(getComputedStyle(badge).bottom).toBe("8px");
				const viewport = view.transcript.parentElement!;
				expect(badge.parentElement).toBe(viewport);
				expect(getComputedStyle(viewport).position).toBe("relative");
				expect(view.transcript.contains(badge)).toBe(false);
				expect(viewport.children).toHaveLength(2);
				expect(badge.querySelector("svg")).not.toBeNull();
			} finally {
				style.remove();
			}
		},
	);
	// Catches: streaming into an existing message being missed, or clicking the pill leaving follow disabled.
	it.each([false, true])(
		"shows detached streaming text and resumes follow after clicking (mobile=%s)",
		async (mobile) => {
			const view = await setup(mobile);
			view.scroll(300);
			append(" More text in the same message.");
			await Promise.resolve();
			expect(view.transcript.scrollTop).toBe(300);
			fireEvent.click(view.getByRole("button", { name: "New text below" }));
			expect(view.transcript.scrollTop).toBe(900);
			expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
			view.transcript.scrollTop = 700;
			append(" Following again.");
			await Promise.resolve();
			expect(view.transcript.scrollTop).toBe(900);
			expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		},
	);

	// Catches: a pending arrival staying visible after the reader manually reaches the bottom.
	it("clears a new-entry indicator when scrolling to the bottom by hand", async () => {
		const view = await setup();
		view.scroll(300);
		acpTranscript.noteUserMessage("chat", "A new prompt");
		await Promise.resolve();
		expect(view.getByRole("button", { name: "New text below" })).toBeTruthy();
		view.scroll(700);
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
	});

	// Catches: bottom-follow output or a typing-state change being counted as unread text.
	it("does not announce text already followed or a busy-only rerender while detached", async () => {
		const view = await setup();
		view.scroll(700);
		append(" Visible at the bottom.");
		await Promise.resolve();
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		view.scroll(300);
		view.setBusy(true);
		await Promise.resolve();
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		expect(view.transcript.scrollTop).toBe(300);
	});

	// Catches: search detachment itself showing a pill, or search disabling arrival detection permanently.
	it("leaves a search match alone until subsequent content arrives", async () => {
		const view = await setup();
		fireEvent.keyDown(view.transcript, { key: "f", ctrlKey: true });
		await Promise.resolve();
		const input = view.getByRole("textbox", { name: "Find in chat" });
		fireEvent.input(input, { target: { value: "earlier" } });
		fireEvent.keyDown(input, { key: "Enter" });
		expect(window.getSelection()?.toString()).toBe("earlier");
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		append(" New arrival after search.");
		await Promise.resolve();
		expect(view.getByRole("button", { name: "New text below" })).toBeTruthy();
	});

	// Catches: the preference hiding only a stale DOM element while future arrivals recreate it.
	it("clears pending text when disabled and keeps new arrivals hidden", async () => {
		const view = await setup();
		view.scroll(300);
		append(" First unread text.");
		await Promise.resolve();
		expect(view.getByRole("button", { name: "New text below" })).toBeTruthy();
		transcriptPreferences.setEnabled(false);
		append(" Further unread text.");
		await Promise.resolve();
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
		expect(view.transcript.scrollTop).toBe(300);
		transcriptPreferences.setEnabled(true);
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
	});

	// Catches: a cleared/switched conversation inheriting another conversation's pending pill.
	it("drops pending arrivals when the conversation is replaced", async () => {
		const view = await setup();
		view.scroll(300);
		append(" Unread text.");
		await Promise.resolve();
		expect(view.getByRole("button", { name: "New text below" })).toBeTruthy();
		acpTranscript.clear("chat");
		acpTranscript.noteUserMessage("chat", "Different conversation");
		await Promise.resolve();
		expect(view.queryByRole("button", { name: "New text below" })).toBeNull();
	});
});

// Catches: the final streamed answer remains below the viewport after a stationary touch ends or is cancelled.
it.each([
	[false, "touchEnd"],
	[false, "touchCancel"],
	[true, "touchEnd"],
	[true, "touchCancel"],
] as const)("catches up followed output on %s/%s when the stream completed during touch", async (mobile, endEvent) => {
	append("An answer in progress.");
	const view = render(() => (
		<Transcript mobile={mobile} entries={() => acpTranscript.entries("chat")} busy={() => false} emptyMessage="Empty" />
	));
	const viewport = view.getByLabelText("Chat transcript");
	let height = 900;
	let top = 700;
	Object.defineProperties(viewport, {
		clientHeight: { configurable: true, get: () => 200 },
		scrollHeight: { configurable: true, get: () => height },
		scrollTop: {
			configurable: true,
			get: () => top,
			set: (value: number) => {
				top = Math.min(value, height - 200);
			},
		},
	});
	await Promise.resolve();
	fireEvent.scroll(viewport);
	fireEvent.touchStart(viewport);
	height = 1100;
	append(" The final answer arrives while the finger rests on the screen.");
	await Promise.resolve();
	expect(top).toBe(700);
	fireEvent[endEvent](viewport);
	await Promise.resolve();
	expect(top).toBe(900);
});
