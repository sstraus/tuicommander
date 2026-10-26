// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, hydrate, createAcpChat, answerPermission, selectSession } = vi.hoisted(() => ({
	invoke: vi.fn(),
	hydrate: vi.fn(async () => {}),
	createAcpChat: vi.fn(),
	answerPermission: vi.fn(async () => {}),
	selectSession: vi.fn(async () => {}),
}));
vi.mock("../../invoke", () => ({ invoke }));
vi.mock("../../stores/settings", () => ({ settingsStore: { hydrate, state: { chatMuteIntermediateText: true } } }));
vi.mock("../../components/AIChatPanel/useAcpChat", () => ({ createAcpChat }));

import { aiChatDraft } from "../../components/AIChatPanel/draft";
import { MobileChatScreen } from "../screens/MobileChatScreen";

function chat() {
	return {
		phase: () => "live",
		root: () => "/home/boss/Gits",
		connectionId: () => "connection-1",
		sessionId: () => "current",
		entries: () => [
			{ id: "u", kind: "user", text: "Check the build" },
			{ id: "a", kind: "agent", text: "The build passed." },
			{ id: "t", kind: "tool", call: { toolCallId: "tool-1", title: "Run tests", status: "completed" } },
		],
		busy: () => false,
		queuedPrompts: () => [] as { turnId: string; summary: string }[],
		held: () => false,
		gap: () => null,
		error: () => null,
		isStreaming: () => true,
		interactions: () => [
			{
				kind: "permission",
				requestId: "permission-1",
				sessionId: "current",
				request: {
					toolCall: { title: "Write report" },
					options: [{ optionId: "allow", name: "Allow once", kind: "allow_once" }],
				},
			},
		],
		sessions: () => [
			{ sessionId: "current", cwd: "/repo", title: "Current work" },
			{ sessionId: "previous", cwd: "/repo", title: "Earlier review" },
		],
		capabilities: () => ({ list: true, load: true }),
		configOptions: () => [],
		answerPermission,
		selectSession,
		answerElicitation: vi.fn(),
		cancelPermission: vi.fn(),
		startSession: vi.fn(),
		ensureStarted: vi.fn(async () => {}),
		send: vi.fn(),
		cancel: vi.fn(),
		cancelQueued: vi.fn(),
		recover: vi.fn(),
	};
}

beforeEach(() => {
	aiChatDraft.reset();
	history.replaceState(null, "", "/mobile");
	invoke.mockReset();
	createAcpChat.mockReset().mockReturnValue(chat());
	answerPermission.mockClear();
	selectSession.mockClear();
});
afterEach(() => {
	cleanup();
	history.replaceState(null, "", "/mobile");
	vi.unstubAllGlobals();
});

describe("mobile ego chat", () => {
	// Catches: failed settings reads are presented as an unconfigured ego installation forever.
	it("waits for settings and retries them on recovery before claiming ego is unconfigured", async () => {
		hydrate.mockResolvedValueOnce(false as never).mockResolvedValueOnce(true as never);
		createAcpChat.mockReturnValue({ ...chat(), phase: () => "unconfigured", entries: () => [] });
		const [revision, setRevision] = createSignal(0);
		render(() => <MobileChatScreen recoveryRevision={revision()} />);
		await waitFor(() => expect(screen.getByText(/Chat settings are unavailable/)).toBeTruthy());
		expect(screen.queryByText(/Configure ego in desktop Settings/)).toBeNull();
		setRevision(1);
		await waitFor(() => expect(screen.getByText(/Configure ego in desktop Settings/)).toBeTruthy());
		expect(hydrate).toHaveBeenCalledWith({ notifyOnError: false });
	});
	it("keeps mobile composer actions as labelled icons with the session send glyph", () => {
		const { container } = render(() => <MobileChatScreen />);
		const textarea = container.querySelector("textarea");
		const send = screen.getByRole("button", { name: "Send" });
		expect(send.textContent?.trim()).toBe("");
		expect(send.querySelector("svg path")?.getAttribute("d")).toBe("M2.01 21L23 12 2.01 3 2 10l15 2-15 2z");

		const plus = screen.getByRole("button", { name: "More composer actions" });
		expect((plus.compareDocumentPosition(textarea!) ?? 0) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		expect(screen.queryByRole("button", { name: "Park draft" })).toBeNull();
		fireEvent.click(plus);
		expect(screen.getByRole("menuitem", { name: "Attach file" })).toBeTruthy();
		expect(screen.getByRole("menuitem", { name: "Park draft" })).toBeDisabled();
	});
	it("sends once on Enter and leaves Shift+Enter for a newline on mobile", () => {
		const current = chat();
		createAcpChat.mockReturnValue(current);
		const { container } = render(() => <MobileChatScreen />);
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		fireEvent.input(textarea, { target: { value: "Phone reply" } });
		fireEvent.keyDown(textarea, { key: "Enter", shiftKey: true });
		expect(current.send).not.toHaveBeenCalled();
		fireEvent.keyDown(textarea, { key: "Enter" });
		expect(current.send).toHaveBeenCalledExactlyOnceWith("Phone reply", [], []);
	});

	it("passes a tapped transcript file link with the chat workspace to mobile navigation", async () => {
		const onOpenFile = vi.fn();
		createAcpChat.mockReturnValue({
			...chat(),
			entries: () => [{ id: "report", kind: "agent", text: "Read docs/guide.md" }],
		});
		render(() => <MobileChatScreen onOpenFile={onOpenFile} />);
		fireEvent.click(await screen.findByRole("link", { name: "docs/guide.md" }));
		expect(onOpenFile).toHaveBeenCalledWith("docs/guide.md", "/home/boss/Gits");
	});
	it("shows a message instead of guessing a path when the chat workspace is unavailable", async () => {
		const onOpenFile = vi.fn();
		createAcpChat.mockReturnValue({
			...chat(),
			root: () => null,
			entries: () => [{ id: "report", kind: "agent", text: "Read docs/guide.md" }],
		});
		render(() => <MobileChatScreen onOpenFile={onOpenFile} />);
		fireEvent.click(await screen.findByRole("link", { name: "docs/guide.md" }));
		expect(screen.getByRole("alert").textContent).toContain("workspace");
		expect(onOpenFile).not.toHaveBeenCalled();
	});

	it("opens global chat without a repository selection or repository fetch", async () => {
		render(() => <MobileChatScreen />);
		expect(screen.queryByRole("combobox", { name: "Repository" })).toBeNull();
		expect(screen.getByRole("combobox", { name: "Conversation" })).toBeTruthy();
		expect(invoke).not.toHaveBeenCalledWith("load_repositories");
		expect(createAcpChat.mock.calls[0][0]()).toBeNull();
	});

	it("opens a linked conversation without a repository in the URL", async () => {
		history.replaceState(null, "", "/mobile?session=previous");
		render(() => <MobileChatScreen />);
		await waitFor(() => expect(selectSession).toHaveBeenCalledWith("previous"));
	});

	it("opens a push-linked conversation with its repository as a context hint", async () => {
		history.replaceState(null, "", "/mobile?repo=%2Frepo&session=previous");
		render(() => <MobileChatScreen />);
		expect(screen.queryByRole("combobox", { name: "Repository" })).toBeNull();
		expect(createAcpChat.mock.calls[0][0]()).toBe("/repo");
		await waitFor(() => expect(selectSession).toHaveBeenCalledWith("previous"));
	});

	// The chat is global and nothing starts ego before the first message, so a
	// link must not wait for a live connection or a listed session.
	it("selects a linked conversation before ego is running, and lets the first message start it", async () => {
		history.replaceState(null, "", "/mobile?repo=%2Frepo&session=previous");
		createAcpChat.mockReturnValue({
			...chat(),
			phase: () => "ready",
			connectionId: () => null,
			sessionId: () => null,
			sessions: () => [],
		});
		const { container } = render(() => <MobileChatScreen />);
		await waitFor(() => expect(selectSession).toHaveBeenCalledWith("previous"));
		expect(container.querySelector("textarea")).toBeTruthy();
	});

	it("opens a push-linked conversation without loading a local repository list", async () => {
		history.replaceState(null, "", "/mobile?repo=%2Funknown&session=previous");
		render(() => <MobileChatScreen />);
		await waitFor(() => expect(selectSession).toHaveBeenCalledWith("previous"));
		expect(invoke).not.toHaveBeenCalledWith("load_repositories");
	});

	it("shows the conversation, card and collapsed activity on direct open", async () => {
		const { container } = render(() => <MobileChatScreen />);
		await waitFor(() => expect(screen.getByText("The build passed.")).toBeTruthy());
		expect(screen.getByText("Check the build")).toBeTruthy();
		const activity = screen.getByText(/1 tool call/).closest("details");
		expect(activity?.open).toBe(false);
		expect(activity?.textContent).toContain("Run tests");
		expect(container.querySelector("textarea")).toBeTruthy();
	});

	it("offers a regular picker and a separate camera capture picker", () => {
		const { container } = render(() => <MobileChatScreen />);
		const pickers = container.querySelectorAll('input[type="file"]');
		expect(pickers).toHaveLength(2);
		expect(pickers[1].getAttribute("capture")).toBe("environment");
		expect(pickers[0]).toBeInstanceOf(HTMLInputElement);
		expect(pickers[0].hasAttribute("capture")).toBe(false);
		expect(pickers[0].getAttribute("accept")).toContain("image/*");
	});

	it("keeps a shared document in the draft until Send", async () => {
		const current = chat();
		createAcpChat.mockReturnValue(current);
		vi.stubGlobal(
			"fetch",
			vi.fn(
				async () =>
					new Response(JSON.stringify({ path: "/repo/.tuic/attachments/1-report.pdf", size: 5 }), { status: 200 }),
			),
		);
		const { container } = render(() => <MobileChatScreen />);
		const picker = container.querySelector('input[type="file"]') as HTMLInputElement;
		fireEvent.click(screen.getByRole("button", { name: "More composer actions" }));
		const openPicker = vi.spyOn(picker, "click");
		fireEvent.click(screen.getByRole("menuitem", { name: "Attach file" }));
		expect(openPicker).toHaveBeenCalledOnce();
		fireEvent.change(picker, { target: { files: [new File(["report"], "report.pdf", { type: "application/pdf" })] } });
		await waitFor(() => expect(screen.getByText("report.pdf")).toBeTruthy());
		expect(current.send).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "Send" }));
		expect(current.send).toHaveBeenCalledWith(
			"",
			[],
			[{ name: "report.pdf", path: "/repo/.tuic/attachments/1-report.pdf" }],
		);
	});

	// Catches: a shell cache version bump leaves the mobile reader on an empty old cache.
	it("round-trips the service worker share target into the mobile draft despite cache version changes", async () => {
		const current = chat();
		createAcpChat.mockReturnValue(current);
		const stores = new Map<string, Map<string, Response>>();
		const caches = {
			open: async (name: string) => {
				if (!stores.has(name)) stores.set(name, new Map());
				const entries = stores.get(name)!;
				return {
					put: async (key: string, response: Response) => {
						entries.set(key, response);
					},
					match: async (key: string) => entries.get(key)?.clone(),
					delete: async (key: string) => entries.delete(key),
				};
			},
		};
		vi.stubGlobal("caches", caches);
		const handlers = new Map<string, (event: unknown) => void>();
		runInNewContext(readFileSync("public/sw.js", "utf8"), {
			self: { addEventListener: (type: string, handler: (event: unknown) => void) => handlers.set(type, handler) },
			caches,
			Request,
			Response,
			URL,
			crypto: { randomUUID: () => "shared-1" },
		});
		let response: Promise<Response> | undefined;
		handlers.get("fetch")!({
			request: {
				url: "https://tuic.example/mobile/share",
				method: "POST",
				formData: async () =>
					new Map([["attachment", new File(["report"], "report.pdf", { type: "application/pdf" })]]),
			},
			respondWith: (result: Promise<Response>) => {
				response = result;
			},
		});
		const redirect = await response!;
		expect(redirect.status).toBe(303);
		history.replaceState(null, "", `${redirect.headers.get("location")}&session=current`);
		const upload = vi.fn(async (_input: unknown, options?: RequestInit) => {
			const file = options?.body as File;
			expect(file.name).toBe("report.pdf");
			expect(file.type).toBe("application/pdf");
			return new Response(JSON.stringify({ path: "/repo/.tuic/attachments/2-report.pdf", size: 6 }), { status: 200 });
		});
		vi.stubGlobal("fetch", upload);
		render(() => <MobileChatScreen />);
		await waitFor(() => expect(screen.getByText("report.pdf")).toBeTruthy());
		expect(upload).toHaveBeenCalledOnce();
		expect(current.send).not.toHaveBeenCalled();
		expect([...stores.values()].some((entries) => entries.has("/_shared/shared-1"))).toBe(false);
		expect(location.search).toBe("?session=current");
	});

	// Catches: a consumed or expired share silently disappears instead of explaining the missing file.
	it("shows an unavailable message for a missing shared file without consuming the URL", async () => {
		history.replaceState(null, "", "/mobile?shared=missing-1");
		const remove = vi.fn();
		vi.stubGlobal("caches", { open: async () => ({ match: async () => undefined, delete: remove }) });
		render(() => <MobileChatScreen />);
		await waitFor(() => expect(screen.getByRole("alert").textContent).toBe("Shared file is no longer available."));
		expect(remove).not.toHaveBeenCalled();
		expect(location.search).toBe("?shared=missing-1");
	});

	it("sends one answer when the permission button is tapped twice", async () => {
		render(() => <MobileChatScreen />);
		const button = await screen.findByRole("button", { name: "Allow once" });
		fireEvent.click(button);
		fireEvent.click(button);
		expect(answerPermission).toHaveBeenCalledTimes(1);
		expect(answerPermission).toHaveBeenCalledWith("permission-1", "allow");
	});

	it("lists titled conversations and resumes the selected one", async () => {
		render(() => <MobileChatScreen />);
		const picker = await screen.findByRole("combobox", { name: "Conversation" });
		expect(screen.getByRole("option", { name: "Earlier review" })).toBeTruthy();
		fireEvent.change(picker, { target: { value: "previous" } });
		expect(selectSession).toHaveBeenCalledWith("previous");
	});

	it("shows the shared queue and cancels a selected prompt from phone", async () => {
		const shared = chat();
		shared.busy = () => true;
		shared.queuedPrompts = () => [{ turnId: "phone-turn", summary: "Follow up from desktop" }];
		createAcpChat.mockReturnValue(shared);
		render(() => <MobileChatScreen />);

		fireEvent.click(screen.getByRole("button", { name: "More composer actions" }));
		expect(screen.getByRole("menuitem", { name: "Queue" })).toBeDisabled();
		fireEvent.click(screen.getByRole("menuitem", { name: "Stop" }));
		expect(shared.cancel).toHaveBeenCalledOnce();
		fireEvent.click(screen.getByRole("button", { name: "Cancel queued prompt Follow up from desktop" }));
		expect(shared.cancelQueued).toHaveBeenCalledWith("phone-turn");
	});

	it("parks and restores a phone draft using the composer control", async () => {
		const { container } = render(() => <MobileChatScreen />);
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		fireEvent.input(textarea, { target: { value: "Phone draft" } });
		fireEvent.click(screen.getByRole("button", { name: "More composer actions" }));
		fireEvent.click(screen.getByRole("menuitem", { name: "Park draft" }));
		expect(textarea.value).toBe("");
		expect(screen.queryByRole("menu")).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "More composer actions" }));
		fireEvent.click(screen.getByRole("menuitem", { name: "Restore parked draft" }));
		expect(textarea.value).toBe("Phone draft");
	});

	it("parks an uploaded document and restores it as an unsent attachment", async () => {
		const current = chat();
		createAcpChat.mockReturnValue(current);
		vi.stubGlobal(
			"fetch",
			vi.fn(
				async () =>
					new Response(JSON.stringify({ path: "/repo/.tuic/attachments/1-report.pdf", size: 6 }), { status: 200 }),
			),
		);
		const { container } = render(() => <MobileChatScreen />);
		const picker = container.querySelector('input[type="file"]') as HTMLInputElement;
		fireEvent.change(picker, { target: { files: [new File(["report"], "report.pdf", { type: "application/pdf" })] } });
		await waitFor(() => expect(screen.getByText("report.pdf")).toBeTruthy());
		fireEvent.click(screen.getByRole("button", { name: "More composer actions" }));
		fireEvent.click(screen.getByRole("menuitem", { name: "Park draft" }));
		expect(screen.queryByText("report.pdf")).toBeNull();
		expect(current.send).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "More composer actions" }));
		fireEvent.click(screen.getByRole("menuitem", { name: "Restore parked draft" }));
		expect(screen.getByText("report.pdf")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Send" }));
		expect(current.send).toHaveBeenCalledWith(
			"",
			[],
			[{ name: "report.pdf", path: "/repo/.tuic/attachments/1-report.pdf" }],
		);
	});
	// The browser transport throws `RPC <command> failed: <status> <body>`; this is the body Boss saw on his iPhone.
	const UNCERTAIN_LOAD =
		"RPC acp_session_load failed: 502 " +
		JSON.stringify({
			code: "agent_error",
			message:
				"the session's last run has an uncertain effect; close it with `session/close` and start a new chat with `session/new`",
			connectionId: "01a12ce9-7cae-72bd-b367-1b7e3fcf4ce7",
			sessionId: null,
			operation: "load",
			retryable: false,
		});

	it("shows a plain message and Start new chat, not raw JSON and Retry, for a refused uncertain session", async () => {
		const current = {
			...chat(),
			error: () => UNCERTAIN_LOAD,
			closeFailed: vi.fn(async () => {}),
			startSession: vi.fn(),
		};
		createAcpChat.mockReturnValue(current);
		const { container } = render(() => <MobileChatScreen />);
		expect(container.textContent).not.toContain("agent_error");
		expect(container.textContent).not.toContain("connectionId");
		expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Start new chat" }));
		await waitFor(() => expect(current.startSession).toHaveBeenCalledOnce());
		expect(current.closeFailed).toHaveBeenCalledOnce();
		expect(current.closeFailed.mock.invocationCallOrder[0]).toBeLessThan(
			current.startSession.mock.invocationCallOrder[0],
		);
	});

	it("keeps Retry for a retryable refusal and shows its own message", () => {
		const body = JSON.stringify({ code: "transport_closed", message: "connection dropped", retryable: true });
		const current = { ...chat(), error: () => `RPC acp_session_load failed: 502 ${body}` };
		createAcpChat.mockReturnValue(current);
		render(() => <MobileChatScreen />);
		expect(screen.getByText(/connection dropped/)).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		expect(current.recover).toHaveBeenCalledOnce();
		expect(screen.queryByRole("button", { name: "Start new chat" })).toBeNull();
	});

	it("keeps Retry for an error that carries no structured body", () => {
		createAcpChat.mockReturnValue({ ...chat(), error: () => "Saved conversation unavailable" });
		render(() => <MobileChatScreen />);
		expect(screen.getByText(/Saved conversation unavailable/)).toBeTruthy();
		expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
	});

	it("shows a non-retryable refusal without Retry or the new-chat action", () => {
		const body = JSON.stringify({ code: "invalid_input", message: "workspace is not allowed", retryable: false });
		createAcpChat.mockReturnValue({ ...chat(), error: () => `RPC acp_session_new failed: 400 ${body}` });
		render(() => <MobileChatScreen />);
		expect(screen.getByText(/workspace is not allowed/)).toBeTruthy();
		expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
		expect(screen.queryByRole("button", { name: "Start new chat" })).toBeNull();
	});
});

// Catches: mobile Chat omitting table actions or opening a document that cannot return to the conversation.
it("opens a table document and returns to Chat without changing its conversation", async () => {
	const source = "| Storia | Priorità | Cosa manca |\n| --- | --- | --- |\n| #1918 | P2 | Aggiornare la sessione |";
	createAcpChat.mockReturnValue({ ...chat(), entries: () => [{ id: "table", kind: "agent", text: source }] });
	const view = render(() => <MobileChatScreen />);
	await Promise.resolve();
	fireEvent.click(view.getByRole("button", { name: "Open as document" }));
	expect(view.getByRole("dialog", { name: "AI Chat table" })).toBeTruthy();
	fireEvent.click(view.getByRole("button", { name: "Back to Chat" }));
	expect(view.queryByRole("dialog")).toBeNull();
	expect(selectSession).not.toHaveBeenCalled();
});

// Catches: Chat changes resize the mobile composer or screen chrome.
it("scopes the Chat size to the AI Chat transcript, excluding its composer", async () => {
	const { mobileFontSizes } = await import("../mobileFontSize");
	while (mobileFontSizes.chat() < 20) mobileFontSizes.step("chat", 1);
	const view = render(() => <MobileChatScreen />);
	const reading = view.container.querySelector("[data-mobile-reading]") as HTMLElement;
	expect(reading.style.getPropertyValue("--font-base")).toBe("20px");
	const composer = await screen.findByRole("textbox");
	expect(composer.closest("[data-mobile-reading]")).toBeNull();
	expect(view.container.querySelector("header")?.closest("[data-mobile-reading]")).toBeNull();
	while (mobileFontSizes.chat() > 14) mobileFontSizes.step("chat", -1);
});
