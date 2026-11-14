// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});

// Catches: identical response excerpts being used as identity, or a render key sent instead of the native ID.
it("routes reaction and comment from repeated answers using the selected native source ID", async () => {
	vi.spyOn(console, "warn").mockImplementation((...args) => {
		throw new Error(args.join(" "));
	});
	const send = vi.fn(async () => {});
	const entries: AcpTranscriptEntry[] = [
		{ id: "render-a", messageId: "native-a", kind: "agent", text: "Same answer." },
		{ id: "render-b", messageId: "native-b", kind: "agent", text: "Same answer." },
	];
	const ui = render(() => (
		<Transcript
			entries={() => entries}
			busy={() => false}
			emptyMessage=""
			observeToolDuration={false}
			messageActions={{ reactions: () => ({}), send }}
		/>
	));
	fireEvent.click(ui.getAllByRole("button", { name: "Assistant message actions" })[1]);
	fireEvent.click(screen.getByRole("menuitem", { name: "Heart" }));
	await waitFor(() =>
		expect(send).toHaveBeenCalledWith("native-b", "Same answer.", {
			kind: "reaction",
			reaction: "heart",
			remove: false,
		}),
	);
	fireEvent.click(ui.getAllByRole("button", { name: "Assistant message actions" })[0]);
	fireEvent.click(screen.getByRole("menuitem", { name: "Comment on message" }));
	fireEvent.input(ui.getByRole("textbox", { name: "Comment on this message" }), {
		target: { value: "A comment for the first answer." },
	});
	fireEvent.click(ui.getByRole("button", { name: "Send comment" }));
	await waitFor(() =>
		expect(send).toHaveBeenLastCalledWith("native-a", "Same answer.", {
			kind: "comment",
			comment: "A comment for the first answer.",
		}),
	);
});

// Settle the shared UI persistence owner imported by the control utilities.
import { activityStore } from "../../stores/activityStore";

beforeAll(() => activityStore.hydrate());
