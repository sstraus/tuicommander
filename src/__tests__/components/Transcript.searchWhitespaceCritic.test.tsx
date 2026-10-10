// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";

afterEach(() => {
	cleanup();
	window.getSelection()?.removeAllRanges();
});

it("does not miss a visible phrase when Markdown whitespace collapses between inline nodes", async () => {
	render(() => (
		<Transcript
			entries={() => [{ id: "reply", kind: "agent", text: "Use  **git**   rebase to continue." }]}
			busy={() => false}
			emptyMessage="No messages"
		/>
	));
	await Promise.resolve();
	fireEvent.keyDown(screen.getByText("git"), { key: "f", ctrlKey: true });
	const input = screen.getByRole("textbox", { name: "Find in chat" });
	fireEvent.input(input, { target: { value: "Use git rebase" } });
	fireEvent.keyDown(input, { key: "Enter" });
	// Prose collapses these spaces on screen; the DOM Range retains source spaces.
	expect(window.getSelection()?.toString()).toBe("Use  git   rebase");
});

it("keeps code block whitespace literal instead of collapsing a different phrase into a match", async () => {
	render(() => (
		<Transcript
			entries={() => [{ id: "reply", kind: "agent", text: "```text\nUse  git   rebase\n```" }]}
			busy={() => false}
			emptyMessage="No messages"
		/>
	));
	await Promise.resolve();
	fireEvent.keyDown(screen.getByText("Use  git   rebase", { normalizer: (text) => text }), { key: "f", ctrlKey: true });
	const input = screen.getByRole("textbox", { name: "Find in chat" });
	fireEvent.input(input, { target: { value: "Use git rebase" } });
	fireEvent.keyDown(input, { key: "Enter" });
	expect(window.getSelection()?.toString()).toBe("");
	fireEvent.input(input, { target: { value: "Use  git   rebase" } });
	fireEvent.keyDown(input, { key: "Enter" });
	expect(window.getSelection()?.toString()).toBe("Use  git   rebase");
});
