// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

const writeClipboard = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("../../utils/clipboard", () => ({ writeClipboard }));

import { feedbackMessage, quoteMessage } from "../../components/AIChatPanel/MessageMenu";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

afterEach(() => {
	cleanup();
	writeClipboard.mockClear();
});

const entries: AcpTranscriptEntry[] = [
	{ id: "u1", kind: "user", text: "Why?" },
	{ id: "a1", kind: "agent", text: "Because **bold** and `code`.\nSecond line" },
];

function view(onQuote?: (quoted: string) => void) {
	return render(() => (
		<Transcript
			entries={() => entries}
			busy={() => false}
			emptyMessage="Empty"
			observeToolDuration={false}
			onQuote={onQuote}
		/>
	));
}

const open = (ui: ReturnType<typeof view>, name: string) => {
	const trigger = ui.getByRole("button", { name });
	fireEvent.click(trigger);
	return trigger;
};

describe("message menu", () => {
	// Catches: Copy staying a permanent visible button instead of living in the menu.
	it("shows no Copy button until the menu opens", () => {
		const ui = view();
		expect(ui.queryByRole("button", { name: /^Copy/ })).toBeNull();
		expect(screen.queryByRole("menu")).toBeNull();
		open(ui, "Assistant message actions");
		expect(screen.getByRole("menu")).toBeTruthy();
	});

	// Catches: the menu copying displayed text instead of the exact Markdown source.
	it("copies the exact message source", () => {
		const ui = view();
		open(ui, "Assistant message actions");
		fireEvent.click(screen.getByRole("menuitem", { name: "Copy message" }));
		expect(writeClipboard).toHaveBeenCalledWith("Because **bold** and `code`.\nSecond line");
		expect(screen.queryByRole("menu")).toBeNull();
	});

	// Catches: raw view replacing the message permanently, or not exposing the Markdown source.
	it("toggles between raw Markdown and the formatted message", () => {
		const ui = view();
		open(ui, "Assistant message actions");
		fireEvent.click(screen.getByRole("menuitem", { name: "Show raw Markdown" }));
		expect(ui.getByLabelText("Raw Markdown").textContent).toBe("Because **bold** and `code`.\nSecond line");
		expect(ui.container.querySelector("strong")).toBeNull();
		open(ui, "Assistant message actions");
		fireEvent.click(screen.getByRole("menuitem", { name: "Show formatted message" }));
		expect(ui.queryByLabelText("Raw Markdown")).toBeNull();
		expect(ui.container.querySelector("strong")?.textContent).toBe("bold");
	});

	// Catches: Reply offered with no composer, or quoting without Markdown quote prefixes.
	it("offers quote reply only when the host has a composer and hands it a quote", () => {
		expect(
			(() => {
				const ui = view();
				open(ui, "Assistant message actions");
				return screen.queryByRole("menuitem", { name: "Quote reply" });
			})(),
		).toBeNull();
		cleanup();
		const onQuote = vi.fn();
		const ui = view(onQuote);
		open(ui, "User message actions");
		fireEvent.click(screen.getByRole("menuitem", { name: "Quote reply" }));
		expect(onQuote).toHaveBeenCalledWith("> Why?\n\n");
	});

	// Catches: feedback sent instead of drafted, wrong verdict, or offered on user messages / no composer.
	it("drafts approve and disapprove feedback with the quoted answer, only for answers", () => {
		const onQuote = vi.fn();
		const ui = view(onQuote);
		open(ui, "Assistant message actions");
		fireEvent.click(screen.getByRole("menuitem", { name: "Disapprove response" }));
		const source = "Because **bold** and `code`.\nSecond line";
		expect(onQuote).toHaveBeenLastCalledWith(feedbackMessage(source, "disapprove"));
		open(ui, "Assistant message actions");
		fireEvent.click(screen.getByRole("menuitem", { name: "Approve response" }));
		expect(onQuote.mock.calls.at(-1)?.[0]).toContain("> Because **bold** and `code`.\n> Second line");
		expect(onQuote.mock.calls.at(-1)?.[0]).toContain("I approve of this response.");
		expect(onQuote.mock.calls.at(-1)?.[0]).toContain("not approval to execute an action");
		cleanup();
		const user = view(vi.fn());
		open(user, "User message actions");
		expect(screen.queryByRole("menuitem", { name: /response/ })).toBeNull();
		cleanup();
		const bare = view();
		open(bare, "Assistant message actions");
		expect(screen.queryByRole("menuitem", { name: /response/ })).toBeNull();
	});

	// Catches: feedback offered on an answer that is still streaming.
	it("hides feedback while the agent is still working", () => {
		const ui = render(() => (
			<Transcript
				entries={() => entries}
				busy={() => true}
				emptyMessage="E"
				observeToolDuration={false}
				onQuote={() => {}}
			/>
		));
		open(ui, "Assistant message actions");
		expect(screen.queryByRole("menuitem", { name: /response/ })).toBeNull();
	});

	// Catches: raw toggle offered for plain user text that has no Markdown rendering.
	it("offers no raw toggle on user messages", () => {
		const ui = view();
		open(ui, "User message actions");
		expect(screen.queryByRole("menuitem", { name: /raw Markdown/ })).toBeNull();
	});

	// Catches: no focus into the menu, dead arrow keys, or focus lost on close.
	it("moves focus with arrow, Home and End keys, and returns it to the trigger", () => {
		const ui = view(() => {});
		const trigger = open(ui, "Assistant message actions");
		return Promise.resolve().then(() => {
			const items = screen.getAllByRole("menuitem");
			expect(items).toHaveLength(5);
			expect(document.activeElement).toBe(items[0]);
			const menu = screen.getByRole("menu");
			fireEvent.keyDown(menu, { key: "ArrowDown" });
			expect(document.activeElement).toBe(items[1]);
			fireEvent.keyDown(menu, { key: "End" });
			expect(document.activeElement).toBe(items[4]);
			fireEvent.keyDown(menu, { key: "ArrowDown" });
			expect(document.activeElement).toBe(items[0]);
			fireEvent.keyDown(menu, { key: "ArrowUp" });
			expect(document.activeElement).toBe(items[4]);
			fireEvent.keyDown(menu, { key: "Home" });
			expect(document.activeElement).toBe(items[0]);
			fireEvent.keyDown(menu, { key: "Enter" });
			expect(writeClipboard).toHaveBeenCalled();
			expect(document.activeElement).toBe(trigger);
		});
	});

	// Catches: Escape closing without restoring focus to the trigger.
	it("closes on Escape and refocuses the trigger", () => {
		const ui = view();
		const trigger = open(ui, "Assistant message actions");
		expect(trigger.getAttribute("aria-expanded")).toBe("true");
		fireEvent.keyDown(document, { key: "Escape" });
		expect(screen.queryByRole("menu")).toBeNull();
		expect(trigger.getAttribute("aria-expanded")).toBe("false");
		expect(document.activeElement).toBe(trigger);
	});
});

describe("quoteMessage", () => {
	// Catches: blank lines breaking the quote block, or no room left to type after it.
	it("prefixes every line and leaves a blank line for the reply", () => {
		expect(quoteMessage("a\n\nb\n")).toBe("> a\n>\n> b\n\n");
	});
});

// Settle the shared UI persistence owner imported by the control utilities.
import { activityStore } from "../../stores/activityStore";

beforeAll(() => activityStore.hydrate());
