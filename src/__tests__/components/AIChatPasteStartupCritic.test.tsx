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

it.each(["switch", "clear", "park", "start"])("keeps startup-delayed paste ownership across %s", async (action) => {
	aiChatDraft.reset();
	const [sessionId, setSessionId] = createSignal(action === "start" ? "" : "original-chat");
	let connected = false;
	let finishStart!: (session: string) => void;
	const starting = new Promise<string>((resolve) => {
		finishStart = resolve;
	});
	const chat = {
		sessionId,
		busy: () => false,
		queuedPrompts: () => [],
		capabilities: () => (connected ? { promptImage: true } : null),
		ensureStarted: () => starting,
	} as unknown as AcpChat;
	vi.spyOn(FileReader.prototype, "readAsDataURL").mockImplementation(function (this: FileReader) {
		Object.defineProperty(this, "result", { value: "data:image/png;base64,cG5n" });
		this.onload?.(new ProgressEvent("load") as ProgressEvent<FileReader>);
	});
	const { container } = render(() => <Composer chat={chat} />);
	const file = new File(["png"], "screenshot.png", { type: "image/png" });
	fireEvent.paste(container.querySelector("textarea")!, {
		clipboardData: {
			items: [{ type: "image/png", getAsFile: () => file }],
			getData: () => "",
		},
	});
	if (action === "switch") setSessionId("other-chat");
	if (action === "clear") aiChatDraft.clear();
	if (action === "park") {
		aiChatDraft.set("park this draft");
		aiChatDraft.parkOrSwap();
	}
	if (action === "start") setSessionId("created-chat");
	connected = true;
	finishStart(action === "start" ? "created-chat" : "original-chat");
	await starting;
	await Promise.resolve();
	await Promise.resolve();
	expect(aiChatDraft.images()).toHaveLength(action === "start" ? 1 : 0);
});
