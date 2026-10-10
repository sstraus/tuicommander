import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { Composer } from "../../components/AIChatPanel/Composer";
import { aiChatDraft } from "../../components/AIChatPanel/draft";
import type { AcpChat } from "../../components/AIChatPanel/useAcpChat";

afterEach(() => {
	cleanup();
	aiChatDraft.reset();
	vi.restoreAllMocks();
});

it.each([
	{ initial: "", count: 1, bug: "an unstarted paste is adopted by a different selected tab" },
	{ initial: "original-chat", count: 2, bug: "the second image of a cancelled paste leaks into another tab" },
])("prevents $bug", async ({ initial, count }) => {
	aiChatDraft.reset();
	const [sessionId, setSessionId] = createSignal(initial);
	let connected = false;
	let finish!: () => void;
	const startup = new Promise<void>((resolve) => {
		finish = resolve;
	});
	const chat = {
		sessionId,
		busy: () => false,
		queuedPrompts: () => [],
		capabilities: () => (connected ? { promptImage: true } : null),
		ensureStarted: () => startup,
	} as unknown as AcpChat;
	vi.spyOn(FileReader.prototype, "readAsDataURL").mockImplementation(function (this: FileReader) {
		Object.defineProperty(this, "result", { value: "data:image/png;base64,cG5n" });
		this.onload?.(new ProgressEvent("load") as ProgressEvent<FileReader>);
	});
	const view = render(() => <Composer chat={chat} />);
	fireEvent.paste(view.container.querySelector("textarea")!, {
		clipboardData: {
			items: Array.from({ length: count }, () => ({
				type: "image/png",
				getAsFile: () => new File(["png"], "clip.png", { type: "image/png" }),
			})),
			getData: () => "",
		},
	});
	setSessionId("other-chat");
	connected = true;
	finish();
	await startup;
	for (let i = 0; i < 8; i++) await Promise.resolve();
	expect(view.queryAllByAltText("Pasted image")).toHaveLength(0);
});
