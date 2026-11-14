import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";
import { useSessionChatView } from "../components/useSessionChatView";
import { clearDictationPartial, saveDictationPartial } from "../utils/dictationPartial";

const writes: { data: string; at: number }[] = [];
vi.mock("../../invoke", () => ({
	invoke: vi.fn(async () => ({ epoch: 1, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 })),
	listen: vi.fn(async () => () => {}),
}));
vi.mock("../../transport", () => ({
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	rpc: vi.fn((_command: string, args: { data?: string }) => {
		if (args.data !== undefined) writes.push({ data: args.data, at: Date.now() });
		return Promise.resolve(undefined);
	}),
	HttpRpcError: class extends Error {},
}));

import { rpc } from "../../transport";

afterEach(() => {
	cleanup();
	clearDictationPartial("restored-dictation");
	writes.length = 0;
	vi.unstubAllGlobals();
});

// Catches: a restored dictated draft submits only Enter after the desktop clears the PTY line.
it("sends the restored dictated text when the desktop cleared the terminal before reload", async () => {
	// CommandInput's dictation callback persists this text before streaming it to the PTY.
	// The desktop can then clear the line while the PWA is suspended.
	saveDictationPartial("restored-dictation", "review the latest changes");
	const { container, getByRole } = render(() => <CommandInput sessionId="restored-dictation" ptyInputLine="" />);
	expect(container.querySelector("textarea")?.value).toBe("review the latest changes");
	expect(writes).toEqual([]);
	fireEvent.click(getByRole("button", { name: "Send" }));
	await waitFor(() => expect(writes.some((write) => write.data === "\r")).toBe(true));
	expect(writes.map((write) => write.data).join("")).toBe("review the latest changes\r");
});

// Catches: first edit sends only the appended suffix, and later edits replay a stale PTY snapshot.
it("keeps the restored prefix on first edit after the PTY clears and tracks subsequent local edits", () => {
	saveDictationPartial("restored-dictation", "review the latest changes");
	const [ptyLine, setPtyLine] = createSignal<string | null>(null);
	const { container } = render(() => <CommandInput sessionId="restored-dictation" ptyInputLine={ptyLine()} />);
	setPtyLine("");
	expect(writes).toEqual([]);
	const input = container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "review the latest changes now" } });
	fireEvent.input(input, { target: { value: "review the latest changes now please" } });
	expect(writes.map((write) => write.data)).toEqual(["review the latest changes now", " please"]);
});

// Catches: blindly replaying the saved draft duplicates words still present in the PTY.
it("sends only Enter when the restored dictated text is still in the PTY", async () => {
	saveDictationPartial("restored-dictation", "review the latest changes");
	const { getByRole } = render(() => (
		<CommandInput sessionId="restored-dictation" ptyInputLine="review the latest changes" />
	));
	expect(writes).toEqual([]);
	fireEvent.click(getByRole("button", { name: "Send" }));
	await waitFor(() => expect(writes.map((write) => write.data)).toEqual(["\r"]));
});

// Catches: a restored edit appends to a different desktop draft instead of replacing its divergent tail.
it("reconciles the restored first edit with the changed PTY draft", () => {
	saveDictationPartial("restored-dictation", "review the latest changes");
	const { container } = render(() => <CommandInput sessionId="restored-dictation" ptyInputLine="review old" />);
	expect(writes).toEqual([]);
	fireEvent.input(container.querySelector("textarea")!, { target: { value: "review the latest changes now" } });
	expect(writes.map((write) => write.data)).toEqual(["\x7f\x7f\x7fthe latest changes now"]);
});

it("stages an uploaded file path in the PTY draft without pressing Enter", async () => {
	vi.stubGlobal(
		"fetch",
		vi.fn(
			async () =>
				new Response(JSON.stringify({ path: "/repo/.tuic/attachments/1-notes.txt", size: 5 }), {
					status: 200,
					headers: { "content-type": "application/json" },
				}),
		),
	);
	const { container } = render(() => <CommandInput sessionId="disposable" />);
	const picker = container.querySelector('input[type="file"]') as HTMLInputElement;
	expect(picker).toBeTruthy();
	fireEvent.change(picker, { target: { files: [new File(["hello"], "notes.txt", { type: "text/plain" })] } });
	await waitFor(() =>
		expect(container.querySelector("textarea")?.value).toContain("@/repo/.tuic/attachments/1-notes.txt"),
	);
	expect(writes.map((write) => write.data)).toEqual(["@/repo/.tuic/attachments/1-notes.txt"]);
});

describe("mobile slash submission", () => {
	it.each([
		["codex", "/status", 40],
		["claude", "/help", 40],
	])("submits a typed %s slash command after the agent input gap", async (agentType, command, minGap) => {
		const { container } = render(() => <CommandInput sessionId="disposable" agentType={agentType} />);
		const input = container.querySelector("textarea")!;
		fireEvent.input(input, { target: { value: command } });
		fireEvent.click(container.querySelector("button.send")!);
		await waitFor(() => expect(writes.some((write) => write.data === "\r")).toBe(true));
		expect(writes.map((write) => write.data)).toEqual([command, "\r"]);
		expect(writes[1].at - writes[0].at).toBeGreaterThanOrEqual(minGap);
	});

	it.each([
		["codex", "/status", 40],
		["claude", "/help", 40],
	])("submits a %s slash menu pick once with its selected command", async (agentType, command, minGap) => {
		const { container } = render(() => (
			<CommandInput
				sessionId="disposable"
				agentType={agentType}
				slashItems={[{ command, description: "Show status", highlighted: true }]}
			/>
		));
		const input = container.querySelector("textarea")!;
		fireEvent.input(input, { target: { value: command.slice(0, 3) } });
		fireEvent.click(
			Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(command))!,
		);
		fireEvent.click(container.querySelector("button.send")!);
		await waitFor(() => expect(writes.some((write) => write.data === "\r")).toBe(true));
		expect(writes.map((write) => write.data)).toEqual([command.slice(0, 3), command.slice(3) + " ", "\r"]);
		expect(writes[2].at - writes[1].at).toBeGreaterThanOrEqual(minGap);
	});

	it("submits a Claude command with arguments without losing the final word", async () => {
		const { container } = render(() => <CommandInput sessionId="disposable" agentType="claude" />);
		fireEvent.input(container.querySelector("textarea")!, { target: { value: "/model opus" } });
		fireEvent.click(container.querySelector("button.send")!);
		await waitFor(() => expect(writes.some((write) => write.data === "\r")).toBe(true));
		expect(writes.map((write) => write.data)).toEqual(["/model opus", "\r"]);
	});

	// Catches: a failed Enter leaves the PTY holding the command while the composer reads empty,
	// so the next Send is typed onto it ("/help/model sonnet").
	it("keeps the draft in sync with the terminal when Enter never arrives", async () => {
		const line = { text: "" };
		const apply = (data: string) => {
			if (data === "\r") return;
			for (const ch of data) line.text = ch === "\x7f" ? line.text.slice(0, -1) : line.text + ch;
		};
		const original = vi.mocked(rpc).getMockImplementation();
		let rejectEnter = true;
		vi.mocked(rpc).mockImplementation((async (_command: string, args?: { data?: string }) => {
			const data = args?.data;
			if (data === undefined) return undefined;
			writes.push({ data, at: Date.now() });
			if (data === "\r" && rejectEnter) throw new Error("Server unreachable");
			apply(data);
		}) as typeof rpc);
		try {
			const { container } = render(() => <CommandInput sessionId="disposable" agentType="claude" />);
			const input = container.querySelector("textarea")!;
			fireEvent.input(input, { target: { value: "/help" } });
			fireEvent.click(container.querySelector("button.send")!);
			await waitFor(() => expect(writes.filter((write) => write.data === "\r").length).toBe(3), { timeout: 4000 });
			await waitFor(() => expect(input.value).toBe("/help"));
			rejectEnter = false;
			fireEvent.input(input, { target: { value: "/model sonnet" } });
			await waitFor(() => expect(line.text).toBe("/model sonnet"));
			fireEvent.click(container.querySelector("button.send")!);
			await waitFor(() => expect(writes.filter((write) => write.data === "\r").length).toBe(4));
			expect(line.text).toBe("/model sonnet");
		} finally {
			vi.mocked(rpc).mockImplementation(original!);
		}
	});

	it("does not press Enter while the final mobile write is still in flight", async () => {
		let releaseWrite!: () => void;
		const inFlight = new Promise<void>((resolve) => {
			releaseWrite = resolve;
		});
		vi.mocked(rpc).mockImplementationOnce(async (_command, args) => {
			writes.push({ data: args?.data as string, at: Date.now() });
			await inFlight;
			return undefined as never;
		});
		const { container } = render(() => <CommandInput sessionId="disposable" agentType="codex" />);
		fireEvent.input(container.querySelector("textarea")!, { target: { value: "/status" } });
		fireEvent.click(container.querySelector("button.send")!);
		await new Promise((resolve) => setTimeout(resolve, 230));
		expect(writes.map((write) => write.data)).toEqual(["/status"]);
		releaseWrite();
		await waitFor(() => expect(writes.map((write) => write.data)).toEqual(["/status", "\r"]));
	});
});

// Catches: the mobile Send path does not bridge the gap before terminal busy arrives.
it("starts Chat working feedback on mobile Send, not while typing", async () => {
	const [busy, setBusy] = createSignal(false);
	const view = render(() => {
		const chat = useSessionChatView({
			sessionId: () => "mobile-busy-send",
			agentType: () => "claude",
			sessionExists: () => true,
			busy,
			subscribeWake: () => () => {},
		});
		return (
			<>
				<chat.Toggle />
				<chat.View />
				<CommandInput sessionId="mobile-busy-send" agentType="claude" />
			</>
		);
	});
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	await waitFor(() => expect(view.container.textContent).toContain("No messages in this conversation yet."));
	fireEvent.input(view.container.querySelector("textarea")!, { target: { value: "Next task" } });
	expect(view.queryByText("Working…")).toBeNull();
	fireEvent.click(view.getByRole("button", { name: "Send" }));
	expect(view.getByText("Working…")).toBeTruthy();
	await waitFor(() => expect(writes.some((write) => write.data === "\r")).toBe(true));
	setBusy(true);
	expect(view.getByText("Working…")).toBeTruthy();
	setBusy(false);
	expect(view.queryByText("Working…")).toBeNull();
});

// Catches: the restored parked draft reaching the PTY in the same read as the
// Enter, so the agent takes Enter plus the draft as one paste and never submits.
it("restores a parked native draft only after the agent Enter gap", async () => {
	const { container } = render(() => <CommandInput sessionId="park-native-gap" agentType="claude" />);
	const input = container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "parked draft" } });
	fireEvent.keyDown(input, { key: "s", ctrlKey: true });
	fireEvent.input(input, { target: { value: "now" } });
	writes.length = 0;
	fireEvent.click(container.querySelector("button.send")!);
	await waitFor(() => expect(input.value).toBe("parked draft"));
	const enter = writes.findIndex((write) => write.data === "\r");
	const restore = writes.findIndex((write, index) => index > enter && write.data.includes("parked draft"));
	expect(enter).toBeGreaterThanOrEqual(0);
	expect(restore).toBeGreaterThan(enter);
	expect(writes[restore].at - writes[enter].at).toBeGreaterThanOrEqual(40);
});
