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

it("does not lose a second paste when capabilities arrive before the initial session", async () => {
	aiChatDraft.reset();
	const [sessionId, setSessionId] = createSignal<string | null>(null);
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
	const finishReads: (() => void)[] = [];
	vi.spyOn(FileReader.prototype, "readAsDataURL").mockImplementation(function (this: FileReader) {
		finishReads.push(() => {
			Object.defineProperty(this, "result", { value: "data:image/png;base64,cG5n" });
			this.onload?.(new ProgressEvent("load") as ProgressEvent<FileReader>);
		});
	});
	const view = render(() => <Composer chat={chat} />);
	const paste = () =>
		fireEvent.paste(view.container.querySelector("textarea")!, {
			clipboardData: {
				items: [{ type: "image/png", getAsFile: () => new File(["png"], "clip.png", { type: "image/png" }) }],
				getData: () => "",
			},
		});
	paste();
	// connect resolves before newSession in createAcpChat.launch.
	connected = true;
	paste();
	setSessionId("created-chat");
	finishStart("created-chat");
	await starting;
	for (let i = 0; i < 8; i++) await Promise.resolve();
	for (const finish of finishReads) finish();
	for (let i = 0; i < 8; i++) await Promise.resolve();
	expect(view.queryAllByAltText("Pasted image")).toHaveLength(2);
});
