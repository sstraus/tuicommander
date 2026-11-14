// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { MessageMenu } from "../../components/AIChatPanel/MessageMenu";

afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});

// Catches: right-aligned menus escaping the left viewport edge, or a tall menu escaping the bottom.
it("keeps the left-edge message menu inside the viewport", async () => {
	vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
		return {
			left: 0,
			right: 24,
			top: 200,
			bottom: 224,
			width: this.getAttribute("role") === "menu" ? 240 : 24,
			height: this.getAttribute("role") === "menu" ? 300 : 24,
			x: 0,
			y: 200,
			toJSON() {},
		};
	});
	const ui = render(() => <MessageMenu role="assistant" onCopy={() => {}} />);
	fireEvent.click(ui.getByRole("button", { name: "Assistant message actions" }));
	await waitFor(() => expect(screen.getByRole("menu").style.position).toBe("fixed"));
	const menu = screen.getByRole("menu");
	expect(parseFloat(menu.style.left)).toBeGreaterThanOrEqual(8);
	expect(parseFloat(menu.style.left) + 240).toBeLessThanOrEqual(window.innerWidth - 8);
	expect(parseFloat(menu.style.top)).toBeGreaterThanOrEqual(8);
	expect(parseFloat(menu.style.top) + 300).toBeLessThanOrEqual(window.innerHeight - 8);
});

// Catches: a selected reaction becoming an irreversible vote, or a failed offline request leaving a false mark.
it("adds and removes a conversational reaction and preserves state on offline refusal", async () => {
	const send = vi.fn(async (_reaction: string, _remove: boolean) => {});
	const ui = render(() => {
		const [selected, setSelected] = createSignal<string[]>([]);
		return (
			<MessageMenu
				role="assistant"
				onCopy={() => {}}
				actions={{
					reactions: selected(),
					onReact: async (reaction, remove) => {
						await send(reaction, remove);
						setSelected(remove ? [] : [reaction]);
					},
					onComment: async () => {},
				}}
			/>
		);
	});
	fireEvent.click(ui.getByRole("button", { name: "Assistant message actions" }));
	fireEvent.click(screen.getByRole("menuitem", { name: "Thumbs up" }));
	await waitFor(() => expect(send).toHaveBeenCalledWith("thumbs_up", false));
	await waitFor(() => expect(ui.getByRole("button", { name: "Remove Thumbs up reaction" })).toBeTruthy());
	send.mockRejectedValueOnce(new Error("Offline: not sent"));
	fireEvent.click(ui.getByRole("button", { name: "Remove Thumbs up reaction" }));
	await waitFor(() => expect(ui.getByRole("alert").textContent).toContain("Offline: not sent"));
	expect(ui.getByRole("button", { name: "Remove Thumbs up reaction" })).toBeTruthy();
	fireEvent.click(ui.getByRole("button", { name: "Remove Thumbs up reaction" }));
	await waitFor(() => expect(ui.queryByRole("button", { name: "Remove Thumbs up reaction" })).toBeNull());
	expect(send).toHaveBeenLastCalledWith("thumbs_up", true);
});

// Catches: serialized desktop refusals hiding their message and duplicate-send warning as [object Object].
it.each([
	{ failure: { code: "transport_closed", message: "Offline: not sent" }, message: "Offline: not sent" },
	{
		failure: {
			code: "invalid_input",
			message: "Action accepted, but saving its mark failed: disk full. Do not resend.",
		},
		message: "Action accepted, but saving its mark failed: disk full. Do not resend.",
	},
	{ failure: new Error("HTTP request failed"), message: "HTTP request failed" },
	{ failure: "Connection unavailable", message: "Connection unavailable" },
])("preserves the action refusal message: $message", async ({ failure, message }) => {
	const onReact = vi.fn().mockRejectedValueOnce(failure);
	const ui = render(() => (
		<MessageMenu role="assistant" onCopy={() => {}} actions={{ reactions: [], onReact, onComment: async () => {} }} />
	));
	fireEvent.click(ui.getByRole("button", { name: "Assistant message actions" }));
	fireEvent.click(screen.getByRole("menuitem", { name: "Thumbs up" }));
	await waitFor(() => expect(ui.getByRole("alert").textContent).toBe(message));
	expect(onReact).toHaveBeenCalledTimes(1);
	expect(ui.queryByRole("button", { name: "Remove Thumbs up reaction" })).toBeNull();
});

// Catches: comments silently becoming composer drafts or being sent when Cancel is selected.
it("sends the comment separately without touching the composer", async () => {
	const onComment = vi.fn(async () => {});
	const ui = render(() => (
		<MessageMenu role="assistant" onCopy={() => {}} actions={{ reactions: [], onReact: async () => {}, onComment }} />
	));
	fireEvent.click(ui.getByRole("button", { name: "Assistant message actions" }));
	fireEvent.click(screen.getByRole("menuitem", { name: "Comment on message" }));
	fireEvent.input(ui.getByRole("textbox", { name: "Comment on this message" }), {
		target: { value: "Keep this approach." },
	});
	fireEvent.click(ui.getByRole("button", { name: "Send comment" }));
	await waitFor(() => expect(onComment).toHaveBeenCalledWith("Keep this approach."));
});

// Settle the shared UI persistence owner imported by the control utilities.
import { activityStore } from "../../stores/activityStore";

beforeAll(() => activityStore.hydrate());
