// @vitest-environment jsdom
//
// The panel renders an agent's answer through ContentRenderer, whose DOMPurify
// pass needs a complete NodeIterator; happy-dom's is not.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanupToasts } from "../helpers/toasts";

const { mockDetachPanel, mockReattachPanel, mockClosePanel } = vi.hoisted(() => ({
	mockDetachPanel: vi.fn().mockResolvedValue(undefined),
	mockReattachPanel: vi.fn().mockResolvedValue(undefined),
	mockClosePanel: vi.fn().mockResolvedValue(undefined),
}));

const { mockWriteClipboard, mockOpenFile, mockOpenUrl } = vi.hoisted(() => ({
	mockWriteClipboard: vi.fn().mockResolvedValue(undefined),
	mockOpenFile: vi.fn(),
	mockOpenUrl: vi.fn(),
}));
const { mockSetFolderRoot, mockShowFileBrowser } = vi.hoisted(() => ({
	mockSetFolderRoot: vi.fn(),
	mockShowFileBrowser: vi.fn(),
}));

vi.mock("../../utils/clipboard", () => ({ writeClipboard: mockWriteClipboard }));
vi.mock("../../utils/filePreview", () => ({ openTerminalFilePath: mockOpenFile }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: mockOpenUrl }));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	Channel: vi.fn(),
	convertFileSrc: (path: string) => path,
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
	emitTo: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../panelRouter", () => ({
	detachPanel: mockDetachPanel,
	reattachPanel: mockReattachPanel,
	closePanel: mockClosePanel,
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../stores/ui", () => ({
	uiStore: {
		state: { detachedPanels: {} },
		isDetached: vi.fn(() => false),
		setDetached: vi.fn(),
		clearDetached: vi.fn(),
		setAiChatPanelMeasuredWidth: vi.fn(),
		setFileBrowserExternalRoot: mockSetFolderRoot,
		setFileBrowserPanelVisible: mockShowFileBrowser,
	},
}));

vi.mock("../../transport", () => ({
	isTauri: () => true,
	owningConnectionFor: () => undefined,
}));

// Whether an ego binary is configured is the one setting this panel reads, and
// criterion 9 turns on it being readable as "not configured" rather than as an
// empty string nobody checked.
const settings = vi.hoisted(() => ({ egoExecutable: "/usr/local/bin/ego" }));

vi.mock("../../stores/settings", async () => {
	const { createSignal } = await import("solid-js");
	const [executable, setExecutable] = createSignal(settings.egoExecutable);
	Object.defineProperty(settings, "egoExecutable", { get: executable, set: setExecutable });
	return {
		settingsStore: {
			state: settings,
			isAiChatEnabled: () => true,
			isAcpConfigured: () => settings.egoExecutable.trim().length > 0,
		},
	};
});

// The client is the IPC boundary and the only thing mocked below it: the store,
// the transcript projection and every reducer between them are the real ones,
// so a test that passes proves the panel reads what the wire actually carries.
const client = vi.hoisted(() => ({
	openConversation: vi.fn(),
	connect: vi.fn(),
	reconnect: vi.fn(),
	disconnect: vi.fn(),
	newSession: vi.fn(),
	loadSession: vi.fn(),
	listSessions: vi.fn(),
	prompt: vi.fn(),
	cancel: vi.fn(),
	cancelQueued: vi.fn(),
	answerPermission: vi.fn(),
	cancelPermission: vi.fn(),
	answerElicitation: vi.fn(),
	setConfigOption: vi.fn(),
	pause: vi.fn(),
	resumeTurn: vi.fn(),
	compact: vi.fn(),
	forkSession: vi.fn(),
}));

vi.mock("../../services/acpClient", () => ({ acpClient: client }));

import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { AIChatPanel } from "../../components/AIChatPanel/AIChatPanel";
import { aiChatDraft } from "../../components/AIChatPanel/draft";
import { elicitationFields } from "../../components/AIChatPanel/Interactions";
import { resetAcpChatBindings } from "../../components/AIChatPanel/useAcpChat";
import { aiChatPanelAdapter } from "../../panelAdapters/aiChat";
import { acpStore } from "../../stores/acp";
import { acpTranscript } from "../../stores/acpTranscript";
import { aiChatTabs } from "../../stores/aiChatTabs";
import { toastsStore } from "../../stores/toasts";
import type {
	AcpAttachmentSnapshot,
	AcpClientEvent,
	AcpConnectionSnapshot,
	AcpSessionConfigOption,
} from "../../types/acp";
import { handleExternalLinkClick } from "../../utils/externalLinkClick";

const ROOT = "/repo/tuicommander";
/** Where every chat runs: the whole workspace, never one repository. */
const CHAT_ROOT = "/srv/chat-workspace";
const CONNECTION = "01932d5e-0000-7000-8000-0000000000c1";
const SESSION = "01932d5e-0000-7000-8000-0000000000aa";
const SECOND_SESSION = "01932d5e-0000-7000-8000-0000000000bb";
// A real 1x1 PNG admitted by ego's ACP prompt tests.
const PNG_1X1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
let supportsImages = false;
let supportsFork = false;
const CHILD_SESSION = "01932d5e-0000-7000-8000-0000000000dd";

function pasteFile(textarea: HTMLTextAreaElement, file: File): Event {
	const event = new Event("paste", { bubbles: true, cancelable: true });
	Object.defineProperty(event, "clipboardData", {
		value: { items: [{ type: file.type, getAsFile: () => file }] },
	});
	textarea.dispatchEvent(event);
	return event;
}

const MODEL_OPTION: AcpSessionConfigOption = {
	id: "model",
	name: "Model",
	type: "select",
	currentValue: "opus",
	options: [
		{ value: "opus", name: "Opus" },
		{ value: "sonnet", name: "Sonnet" },
	],
};

function attachment(overrides: Partial<AcpAttachmentSnapshot> = {}): AcpAttachmentSnapshot {
	return {
		sessionId: SESSION,
		state: "idle",
		cwd: ROOT,
		additionalDirectories: [],
		configOptions: [MODEL_OPTION],
		usage: null,
		activeTurn: null,
		queuedPrompts: [],
		pendingPermissionIds: [],
		pendingElicitationIds: [],
		...overrides,
	};
}

function snapshot(overrides: Partial<AcpConnectionSnapshot> = {}): AcpConnectionSnapshot {
	return {
		connectionId: CONNECTION,
		generation: 1,
		state: "ready",
		agentInfo: { name: "ego", version: "0.1.0" },
		capabilities: {
			protocol: 1,
			load: true,
			list: true,
			resume: true,
			fork: supportsFork,
			delete: false,
			close: true,
			additionalDirectories: true,
			promptImage: supportsImages,
			promptAudio: false,
			promptEmbeddedContext: false,
			mcpStdio: false,
			mcpHttp: true,
			mcpSse: false,
			mcpAcp: true,
			clientFormElicitation: true,
			clientBooleanConfig: false,
			egoHoldVersion: 1,
			egoCompactVersion: 1,
		},
		attachments: [],
		earliestSequence: 1,
		latestSequence: 1,
		settlement: null,
		...overrides,
	};
}

let sequence = 0;

/** One event frame, as the stream would deliver it. */
function feed(event: AcpClientEvent, sessionId: string | null = SESSION, turnId: string | null = null): void {
	sequence += 1;
	const frame = {
		kind: "event" as const,
		connectionId: CONNECTION,
		generation: 1,
		sequence,
		sessionId,
		turnId,
		event,
	};
	acpStore.applyFrame(CONNECTION, frame);
	acpTranscript.applyFrame(frame);
}

/** Let the panel's connect-then-open-session chain settle. */
async function settle(): Promise<void> {
	for (let turn = 0; turn < 6; turn += 1) await Promise.resolve();
	await new Promise((resolve) => setTimeout(resolve, 0));
}

/** Open the panel and start a chat the way a person would, with "+". Nothing
 *  starts ego on its own any more (#1157-1e54). */
async function renderPanel() {
	const view = render(() => <AIChatPanel visible={true} repoPath={ROOT} onClose={() => {}} />);
	await settle();
	(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
	await settle();
	return view;
}

/** Type a message and press Send. */
async function typeAndSend(container: HTMLElement, text: string): Promise<void> {
	const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
	textarea.value = text;
	textarea.dispatchEvent(new Event("input", { bubbles: true }));
	(
		[...container.querySelectorAll("button")].find(
			(button) => button.getAttribute("aria-label") === "Send",
		) as HTMLButtonElement
	).click();
	await settle();
}

/** Open the panel without starting anything. */
function renderIdlePanel() {
	return render(() => <AIChatPanel visible={true} repoPath={ROOT} onClose={() => {}} />);
}

beforeEach(() => {
	vi.clearAllMocks();
	window.history.replaceState(null, "", "/");
	vi.mocked(invoke).mockImplementation(async (command) => {
		if (command === "load_config") return { ai_chat_sessions: {} };
		if (command === "acp_workspace_root") return CHAT_ROOT;
		return undefined;
	});
	sequence = 0;
	supportsImages = false;
	supportsFork = false;
	settings.egoExecutable = "/usr/local/bin/ego";
	acpStore.reset();
	acpTranscript.reset();
	resetAcpChatBindings();
	localStorage.clear();
	aiChatTabs.resetMemory();
	aiChatDraft.reset();

	client.connect.mockImplementation(async () => {
		const opened = snapshot();
		acpStore.applySnapshot(opened);
		acpStore.markStreaming(CONNECTION);
		return opened;
	});
	client.newSession.mockImplementation(async () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
		return SESSION;
	});
	client.listSessions.mockResolvedValue({ sessions: [], nextCursor: null });
	// A replacement is a fresh process: nothing is attached until it loads.
	client.reconnect.mockImplementation(async () => {
		const opened = snapshot();
		acpStore.applySnapshot(opened);
		acpStore.markStreaming(CONNECTION);
		return opened;
	});
	for (const method of [
		"disconnect",
		"loadSession",
		"cancel",
		"cancelQueued",
		"answerPermission",
		"cancelPermission",
		"answerElicitation",
		"setConfigOption",
		"pause",
		"resumeTurn",
		"compact",
	] as const) {
		client[method].mockResolvedValue(undefined);
	}
	client.prompt.mockResolvedValue("turn-1");
});

afterEach(() => {
	cleanup();
	cleanupToasts();
});

describe("AIChatPanel: repository profile warnings", () => {
	// Catches an authoritative ego clamp silently discarded by the chat host.
	it("shows recorded ego clamp warnings once for the attached conversation", async () => {
		const recorded = JSON.parse(
			readFileSync(resolve(process.cwd(), "src-tauri/tests/fixtures/acp/ego-profile-ceiling.json"), "utf8"),
		);
		client.newSession.mockImplementation(async () => {
			acpStore.applySnapshot(
				snapshot({
					attachments: [
						attachment({
							profileWarnings: recorded._meta.ego.warnings,
						}),
					],
				}),
			);
			return SESSION;
		});
		await renderPanel();
		await settle();
		expect(
			toastsStore.toasts
				.filter((toast) => toast.title === "Repository ego profile")
				.map((toast) => ({ message: toast.message, level: toast.level })),
		).toEqual([
			{
				message:
					"mode yolo exceeds profile ceiling default; using default\nsandbox off exceeds profile ceiling workspace; using workspace",
				level: "warn",
			},
		]);
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						profileWarnings: recorded._meta.ego.warnings,
					}),
				],
			}),
		);
		await settle();
		expect(toastsStore.toasts.filter((toast) => toast.title === "Repository ego profile")).toHaveLength(1);
	});

	// Catches ordinary sessions getting a spurious profile warning.
	it("keeps sessions without repository profile warnings quiet", async () => {
		await renderPanel();
		await settle();
		expect(toastsStore.toasts.filter((toast) => toast.title === "Repository ego profile")).toHaveLength(0);
	});
});

describe("AIChatPanel: the frame it keeps", () => {
	// The panel keeps its slot, its id and its detach control across the engine
	// swap. The registry entry behind this button is what makes Cmd+Alt+A, the
	// status-bar button and the command-palette entry work as well.
	it("offers its own window", async () => {
		const { container } = await renderPanel();
		await settle();

		expect(container.querySelector("#ai-chat-panel")).not.toBeNull();
		const detach = container.querySelector('button[title="Open in separate window"]') as HTMLButtonElement;
		detach.click();
		expect(mockDetachPanel).toHaveBeenCalledWith("ai-chat");
	});

	// The header names the repository, not the focused terminal: the panel binds
	// to a repo root and a session, and a per-terminal binding is the exact
	// inverse of a control plane.
	it("names the repository it is bound to", async () => {
		const { container } = await renderPanel();
		await settle();

		expect(container.textContent).toContain("tuicommander");
	});
});

describe("AIChatPanel: transcript actions", () => {
	// Catches: orphan Copy controls reserving an empty full-width row beneath each reply.
	it("anchors assistant Copy beside the reply, keeps user bubbles compact and tool counts on one line", async () => {
		const style = document.createElement("style");
		style.textContent = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		document.head.append(style);
		try {
			const { container } = await renderPanel();
			await settle();
			feed({ kind: "promptSent", text: "Question" });
			feed({
				kind: "sessionUpdate",
				update: {
					sessionUpdate: "agent_message_chunk",
					content: { type: "text", text: "Answer" },
				},
			});
			feed({
				kind: "sessionUpdate",
				update: {
					sessionUpdate: "tool_call",
					toolCallId: "one",
					title: "Inspect",
					status: "completed",
				},
			});
			await settle();
			const assistantCopy = container.querySelector('button[aria-label="Copy assistant message"]')!;
			const actions = assistantCopy.parentElement!;
			expect(getComputedStyle(actions).position).toBe("absolute");
			expect(getComputedStyle(actions).left).toBe("0px");
			expect(getComputedStyle(actions).top).toBe("100%");
			// A user bubble hugs its text: its Copy sits outside the bubble instead of
			// holding an invisible row that made "che model usi?" look two lines tall.
			const userCopy = container.querySelector('button[aria-label="Copy user message"]')!;
			expect(getComputedStyle(userCopy).position).toBe("absolute");
			const count = container.querySelector(".toolCallCount")!;
			expect(getComputedStyle(count).whiteSpace).toBe("nowrap");
		} finally {
			style.remove();
		}
	});
	it("follows new output at the bottom and keeps the reading position after scrolling up", async () => {
		const { container } = await renderPanel();
		await settle();
		const transcript = container.querySelector('[aria-label="Chat transcript"]') as HTMLDivElement;
		Object.defineProperties(transcript, {
			clientHeight: { configurable: true, value: 200 },
			scrollHeight: { configurable: true, value: 900 },
		});
		transcript.scrollTop = 700;
		transcript.dispatchEvent(new Event("scroll"));
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "First answer" },
			},
		});
		await settle();
		expect(transcript.scrollTop).toBe(900);
		transcript.scrollTop = 300;
		transcript.dispatchEvent(new Event("scroll"));
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: " and more" },
			},
		});
		await settle();
		expect(transcript.scrollTop).toBe(300);
	});

	it("brings the typing indicator into view when a turn starts at the bottom", async () => {
		const { container } = await renderPanel();
		await settle();
		const transcript = container.querySelector('[aria-label="Chat transcript"]') as HTMLDivElement;
		Object.defineProperties(transcript, {
			clientHeight: { configurable: true, value: 200 },
			scrollHeight: { configurable: true, value: 900 },
		});
		transcript.scrollTop = 700;
		transcript.dispatchEvent(new Event("scroll"));
		feed({ kind: "turnStarted" }, SESSION, "turn-1");
		await settle();
		expect(container.querySelector(".thinkingPulse")).not.toBeNull();
		expect(transcript.scrollTop).toBe(900);
	});
	// Catches: a permanently visible Copy label or a button removed from keyboard focus.
	it("hides message Copy at rest while keeping it keyboard focusable", async () => {
		const style = document.createElement("style");
		style.textContent = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		document.head.append(style);
		try {
			const { container } = await renderPanel();
			await settle();
			feed({ kind: "promptSent", text: "Question" });
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Answer" } },
			});
			await settle();
			for (const label of ["Copy user message", "Copy assistant message"]) {
				const button = container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
				expect(getComputedStyle(button).opacity, label).toBe("0");
				button.focus();
				expect(document.activeElement, label).toBe(button);
				button.blur();
			}
		} finally {
			style.remove();
		}
	});

	it("makes message text, tool output, and code selectable under the global no-selection rule", async () => {
		const style = document.createElement("style");
		style.textContent = ["src/global.css", "src/components/AIChatPanel/AIChatPanel.module.css"]
			.map((path) => readFileSync(resolve(process.cwd(), path), "utf8"))
			.join("\n");
		document.head.append(style);
		try {
			const { container } = await renderPanel();
			await settle();
			feed({ kind: "promptSent", text: "Question" });
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "```sh\necho answer\n```" } },
			});
			feed({
				kind: "sessionUpdate",
				update: {
					sessionUpdate: "tool_call",
					toolCallId: "selectable-output",
					title: "Run",
					status: "completed",
					content: [{ type: "content", content: { type: "text", text: "tool output" } }],
				},
			});
			await settle();
			for (const selector of [".userMsg", ".assistantMsg pre", ".toolCallBody"]) {
				const element = container.querySelector(selector);
				expect(element, selector).not.toBeNull();
				expect(getComputedStyle(element!).userSelect, selector).toBe("text");
			}
		} finally {
			style.remove();
		}
	});
	it("sends a detached file link to the main-window terminal opener", async () => {
		window.history.replaceState(null, "", "/?mode=panel");
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src/main.ts" };
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "[file](src/main.ts)" } },
		});
		await settle();
		(container.querySelector(".assistantMsg a") as HTMLAnchorElement).click();
		await settle();
		expect(emitTo).toHaveBeenCalledWith("main", "panel-action", {
			panelId: "ai-chat",
			action: "open-file",
			data: { path: "/repo/tuicommander/src/main.ts" },
		});
		expect(mockOpenFile).not.toHaveBeenCalled();
		aiChatPanelAdapter.handleAction?.("open-file", { path: "/repo/tuicommander/src/main.ts" });
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/src/main.ts");
	});
	it("keeps a file link's line and column when opening the resolved file", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "resolve_terminal_path")
				return { absolute_path: "/repo/tuicommander/src/main.ts", is_directory: false };
			return undefined;
		});
		const { container } = await renderPanel();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "[file](src/main.ts:12:3)" } },
		});
		await settle();
		(container.querySelector(".assistantMsg a") as HTMLAnchorElement).click();
		await settle();
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/src/main.ts", undefined, 12, 3);
		expect(mockSetFolderRoot).not.toHaveBeenCalled();
	});
	it("reveals a resolved directory link in the file browser", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src", is_directory: true };
			return undefined;
		});
		const { container } = await renderPanel();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "[source](src/)" } },
		});
		await settle();
		(container.querySelector(".assistantMsg a") as HTMLAnchorElement).click();
		await settle();
		expect(mockSetFolderRoot).toHaveBeenCalledWith("/repo/tuicommander/src");
		expect(mockShowFileBrowser).toHaveBeenCalledWith(true);
		expect(mockOpenFile).not.toHaveBeenCalled();
	});
	it("sends a detached directory link to the main-window file browser", async () => {
		window.history.replaceState(null, "", "/?mode=panel");
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src", is_directory: true };
			return undefined;
		});
		const { container } = await renderPanel();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "[source](src/)" } },
		});
		await settle();
		(container.querySelector(".assistantMsg a") as HTMLAnchorElement).click();
		await settle();
		expect(emitTo).toHaveBeenCalledWith("main", "panel-action", {
			panelId: "ai-chat",
			action: "open-directory",
			data: { path: "/repo/tuicommander/src" },
		});
		aiChatPanelAdapter.handleAction?.("open-directory", { path: "/repo/tuicommander/src" });
		expect(mockSetFolderRoot).toHaveBeenCalledWith("/repo/tuicommander/src");
		expect(mockShowFileBrowser).toHaveBeenCalledWith(true);
		expect(mockOpenFile).not.toHaveBeenCalled();
	});
	it("copies the raw user message, assistant answer, and fenced code", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "Question <one>" });
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Answer **two**\n\n```sh\necho three\n```" },
			},
		});
		await settle();
		const buttons = [...container.querySelectorAll<HTMLButtonElement>('button[aria-label^="Copy "]')];
		for (const button of buttons) button.click();
		await settle();
		expect(mockWriteClipboard.mock.calls.map(([text]) => text)).toContain("Question <one>");
		expect(mockWriteClipboard.mock.calls.map(([text]) => text)).toContain("Answer **two**\n\n```sh\necho three\n```");
		expect(mockWriteClipboard.mock.calls.map(([text]) => text)).toContain("echo three");
	});

	it("opens web links externally and file links through the terminal file opener", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src/main.ts" };
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: {
					type: "text",
					text: "[Website](https://example.com/help) and [source](/repo/tuicommander/src/main.ts)",
				},
			},
		});
		await settle();
		const links = [...container.querySelectorAll<HTMLAnchorElement>(".assistantMsg a")];
		links.forEach((link) => link.click());
		await settle();
		expect(mockOpenUrl).toHaveBeenCalledWith("https://example.com/help");
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/src/main.ts");
	});

	// Catches: a card rendered as plain assistant text, with no button.
	it("renders a salience=card notice as a card with its action, and opens the result file from it", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/results/worker.md" };
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Worker finished: RESULT" },
				_meta: { ego: { salience: "card", action: { kind: "open_result", path: "results/worker.md" } } },
			},
		});
		await settle();
		const card = container.querySelector('[aria-label="Notice"]') as HTMLElement;
		expect(card.textContent).toContain("Worker finished: RESULT");
		expect(container.querySelector(".assistantMsg")).toBeNull();
		// Catches: a dead action button.
		const button = [...card.querySelectorAll("button")].find((item) => item.textContent === "Open result");
		button?.click();
		await settle();
		expect(invoke).toHaveBeenCalledWith("resolve_terminal_path", { cwd: CHAT_ROOT, candidate: "results/worker.md" });
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/results/worker.md");
	});

	// Catches: breaking non-ego or older ego sessions.
	it("renders an update without _meta as an assistant message, not a card", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "plain reply" } },
		});
		await settle();
		expect(container.querySelector('[aria-label="Notice"]')).toBeNull();
		expect(container.querySelector(".assistantMsg")?.textContent).toContain("plain reply");
	});

	it("makes a bare source path clickable only after the backend resolves it", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src/main.ts" };
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Open src/main.ts:42 to inspect it." },
			},
		});
		await settle();
		const link = [...container.querySelectorAll<HTMLAnchorElement>(".assistantMsg a")].find(
			(anchor) => anchor.textContent === "src/main.ts:42",
		);
		expect(link).toBeDefined();
		link?.click();
		await settle();
		// Relative to where ego runs, which is the workspace, not the viewed repo.
		expect(invoke).toHaveBeenCalledWith("resolve_terminal_path", { cwd: CHAT_ROOT, candidate: "src/main.ts:42" });
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/src/main.ts", undefined, 42, undefined);
	});

	it("opens links in a user message through the same URL and file handlers", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "resolve_terminal_path") return { absolute_path: "/repo/tuicommander/src/main.ts" };
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "Open https://example.com/help and src/main.ts" });
		await settle();
		const links = [...container.querySelectorAll<HTMLAnchorElement>(".userMsg a")];
		expect(links.map((link) => link.textContent)).toEqual(["https://example.com/help", "src/main.ts"]);
		links.forEach((link) => link.click());
		await settle();
		expect(mockOpenUrl).toHaveBeenCalledWith("https://example.com/help");
		expect(mockOpenFile).toHaveBeenCalledWith("/repo/tuicommander/src/main.ts");
	});

	// Catches: the transcript's own onClick and the document-level handler both opening the same web link.
	it("opens a web link in a user message exactly once when the document handler is installed", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			return undefined;
		});
		document.addEventListener("click", handleExternalLinkClick);
		try {
			const { container } = await renderPanel();
			await settle();
			feed({ kind: "promptSent", text: "Open https://example.com/help" });
			await settle();
			container.querySelector<HTMLAnchorElement>(".userMsg a")?.click();
			await settle();
			expect(mockOpenUrl).toHaveBeenCalledTimes(1);
		} finally {
			document.removeEventListener("click", handleExternalLinkClick);
		}
	});

	it("keeps a failed file lookup inside the panel without opening a path", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "resolve_terminal_path") throw new Error("resolver unavailable");
			return undefined;
		});
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "[file](src/main.ts)" } },
		});
		await settle();
		(container.querySelector(".assistantMsg a") as HTMLAnchorElement).click();
		await settle();
		expect(mockOpenFile).not.toHaveBeenCalled();
		expect(mockSetFolderRoot).not.toHaveBeenCalled();
		expect(container.textContent).toContain("file");
	});
});

describe("AIChatPanel: one chat across repositories", () => {
	function renderSwitchable() {
		const [repo, setRepo] = createSignal<string | null>(ROOT);
		const view = render(() => <AIChatPanel visible={true} repoPath={repo()} onClose={() => {}} />);
		return { ...view, setRepo };
	}

	async function send(container: HTMLElement, text: string): Promise<void> {
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = text;
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(
			[...container.querySelectorAll("button")].find(
				(button) => button.getAttribute("aria-label") === "Send",
			) as HTMLButtonElement
		).click();
		await settle();
	}

	it("starts nothing when the panel opens or the repository changes", async () => {
		const { setRepo } = renderSwitchable();
		await settle();
		setRepo("/repo/other");
		await settle();
		setRepo(null);
		await settle();
		expect(client.connect).not.toHaveBeenCalled();
		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.loadSession).not.toHaveBeenCalled();
	});

	it("starts one ego on the first message, in the workspace root, and a second tab reuses it", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = renderSwitchable();
		await settle();
		await send(container, "hello");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.connect).toHaveBeenCalledWith(CHAT_ROOT);
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.prompt).toHaveBeenCalledTimes(1);
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		expect(client.newSession).toHaveBeenCalledTimes(2);
		expect(client.newSession).toHaveBeenLastCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.connect).toHaveBeenCalledTimes(1);
	});

	it("keeps the same tabs and the same chat when the repository changes", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container, setRepo } = renderSwitchable();
		await settle();
		await send(container, "hello");
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		const tabs = () =>
			[...container.querySelectorAll("[data-chat-session]")].map((tab) => tab.getAttribute("data-chat-session"));
		const before = tabs();
		expect(before).toEqual([SESSION, SECOND_SESSION]);
		setRepo("/repo/other");
		await settle();
		expect(tabs()).toEqual(before);
		await send(container, "still here");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SECOND_SESSION, "still here", [], "/repo/other");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.loadSession).not.toHaveBeenCalled();
	});

	// A reloaded document connects again and is handed the ego that is already
	// running, with its sessions still attached; loading one again is refused.
	it("does not replay a conversation the running ego already has attached", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: { [CHAT_ROOT]: SESSION } };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			return undefined;
		});
		client.connect.mockImplementation(async () => {
			const adopted = snapshot({ attachments: [attachment()] });
			acpStore.applySnapshot(adopted);
			acpStore.markStreaming(CONNECTION);
			return adopted;
		});
		const { container } = renderSwitchable();
		await settle();
		await send(container, "carry on");
		expect(client.loadSession).not.toHaveBeenCalled();
		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "carry on", [], ROOT);
	});

	it("sends the viewed repository with each prompt as context, never as the session's cwd", async () => {
		const { container, setRepo } = renderSwitchable();
		await settle();
		await send(container, "what is here");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "what is here", [], ROOT);
		setRepo(null);
		await settle();
		await send(container, "and now");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "and now", [], null);
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.newSession).not.toHaveBeenCalledWith(CONNECTION, ROOT);
	});
});

describe("AIChatPanel: parallel tabs", () => {
	// Catches: selecting a restored tab without a binding never connects or replays.
	it("restores two saved tabs and loads the second when selected without sending", async () => {
		aiChatTabs.add("global", SESSION);
		aiChatTabs.add("global", SECOND_SESSION);
		aiChatTabs.add("global", SESSION);
		aiChatTabs.resetMemory();
		client.loadSession.mockImplementation(async (_id, session) => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: session })] }));
			feed(
				{
					kind: "sessionUpdate",
					update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Saved second answer" } },
				},
				session,
			);
		});
		const { container } = renderIdlePanel();
		await settle();
		(container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(
			container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`)?.getAttribute("aria-selected"),
		).toBe("true");
		expect(container.textContent).toContain("Saved second answer");
		expect(container.textContent).toContain("Model: Opus");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.prompt).not.toHaveBeenCalled();
	});

	function restoreTabs(): void {
		aiChatTabs.add("global", SESSION);
		aiChatTabs.add("global", SECOND_SESSION);
		aiChatTabs.add("global", SESSION);
		aiChatTabs.resetMemory();
	}

	function selectTab(container: HTMLElement, session: string): void {
		(container.querySelector(`button[data-chat-session="${session}"]`) as HTMLButtonElement).click();
	}

	// Catches: an unattached saved tab looks idle and hides its model row during connect.
	it("shows connecting and pending model controls until a saved tab attaches", async () => {
		restoreTabs();
		let finish!: () => void;
		client.loadSession.mockImplementation(
			() =>
				new Promise<void>((resolve) => {
					finish = resolve;
				}),
		);
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		expect(container.textContent).toContain("Connecting conversation…");
		expect(container.textContent).toContain("Model: pending · Mode: pending");
		expect(
			container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`)?.getAttribute("aria-selected"),
		).toBe("true");
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: SECOND_SESSION })] }));
		finish();
		await settle();
		expect(container.textContent).toContain("Model: Opus");
		expect(container.textContent).not.toContain("Connecting conversation…");
	});

	// Catches: Retry clears the error without reattaching the saved tab that failed.
	it("retries the refused saved tab on its existing connection", async () => {
		restoreTabs();
		client.loadSession
			.mockRejectedValueOnce(new Error("Saved conversation unavailable"))
			.mockImplementation(async (_id, session) => {
				acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: session })] }));
				acpTranscript.restore(session, [{ id: "saved-answer", kind: "agent", text: "Recovered second answer" }]);
			});
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		expect(container.textContent).toContain("Saved conversation unavailable");
		expect(container.textContent).toContain("The conversation could not be opened");
		[...container.querySelectorAll("button")].find((button) => button.textContent === "Retry")?.click();
		await settle();
		expect(container.textContent).toContain("Recovered second answer");
		expect(container.textContent).toContain("Model: Opus");
		expect(client.loadSession.mock.calls.map((call) => call[1])).toEqual([SECOND_SESSION, SECOND_SESSION]);
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.reconnect).not.toHaveBeenCalled();
	});

	// Catches: rapid selections spawn two egos or let an older selection take focus.
	it("shares a pending connection and keeps the latest saved tab selected", async () => {
		restoreTabs();
		let finish!: () => void;
		client.connect.mockImplementation(
			() =>
				new Promise<AcpConnectionSnapshot>((resolve) => {
					finish = () => {
						const opened = snapshot();
						acpStore.applySnapshot(opened);
						acpStore.markStreaming(CONNECTION);
						resolve(opened);
					};
				}),
		);
		client.loadSession.mockImplementation(async (_id, session) => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: session })] }));
		});
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SESSION);
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		finish();
		await settle();
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.loadSession).toHaveBeenCalledTimes(1);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(
			container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`)?.getAttribute("aria-selected"),
		).toBe("true");
	});

	// Catches: a slow failed load overwrites the current tab's successful state.
	it("ignores a previous saved tab failure after another tab has attached", async () => {
		restoreTabs();
		let refuse!: () => void;
		client.loadSession.mockImplementation(async (_id, session) => {
			if (session === SECOND_SESSION)
				return new Promise<void>((_resolve, reject) => {
					refuse = () => reject(new Error("Previous tab failed"));
				});
			acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
			acpTranscript.restore(SESSION, [{ id: "current-answer", kind: "agent", text: "Current answer" }]);
		});
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		selectTab(container, SESSION);
		await settle();
		refuse();
		await settle();
		expect(container.textContent).toContain("Current answer");
		expect(container.textContent).not.toContain("Previous tab failed");
		expect(container.textContent).not.toContain("Connecting conversation…");
	});

	// Catches: clicking an already attached saved tab loads it again and duplicates history.
	it("adopts the existing attachment of a saved tab without replaying it", async () => {
		restoreTabs();
		client.connect.mockImplementation(async () => {
			const opened = snapshot({ attachments: [attachment({ sessionId: SECOND_SESSION })] });
			acpStore.applySnapshot(opened);
			acpStore.markStreaming(CONNECTION);
			acpTranscript.restore(SECOND_SESSION, [{ id: "live-answer", kind: "agent", text: "Existing live answer" }]);
			return opened;
		});
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		expect(container.textContent).toContain("Existing live answer");
		expect(container.textContent).toContain("Model: Opus");
		expect(client.loadSession).not.toHaveBeenCalled();
	});

	// Catches: a missing load capability silently replaces saved conversations with a new one.
	it("explains an unsupported saved-tab load without creating a replacement", async () => {
		restoreTabs();
		client.connect.mockImplementation(async () => {
			const opened = snapshot();
			if (opened.capabilities) opened.capabilities.load = false;
			acpStore.applySnapshot(opened);
			return opened;
		});
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		expect(container.textContent).toContain("This agent cannot load saved conversations.");
		expect(container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.loadSession).not.toHaveBeenCalled();
	});

	// Catches: restored tab labels use Chat N or an opaque UUID instead of conversation content.
	it("names saved tabs and picker entries from titles or their first prompt", async () => {
		restoreTabs();
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: SESSION },
				{ sessionId: SECOND_SESSION, cwd: CHAT_ROOT, title: "Release discussion" },
			],
		});
		acpTranscript.restore(SESSION, [{ id: "first-question", kind: "user", text: "Review the saved tabs" }]);
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		expect([...container.querySelectorAll("[data-chat-session]")].map((tab) => tab.textContent)).toEqual([
			"Review the saved tabs",
			"Release discussion",
		]);
		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		expect([...picker.options].map((option) => option.textContent)).toEqual([
			"Review the saved tabs",
			"Release discussion",
		]);
		feed(
			{ kind: "sessionUpdate", update: { sessionUpdate: "session_info_update", title: "Updated release discussion" } },
			SECOND_SESSION,
		);
		expect(container.querySelector(`[data-chat-session="${SECOND_SESSION}"]`)?.textContent).toBe(
			"Updated release discussion",
		);
	});

	// Catches: an attached tab with late config options loses the entire model/mode row.
	it.each<{ configOptions: AcpSessionConfigOption[] }>([
		{ configOptions: [] },
		{
			configOptions: [
				{
					id: "effort",
					name: "Effort",
					type: "select",
					currentValue: "high",
					options: [{ value: "high", name: "High" }],
				},
			],
		},
	])("keeps model and mode pending until options arrive, starting with $configOptions", async ({ configOptions }) => {
		const { container } = await renderPanel();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ configOptions })] }));
		expect(container.textContent).toContain("Model: pending · Mode: pending");
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
		expect(container.textContent).toContain("Model: Opus");
		expect(container.textContent).not.toContain("Model: pending");
		expect(container.querySelector('[aria-label="Session settings"]')).not.toBeNull();
	});

	// Catches: a late custom-launch failure replaces another tab's successful connection state.
	it("ignores a previous custom tab failure after the default tab attaches", async () => {
		restoreTabs();
		const launch = {
			executable: "/opt/observer/ego",
			profile: "coordinator",
			workspace: "/srv/observer",
			peerId: "observer-peer",
		};
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_launches: { [SESSION]: launch } };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			return undefined;
		});
		let refuse!: () => void;
		client.openConversation.mockImplementation(
			() =>
				new Promise((_resolve, reject) => {
					refuse = () => reject(new Error("Previous custom tab failed"));
				}),
		);
		client.loadSession.mockImplementation(async (_id, session) => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: session })] }));
		});
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SESSION);
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		refuse();
		await settle();
		expect(container.textContent).toContain("Model: Opus");
		expect(container.textContent).not.toContain("Previous custom tab failed");
	});

	// Catches: switching from a saved custom tab to a saved default tab never starts its owner.
	it("attaches saved custom and default tabs to their own connections", async () => {
		restoreTabs();
		const launch = {
			executable: "/opt/observer/ego",
			profile: "coordinator",
			workspace: "/srv/observer",
			peerId: "observer-peer",
		};
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_launches: { [SESSION]: launch } };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			return undefined;
		});
		const customConnection = "01932d5e-0000-7000-8000-0000000000c2";
		client.openConversation.mockImplementation(async () => {
			const connection = snapshot({ connectionId: customConnection, attachments: [attachment()] });
			acpStore.applySnapshot(connection);
			acpStore.markStreaming(customConnection);
			return { connection, sessionId: SESSION, launch };
		});
		client.loadSession.mockImplementation(async (_id, session) => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: session })] }));
		});
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SESSION);
		await settle();
		expect(client.openConversation).toHaveBeenCalledWith({ sessionId: SESSION });
		expect(client.connect).not.toHaveBeenCalled();
		selectTab(container, SECOND_SESSION);
		await settle();
		await typeAndSend(container, "Continue the default conversation");
		expect(client.connect).toHaveBeenCalledWith(CHAT_ROOT);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(client.prompt).toHaveBeenCalledWith(
			CONNECTION,
			SECOND_SESSION,
			"Continue the default conversation",
			[],
			ROOT,
		);
		expect(client.newSession).not.toHaveBeenCalled();
	});

	it("keeps both tabs and transcripts when the panel is hidden and shown", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const [visible, setVisible] = createSignal(true);
		const { container } = render(() => <AIChatPanel visible={visible()} repoPath={ROOT} onClose={() => {}} />);
		await settle();
		for (const _ of [SESSION, SECOND_SESSION]) {
			(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
			await settle();
		}
		feed(
			{
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Background reply" } },
			},
			SESSION,
		);
		setVisible(false);
		setVisible(true);
		(container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
		expect(container.textContent).toContain("Background reply");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.disconnect).not.toHaveBeenCalled();
	});
	/** Two tabs on a live connection where neither has an attachment yet, then
	 *  hidden and shown so the bound-root path replays them. */
	async function unattachedTabs(visible: () => boolean, setVisible: (value: boolean) => void) {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const view = render(() => <AIChatPanel visible={visible()} repoPath={ROOT} onClose={() => {}} />);
		await settle();
		for (const _ of [SESSION, SECOND_SESSION]) {
			(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
			await settle();
		}
		setVisible(false);
		setVisible(true);
		await settle();
		return view;
	}
	const loadsOf = (session: string) => client.loadSession.mock.calls.filter((call) => call[1] === session).length;

	it("does not load a tab again while its first load is still pending", async () => {
		let finishLoads!: () => void;
		const pendingLoads = new Promise<void>((resolve) => {
			finishLoads = resolve;
		});
		client.loadSession.mockReturnValue(pendingLoads);
		const [visible, setVisible] = createSignal(true);
		await unattachedTabs(visible, setVisible);
		expect(loadsOf(SECOND_SESSION)).toBe(1);
		acpStore.applySnapshot(snapshot());
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "x" } },
		});
		setVisible(false);
		setVisible(true);
		await settle();
		expect(loadsOf(SECOND_SESSION)).toBe(1);
		finishLoads();
		await settle();
	});

	it("does not re-load a failed tab on the next update, and says why", async () => {
		client.loadSession.mockImplementation(async (_id, session) => {
			if (session === SECOND_SESSION) throw { kind: "agent", message: "MCP admission refused" };
		});
		const [visible, setVisible] = createSignal(true);
		const { container } = await unattachedTabs(visible, setVisible);
		expect(loadsOf(SECOND_SESSION)).toBe(1);
		expect(container.textContent).toContain("MCP admission refused");
		acpStore.applySnapshot(snapshot());
		setVisible(false);
		setVisible(true);
		await settle();
		expect(loadsOf(SECOND_SESSION)).toBe(1);
		(container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		(container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(loadsOf(SECOND_SESSION)).toBe(2);
	});

	it("keeps a terminal context-menu draft queued before the first session opens", async () => {
		aiChatDraft.append("Explain this selected error");
		const { container } = await renderPanel();
		await settle();
		expect((container.querySelector("textarea") as HTMLTextAreaElement).value).toBe("Explain this selected error");
	});
	it("parks a draft with Ctrl+S, blocks browser Save, and restores it after the next send", async () => {
		const { container } = await renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Long-running thought";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		const shortcut = new KeyboardEvent("keydown", { key: "s", ctrlKey: true, bubbles: true, cancelable: true });
		textarea.dispatchEvent(shortcut);
		expect(shortcut.defaultPrevented).toBe(true);
		expect(textarea.value).toBe("");
		expect(container.querySelector('button[aria-pressed="true"][aria-label$="parked draft"]')).not.toBeNull();
		const repeated = new KeyboardEvent("keydown", {
			key: "s",
			ctrlKey: true,
			repeat: true,
			bubbles: true,
			cancelable: true,
		});
		textarea.dispatchEvent(repeated);
		expect(repeated.defaultPrevented).toBe(true);
		expect(textarea.value).toBe("");
		await typeAndSend(container, "Quick interruption");
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Quick interruption", [], ROOT);
		expect(textarea.value).toBe("Long-running thought");
		expect(container.querySelector('button[aria-pressed="true"][aria-label$="parked draft"]')).toBeNull();
	});
	it("warns when browser storage cannot keep a parked draft across reload", async () => {
		const { container } = await renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Keep this safe";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(container.querySelector('button[aria-label="Park draft"]') as HTMLButtonElement).click();
		await settle();
		expect(container.querySelector('[role="alert"]')?.textContent).toContain("reload may lose it");
	});

	it("parks image attachments, swaps nonempty drafts, and keeps parking across panel remount", async () => {
		supportsImages = true;
		const first = await renderPanel();
		const textarea = first.container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Describe this image";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		pasteFile(
			textarea,
			new File([Uint8Array.from(atob(PNG_1X1), (char) => char.charCodeAt(0))], "pixel.png", { type: "image/png" }),
		);
		await vi.waitFor(() => expect(first.container.querySelectorAll('img[alt="Pasted image"]')).toHaveLength(1));
		textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true, bubbles: true, cancelable: true }));
		expect(first.container.querySelectorAll('img[alt="Pasted image"]')).toHaveLength(0);
		textarea.value = "Different draft";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true, bubbles: true, cancelable: true }));
		expect(textarea.value).toBe("Describe this image");
		expect(first.container.querySelectorAll('img[alt="Pasted image"]')).toHaveLength(1);
		first.unmount();
		const second = render(() => <AIChatPanel visible={true} repoPath={ROOT} onClose={() => {}} />);
		await settle();
		expect(second.container.querySelector('button[aria-pressed="true"][aria-label$="parked draft"]')).not.toBeNull();
		await typeAndSend(second.container, "Describe this image");
		expect(client.prompt).toHaveBeenCalledWith(
			CONNECTION,
			SESSION,
			"Describe this image",
			[{ type: "image", mimeType: "image/png", data: PNG_1X1 }],
			ROOT,
		);
		expect((second.container.querySelector("textarea") as HTMLTextAreaElement).value).toBe("Different draft");
	});
	it("keeps parked drafts with their chat tabs and expands parked long paste after restoration", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const longPaste = Array.from({ length: 201 }, (_, index) => `word${index}`).join(" ");
		const paste = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(paste, "clipboardData", { value: { items: [], getData: () => longPaste } });
		textarea.dispatchEvent(paste);
		expect(textarea.value).toContain("[Pasted text #");
		textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true, bubbles: true, cancelable: true }));
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		expect(container.querySelector('button[aria-pressed="true"][aria-label$="parked draft"]')).toBeNull();
		(container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(container.querySelector('button[aria-pressed="true"][aria-label$="parked draft"]')).not.toBeNull();
		await typeAndSend(container, "Short detour");
		const restored = container.querySelector("textarea") as HTMLTextAreaElement;
		expect(restored.value).toContain("[Pasted text #");
		(
			[...container.querySelectorAll("button")].find(
				(button) => button.getAttribute("aria-label") === "Send",
			) as HTMLButtonElement
		).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, longPaste, [], ROOT);
	});
	it("routes prompts to the selected session and returns to the neighbor when closing it", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		let textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Second request";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(
			[...container.querySelectorAll("button")].find(
				(button) => button.getAttribute("aria-label") === "Send",
			) as HTMLButtonElement
		).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, "Second request", [], ROOT);
		(container.querySelector(`button[aria-label="Close chat tab ${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "First request";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(
			[...container.querySelectorAll("button")].find(
				(button) => button.getAttribute("aria-label") === "Send",
			) as HTMLButtonElement
		).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "First request", [], ROOT);
		expect(client.disconnect).not.toHaveBeenCalled();
	});
	it("opens a new tab with the focused-panel shortcut", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		(container.querySelector("textarea") as HTMLTextAreaElement).dispatchEvent(
			new KeyboardEvent("keydown", { key: "t", metaKey: true, bubbles: true }),
		);
		await settle();
		expect(container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
	});

	it("keeps separate ACP transcripts and composer drafts while switching and closing tabs", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		feed(
			{
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "First answer" } },
			},
			SESSION,
		);
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Draft for first";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		feed(
			{
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Second answer" } },
			},
			SECOND_SESSION,
		);
		await settle();
		expect(container.textContent).toContain("Second answer");
		expect(container.textContent).not.toContain("First answer");
		const secondTextarea = container.querySelector("textarea") as HTMLTextAreaElement;
		expect(secondTextarea.value).toBe("");
		secondTextarea.value = "Draft for second";
		secondTextarea.dispatchEvent(new Event("input", { bubbles: true }));
		(container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(container.textContent).toContain("First answer");
		expect(container.textContent).not.toContain("Second answer");
		expect((container.querySelector("textarea") as HTMLTextAreaElement).value).toBe("Draft for first");
		(container.querySelector(`button[aria-label="Close chat tab ${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`)).toBeNull();
		expect(container.textContent).toContain("First answer");
		expect(client.disconnect).not.toHaveBeenCalled();
	});

	it("restores both tabs and their transcripts after the panel mounts in a new document", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const first = await renderPanel();
		await settle();
		(first.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		first.unmount();
		resetAcpChatBindings();
		aiChatTabs.resetMemory();
		acpStore.reset();
		acpTranscript.reset();
		client.loadSession.mockImplementation(async (_id, session) => {
			feed(
				{
					kind: "sessionUpdate",
					update: {
						sessionUpdate: "agent_message_chunk",
						content: { type: "text", text: session === SESSION ? "First replay" : "Second replay" },
					},
				},
				session,
			);
		});
		// The tabs come back with the document; their history comes back when
		// ego does, which is the next message.
		const second = renderIdlePanel();
		await settle();
		expect(second.container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
		expect(client.loadSession).not.toHaveBeenCalled();
		const textarea = second.container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "where were we";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		(
			[...second.container.querySelectorAll("button")].find(
				(button) => button.getAttribute("aria-label") === "Send",
			) as HTMLButtonElement
		).click();
		await settle();
		expect(second.container.textContent).toContain("Second replay");
		(second.container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(second.container.textContent).toContain("First replay");
	});
});

describe("AIChatPanel: transcript keyboard", () => {
	it("selects only the transcript, finds a term, and clears this tab's view", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "First question" });
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Second answer" } },
		});
		await settle();
		const transcript = container.querySelector('[aria-label="Chat transcript"]') as HTMLDivElement;
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "a", metaKey: true, bubbles: true }));
		expect(window.getSelection()?.toString()).toContain("First question");
		expect(window.getSelection()?.toString()).not.toContain("Ask ego about this repository");
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "c", metaKey: true, bubbles: true }));
		expect(mockWriteClipboard.mock.calls.at(-1)?.[0]).toContain("First question");
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "f", metaKey: true, bubbles: true }));
		const search = container.querySelector('input[aria-label="Find in chat"]') as HTMLInputElement;
		expect(search).not.toBeNull();
		search.value = "Second";
		search.dispatchEvent(new Event("input", { bubbles: true }));
		search.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
		expect(window.getSelection()?.toString()).toBe("Second");
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
		await settle();
		expect(container.textContent).not.toContain("Second answer");
	});
});

describe("AIChatPanel: without a configured binary", () => {
	// An empty `ego_executable` is refused in Rust at connect. Saying so is the
	// difference between a panel that explains itself and one that silently
	// never starts.
	it("explains that ACP is not configured and launches nothing", async () => {
		settings.egoExecutable = "";
		const { container } = renderIdlePanel();
		await settle();

		expect(container.textContent).toContain("AI Chat is inactive because the ego executable is not configured");
		expect(client.connect).not.toHaveBeenCalled();
		expect(container.querySelector("textarea")).toBeNull();
	});
	// Catches: missing setup action leaves a fresh profile with no route to configure ego.
	it("opens the ego settings section from the inactive panel", async () => {
		settings.egoExecutable = "";
		const [opened, setOpened] = createSignal("");
		const view = render(() => (
			<>
				<AIChatPanel
					visible={true}
					repoPath={ROOT}
					onClose={() => {}}
					onOpenSettings={(tab, section) => setOpened(`${tab}/${section}`)}
				/>
				<div role="status">{opened()}</div>
			</>
		));
		await settle();
		view.getByRole("button", { name: "Configure ego" }).click();
		expect(view.getByRole("status").textContent).toBe("general/settings-ego");
		expect(client.connect).not.toHaveBeenCalled();
	});

	// Catches: the detached setup button changes only its own WebView and never opens main Settings.
	it("routes detached ego setup to the main window", async () => {
		settings.egoExecutable = "";
		window.history.replaceState(null, "", "/?mode=panel");
		const view = renderIdlePanel();
		await settle();
		view.getByRole("button", { name: "Configure ego" }).click();
		await settle();
		expect(emitTo).toHaveBeenCalledWith("main", "panel-action", {
			panelId: "ai-chat",
			action: "configure-ego",
			data: {},
		});
		expect(invoke).toHaveBeenCalledWith("focus_main_window");
	});

	// Catches: setup completion leaves the inactive panel stuck until TUIC restarts.
	it("activates the composer immediately after configuring ego", async () => {
		settings.egoExecutable = "";
		const view = renderIdlePanel();
		await settle();
		expect(view.queryByRole("textbox")).toBeNull();
		settings.egoExecutable = "/usr/local/bin/ego";
		await settle();
		expect(view.queryByRole("button", { name: "Configure ego" })).toBeNull();
		expect(view.container.querySelector("textarea")).not.toBeNull();
		expect(client.connect).not.toHaveBeenCalled();
	});
});

describe("AIChatPanel: composer height", () => {
	const LINE_PX = 20;
	const PANEL_PX = 500;
	const scrollHeight = vi.spyOn(HTMLTextAreaElement.prototype, "scrollHeight", "get");
	const clientHeight = vi.spyOn(HTMLElement.prototype, "clientHeight", "get");

	beforeEach(() => {
		scrollHeight.mockImplementation(function (this: HTMLTextAreaElement) {
			return this.value.split("\n").length * LINE_PX + 16;
		});
		clientHeight.mockReturnValue(PANEL_PX);
	});
	afterEach(() => {
		scrollHeight.mockReset();
		clientHeight.mockReset();
	});

	async function typeLines(container: HTMLElement, lines: number): Promise<HTMLTextAreaElement> {
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = Array.from({ length: lines }, (_, i) => `line ${i}`).join("\n");
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		await settle();
		return textarea;
	}

	it("grows with the text and scrolls only past 40% of the panel (catches a fixed height or a 150px cap)", async () => {
		const { container } = await renderPanel();
		const grown = await typeLines(container, 5);
		expect(grown.style.height).toBe(`${5 * LINE_PX + 16}px`);
		expect(grown.style.overflowY).toBe("hidden");
		const capped = await typeLines(container, 40);
		expect(capped.style.height).toBe(`${PANEL_PX * 0.4}px`);
		expect(capped.style.overflowY).toBe("auto");
	});

	it("returns to one line after Send (catches a height kept from the previous message)", async () => {
		const { container } = await renderPanel();
		await typeLines(container, 8);
		await typeAndSend(container, "short");
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		expect(textarea.value).toBe("");
		expect(textarea.style.height).toBe("36px");
	});
});

describe("AIChatPanel: a turn", () => {
	it("removes the complete first-turn ack after ego streams it in tiny chunks", async () => {
		const { container } = await renderPanel();
		await settle();
		// Recorded AssistantDelta text from ego session 2080b5ad, events 13-47.
		const chunks = [
			"T",
			"UI",
			"Commander",
			" v",
			"1",
			".",
			"7",
			".",
			"7",
			" is",
			" connected",
			".\n",
			"intent",
			":",
			" Ver",
			"ifico",
			" gli",
			" agent",
			"i",
			" att",
			"ivi",
			" e",
			" ti",
			" ri",
			"porto",
			" lo",
			" stato",
			" att",
			"uale",
			" (",
			"Ag",
			"enti",
			" att",
			"ivi",
			")",
		];
		for (const chunk of chunks) {
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: chunk } },
			});
			await settle();
		}
		const intent = container.querySelector('[aria-label="Agent intent"]');
		expect(intent?.textContent).toContain("Verifico gli agenti attivi e ti riporto lo stato attuale");
		expect(container.textContent).not.toContain(".7.7 is connected.");
	});

	it("keeps a long paste compact in the composer but sends every original word", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const pasted = Array.from({ length: 201 }, (_, index) => `word${index}`).join(" ");
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", { value: { items: [], getData: () => pasted } });
		textarea.dispatchEvent(event);
		await settle();
		expect(event.defaultPrevented).toBe(true);
		expect(textarea.value).toBe("[Pasted text #1 +201 words]");
		(container.querySelector('button[aria-label="Send"]') as HTMLButtonElement).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, pasted, [], ROOT);
	});

	it("preserves ordinary paste at the 200-word boundary", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", {
			value: { items: [], getData: () => Array(200).fill("word").join(" ") },
		});
		textarea.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(false);
	});

	it("keeps typed text around a large paste in the sent prompt", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Before after";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		textarea.setSelectionRange(7, 7);
		const pasted = Array(201).fill("detail").join(" ");
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", { value: { items: [], getData: () => pasted } });
		textarea.dispatchEvent(event);
		await settle();
		expect(textarea.value).toBe("Before [Pasted text #1 +201 words]after");
		(container.querySelector('button[aria-label="Send"]') as HTMLButtonElement).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, `Before ${pasted}after`, [], ROOT);
	});

	it("grows the composer with input until its maximum height", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		Object.defineProperty(textarea, "scrollHeight", { configurable: true, value: 108 });
		textarea.value = "several lines\nmore lines";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		expect(textarea.style.height).toBe("108px");
		Object.defineProperty(textarea, "scrollHeight", { configurable: true, value: 600 });
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		expect(textarea.style.height).toBe("150px");
	});
	it("hides the connection ack and presents intent as turn status", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: {
					type: "text",
					text: "TUICommander v1.7.7 is connected. intent: Controllo lo stato prima di risponderti (Stato)\nRisultato pronto.",
				},
			},
		});
		await settle();
		expect(container.textContent).not.toContain("TUICommander v1.7.7 is connected.");
		expect(container.textContent).not.toContain("intent:");
		expect(container.querySelector('[aria-label="Agent intent"]')?.textContent).toContain(
			"Controllo lo stato prima di risponderti",
		);
		expect(container.textContent).toContain("Risultato pronto.");
	});

	it("turns a streamed suggestion into three actions and submits the chosen text", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Scegli il prossimo passo.\nsug" },
			},
		});
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "gest: [ Stato lavori | Una decisione aperta | Nuova richiesta ]" },
			},
		});
		await settle();
		expect(container.textContent).not.toContain("suggest:");
		const choices = [...container.querySelectorAll('[aria-label="Suggested replies"] button')];
		expect(choices.map((button) => button.textContent)).toEqual([
			"Stato lavori",
			"Una decisione aperta",
			"Nuova richiesta",
		]);
		(choices[1] as HTMLButtonElement).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Una decisione aperta", [], ROOT);
		feed({ kind: "promptSent", text: "Una decisione aperta" });
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "Una decisione aperta" } },
		});
		await settle();
		// Catches: the suggestion click creates a bubble that doubles when ego echoes the prompt.
		expect(
			[...container.querySelectorAll(".userMsg")].map((message) => message.textContent?.replace("Copy", "")),
		).toEqual(["Una decisione aperta"]);
	});

	// Catches: an inline token at the end of the answer remaining visible as raw text.
	it("turns a trailing inline suggestion into reply actions", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "The checks are active. suggest: [ Retry | Show status | Diagnose ]" },
			},
		});
		await settle();
		expect(container.querySelector(".assistantMsg")?.textContent).toContain("The checks are active.");
		expect(container.querySelector(".assistantMsg")?.textContent).not.toContain("suggest:");
		expect(
			[...container.querySelectorAll('[aria-label="Suggested replies"] button')].map((button) => button.textContent),
		).toEqual(["Retry", "Show status", "Diagnose"]);
	});

	// Catches: parsing a protocol-looking phrase before the end of the answer.
	it("keeps an inline suggestion before further prose as answer text", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "The syntax is suggest: [ A | B ] in this example.\nMore explanation follows." },
			},
		});
		await settle();
		expect(container.querySelector(".assistantMsg")?.textContent).toContain("suggest: [ A | B ] in this example.");
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("leaves mentions of protocol words inside prose and fenced code unchanged", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: {
					type: "text",
					text: "I suggest: [ A | B ] in the sample.\n```text\nsuggest: [ Alpha | Beta ]\nintent: sample (Demo)\n```",
				},
			},
		});
		await settle();
		expect(container.textContent).toContain("I suggest: [ A | B ]");
		expect(container.textContent).toContain("suggest: [ Alpha | Beta ]");
		expect(container.textContent).toContain("intent: sample (Demo)");
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("keeps malformed suggestions and mid-sentence intent as answer text", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: {
					type: "text",
					text: "The intent: of this example is explanatory.\nsuggest: [ A | B\nsuggest: [ A | nested [ B ] | C ]\nsuggest: [ A | B | C | D | E ]",
				},
			},
		});
		await settle();
		expect(container.textContent).toContain("The intent: of this example");
		expect(container.textContent).toContain("suggest: [ A | B");
		expect(container.textContent).toContain("suggest: [ A | nested [ B ] | C ]");
		expect(container.textContent).toContain("suggest: [ A | B | C | D | E ]");
		expect(container.querySelector('[aria-label="Agent intent"]')).toBeNull();
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("does not interpret markers in indented markdown code", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Example:\n\n    suggest: [ Yes | No ]\n    intent: show syntax (Example)" },
			},
		});
		await settle();
		expect(container.textContent).toContain("suggest: [ Yes | No ]");
		expect(container.textContent).toContain("intent: show syntax (Example)");
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("keeps an indented code example at the start of an answer", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "    suggest: [ A | B ]" },
			},
		});
		await settle();
		expect(container.querySelector("pre code")?.textContent).toContain("suggest: [ A | B ]");
		expect(container.querySelector('[aria-label="Suggested replies"]')).toBeNull();
	});

	it("shows an ACP prompt failure in the transcript and returns the composer to Send", async () => {
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						state: "prompting",
						activeTurn: { turnId: "turn-1", state: "running", stopReason: null, usage: null },
					}),
				],
			}),
		);
		feed(
			{ kind: "turnFailed", message: "no capabilities are configured for `openai-codex/gpt-5.6-sol`", state: "idle" },
			SESSION,
			"turn-1",
		);
		await settle();
		expect(container.textContent).toContain("no capabilities are configured");
		expect(
			[...container.querySelectorAll("button")].some((button) => button.getAttribute("aria-label") === "Send"),
		).toBe(true);
	});

	it("shows that a normally settled turn without an agent reply ended", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "ciao" });
		feed({ kind: "turnSettled", stopReason: "end_turn", usage: null });
		await settle();
		expect(container.textContent).toContain("Turn ended without a reply");
	});

	it("shows an agent refusal after the prompt settles", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "promptSent", text: "ciao" });
		feed({ kind: "turnSettled", stopReason: "refusal", usage: null });
		await settle();
		expect(container.textContent).toContain("The agent refused this turn.");
	});

	it("shows the shared queue, can remove the phone prompt, and accepts a desktop prompt while busy", async () => {
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						state: "prompting",
						activeTurn: { turnId: "running", state: "running", stopReason: null, usage: null },
						queuedPrompts: [{ turnId: "phone-queued", summary: "from phone" }],
					}),
				],
			}),
		);
		await settle();
		expect(container.textContent).toContain("from phone");
		const remove = container.querySelector('button[aria-label="Cancel queued prompt from phone"]') as HTMLButtonElement;
		expect(remove).not.toBeNull();
		remove.click();
		await settle();
		expect(client.cancelQueued).toHaveBeenCalledWith(CONNECTION, SESSION, "phone-queued");

		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "desktop next";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		await settle();
		[...container.querySelectorAll("button")].find((button) => button.getAttribute("aria-label") === "Queue")?.click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "desktop next", [], ROOT);
	});
	// Catches: an image paste is ignored even though ego advertises image prompts.
	it("stages a pasted PNG and sends it with the next turn", async () => {
		supportsImages = true;
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const file = new File([Uint8Array.from(atob(PNG_1X1), (byte) => byte.charCodeAt(0))], "clip.png", {
			type: "image/png",
		});
		const event = pasteFile(textarea, file);
		await vi.waitFor(() => expect(container.querySelector('img[alt="Pasted image"]')).not.toBeNull());

		expect(event.defaultPrevented).toBe(true);
		[...container.querySelectorAll("button")].find((button) => button.getAttribute("aria-label") === "Send")?.click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(
			CONNECTION,
			SESSION,
			"",
			[{ type: "image", mimeType: "image/png", data: PNG_1X1 }],
			ROOT,
		);
	});

	// Catches: a paste is accepted for an agent that cannot read image blocks.
	it("refuses an image when the connection did not advertise image prompts", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		pasteFile(textarea, new File(["png"], "clip.png", { type: "image/png" }));
		await settle();

		expect(container.textContent).toContain("does not support images");
		expect(container.querySelector('img[alt="Pasted image"]')).toBeNull();
		expect(client.prompt).not.toHaveBeenCalled();
	});

	// Catches: a large clipboard blob is encoded before any size check.
	it("refuses an oversized image and reports its size", async () => {
		supportsImages = true;
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const read = vi.spyOn(FileReader.prototype, "readAsDataURL");
		pasteFile(textarea, new File([new Uint8Array(10 * 1024 * 1024 + 1)], "huge.png", { type: "image/png" }));
		await settle();

		expect(container.textContent).toContain("10 MiB");
		expect(container.querySelector('img[alt="Pasted image"]')).toBeNull();
		expect(client.prompt).not.toHaveBeenCalled();
		expect(read).not.toHaveBeenCalled();
		read.mockRestore();
	});

	// Catches: two quick paste events each pass the cap while the first read is pending.
	it("keeps rapid image pastes within the total size cap", async () => {
		supportsImages = true;
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		pasteFile(textarea, new File([new Uint8Array(6 * 1024 * 1024)], "first.png", { type: "image/png" }));
		pasteFile(textarea, new File([new Uint8Array(6 * 1024 * 1024)], "second.png", { type: "image/png" }));

		await vi.waitFor(() => expect(container.textContent).toContain("6.0 MiB; the total limit is 10 MiB"));
		expect(container.querySelectorAll('img[alt="Pasted image"]')).toHaveLength(1);
	});

	// Catches: a staged image still goes on the wire after the person removes it.
	it("removes a staged image before sending the text", async () => {
		supportsImages = true;
		const { container } = await renderPanel();
		await settle();
		pasteFile(
			container.querySelector("textarea") as HTMLTextAreaElement,
			new File(["png"], "clip.png", { type: "image/png" }),
		);
		await vi.waitFor(() => expect(container.querySelector('button[aria-label="Remove pasted image"]')).not.toBeNull());
		(container.querySelector('button[aria-label="Remove pasted image"]') as HTMLButtonElement).click();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "text only";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		await settle();
		[...container.querySelectorAll("button")].find((button) => button.getAttribute("aria-label") === "Send")?.click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "text only", [], ROOT);
	});

	// Catches: intercepting all paste events breaks the browser's text insertion.
	it("leaves plain text paste to the textarea", async () => {
		const { container } = await renderPanel();
		await settle();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", { value: { items: [], getData: () => "ordinary text" } });
		textarea.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(false);
	});
	it("opens the connection and its session on the workspace root", async () => {
		await renderPanel();
		await settle();

		expect(client.connect).toHaveBeenCalledWith(CHAT_ROOT);
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
	});

	it("sends what was typed and streams the answer back", async () => {
		const { container } = await renderPanel();
		await settle();

		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "what does acp/mod.rs do?";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		await settle();

		const send = [...container.querySelectorAll("button")].find(
			(button) => button.getAttribute("aria-label") === "Send",
		);
		send?.click();
		await settle();

		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "what does acp/mod.rs do?", [], ROOT);

		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "It " } },
		});
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "serializes." } },
		});
		await settle();

		expect(container.textContent).toContain("It serializes.");
	});

	// One connection per root. Coming back to a repository must not launch a
	// second ego on a root that already has one.
	it("reuses the connection a root already has", async () => {
		const first = await renderPanel();
		await settle();
		first.unmount();

		await renderPanel();
		await settle();

		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.newSession).toHaveBeenCalledTimes(1);
	});
});

describe("AIChatPanel: durable conversations", () => {
	it("labels untitled conversations without exposing an id as the option text", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: null, updatedAt: "2026-09-27T09:00:00Z" },
				{
					sessionId: "01932d5e-0000-7000-8000-0000000000bb",
					cwd: CHAT_ROOT,
					title: null,
					updatedAt: "2026-09-26T09:00:00Z",
				},
			],
			nextCursor: null,
		});
		const { container } = await renderPanel();
		await settle();
		const options = [...container.querySelectorAll('select[title="Conversation"] option')];
		expect(options).toHaveLength(2);
		expect(options[0].textContent).toContain("Conversation");
		expect(options[0].textContent).not.toContain(SESSION);
		expect(options[0].getAttribute("title")).toBe(SESSION);
	});

	it("shows a new ACP session title in the header and conversation picker", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Conversation 1", updatedAt: "2026-09-27T09:00:00Z" },
				{ sessionId: "other", cwd: CHAT_ROOT, title: "Other", updatedAt: "2026-09-26T09:00:00Z" },
			],
			nextCursor: null,
		});
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "session_info_update", title: "Review architecture" } });
		await settle();

		expect(container.querySelector('select[title="Conversation"]')?.textContent).toContain("Review architecture");
		expect(container.querySelector('[class*="headerLeft"]')?.textContent).toContain("Review architecture");
	});

	it("renders context occupancy and cost from one ACP usage update", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "usage_update", used: 50000, size: 200000, cost: { amount: 0.001035, currency: "USD" } },
		});
		await settle();

		expect(container.textContent).toContain("Context 25%");
		expect(container.textContent).toContain("USD 0.001035");
	});

	it("renders context occupancy without a missing cost", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "usage_update", used: 100, size: 400 } });
		await settle();

		expect(container.textContent).toContain("Context 25%");
		expect(container.textContent).not.toContain("NaN");
		expect(container.textContent).not.toContain("USD");
	});

	it("does not carry a previous cost into a costless usage update", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "usage_update", used: 100, size: 400, cost: { amount: 2, currency: "USD" } },
		});
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "usage_update", used: 200, size: 400 } });
		await settle();

		expect(container.textContent).toContain("Context 50%");
		expect(container.textContent).not.toContain("USD 2");
	});

	it("does not render a percentage for a zero context window", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "usage_update", used: 100, size: 0 } });
		await settle();

		expect(container.textContent).not.toContain("Context");
		expect(container.textContent).not.toContain("Infinity");
	});

	it("does not replace a saved binding when config cannot be read", async () => {
		vi.mocked(invoke).mockRejectedValue(new Error("Config unavailable"));
		const { container } = await renderPanel();
		await settle();
		expect(client.newSession).not.toHaveBeenCalled();
		expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_config")).toBe(false);
		expect(container.textContent).toContain("Config unavailable");
	});

	it("opens a conversation when the agent does not advertise listing", async () => {
		client.connect.mockImplementation(async () => {
			const opened = snapshot({ capabilities: { ...snapshot().capabilities!, list: false } });
			acpStore.applySnapshot(opened);
			acpStore.markStreaming(CONNECTION);
			return opened;
		});
		client.newSession.mockImplementation(async () => {
			acpStore.applySnapshot(
				snapshot({
					capabilities: { ...snapshot().capabilities!, list: false },
					attachments: [attachment()],
				}),
			);
			return SESSION;
		});
		const { container } = await renderPanel();
		await settle();
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.listSessions).not.toHaveBeenCalled();
		expect(container.querySelector("textarea")).not.toBeNull();
		const next = container.querySelector<HTMLButtonElement>('button[aria-label="Start another conversation"]');
		next?.click();
		await settle();
		expect(client.newSession).toHaveBeenCalledTimes(2);
		expect(client.listSessions).not.toHaveBeenCalled();
	});

	it("saves the selected session and restores it from the next document's config", async () => {
		const saved: Record<string, string> = {};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "load_config") return { ai_chat_sessions: { ...saved } };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "save_config")
				Object.assign(
					saved,
					(args as { config: { ai_chat_sessions: Record<string, string> } }).config.ai_chat_sessions,
				);
			return undefined;
		});
		const first = await renderPanel();
		await settle();
		expect(saved[CHAT_ROOT]).toBe(SESSION);
		first.unmount();
		resetAcpChatBindings();
		vi.clearAllMocks();

		const second = renderIdlePanel();
		await settle();
		await typeAndSend(second.container, "go on");
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SESSION, CHAT_ROOT);
		expect(client.newSession).not.toHaveBeenCalled();
	});

	it("keeps the backend projection order across ACP list pages", async () => {
		client.listSessions.mockImplementation(async (_id, _root, cursor) =>
			cursor
				? {
						sessions: [{ sessionId: SESSION, cwd: CHAT_ROOT, title: "First page", updatedAt: "2026-09-25T09:00:00Z" }],
						nextCursor: null,
					}
				: {
						sessions: [{ sessionId: "newest", cwd: CHAT_ROOT, title: "New page", updatedAt: "2026-09-27T09:00:00Z" }],
						nextCursor: "page-2",
					},
		);
		const { container } = await renderPanel();
		await settle();
		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		expect([...picker.options].map((option) => option.textContent)).toEqual(["New page", "First page"]);
	});

	it("keeps the current conversation visible when a picked load is refused", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Current", updatedAt: "2026-09-26T09:00:00Z" },
				{ sessionId: "unavailable", cwd: CHAT_ROOT, title: "Unavailable", updatedAt: "2026-09-25T09:00:00Z" },
			],
			nextCursor: null,
		});
		client.loadSession.mockRejectedValue(new Error("Session unavailable"));
		const { container } = await renderPanel();
		await settle();
		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		picker.value = "unavailable";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		expect(picker.value).toBe(SESSION);
		expect(container.textContent).toContain("Session unavailable");
	});

	it("loads the saved conversation after a fresh document opens", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: { [CHAT_ROOT]: "prior-session" } };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			return undefined;
		});
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: "prior-session", cwd: CHAT_ROOT, title: "Design review", updatedAt: "2026-09-26T12:00:00Z" },
			],
			nextCursor: null,
		});
		client.loadSession.mockImplementation(async () => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: "prior-session" })] }));
			feed(
				{
					kind: "sessionUpdate",
					update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Earlier answer" } },
				},
				"prior-session",
			);
		});

		const { container } = renderIdlePanel();
		await settle();
		expect(client.connect).not.toHaveBeenCalled();
		await typeAndSend(container, "go on");

		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, "prior-session", CHAT_ROOT);
		expect(container.textContent).toContain("Earlier answer");
	});

	it("renders durable conversation titles in the backend activity order", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: "newest", cwd: CHAT_ROOT, title: "Latest topic", updatedAt: "2026-09-26T09:00:00Z" },
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Current topic", updatedAt: "2026-09-25T09:00:00Z" },
				{ sessionId: "old", cwd: CHAT_ROOT, title: "Old topic", updatedAt: "2026-09-24T09:00:00Z" },
			],
			nextCursor: null,
		});
		const { container } = await renderPanel();
		await settle();

		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		expect([...picker.options].map((option) => option.textContent)).toEqual([
			"Latest topic",
			"Current topic",
			"Old topic",
		]);
	});

	it("loads a picked conversation once and shows its replay without duplication", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Current", updatedAt: "2026-09-25T09:00:00Z" },
				{ sessionId: "prior-session", cwd: CHAT_ROOT, title: "Earlier", updatedAt: "2026-09-24T09:00:00Z" },
			],
			nextCursor: null,
		});
		client.loadSession.mockImplementation(async () => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: "prior-session" })] }));
			feed(
				{
					kind: "sessionUpdate",
					update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Only once" } },
				},
				"prior-session",
			);
		});
		const { container } = await renderPanel();
		await settle();
		const picker = container.querySelector('select[title="Conversation"]') as HTMLSelectElement;
		picker.value = "prior-session";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();

		expect(client.loadSession).toHaveBeenCalledTimes(1);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, "prior-session", CHAT_ROOT);
		expect(container.textContent?.split("Only once")).toHaveLength(2);
	});
});

describe("AIChatPanel: tool activity", () => {
	function tool(id: number, status: "completed" | "failed" = "completed") {
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "tool_call",
				toolCallId: `call-${id}`,
				title: `Inspect file ${id}`,
				kind: "read",
				status,
				content: [{ type: "content", content: { type: "text", text: `output ${id}` } }],
			},
		});
	}

	it("keeps raw command text out of the collapsed row", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "tool_call",
				toolCallId: "bash",
				title: "bash -lc command -v tuic || true; ls tools/*",
				kind: "execute",
				status: "completed",
				content: [{ type: "content", content: { type: "text", text: "command output" } }],
			},
		});
		await settle();
		const group = container.querySelector("details[class*=toolActivity]") as HTMLDetailsElement;
		expect(group.querySelector(":scope > summary")?.textContent).toContain("1 tool call");
		expect(group.querySelector(":scope > summary")?.textContent).not.toContain("command -v");
		group.open = true;
		expect(group.textContent).toContain("command -v");
	});

	it("collapses seven calls in one turn into one activity line with count and salient titles", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "Check files" } },
		});
		for (let id = 1; id <= 7; id += 1) tool(id);
		await settle();

		const summaries = [...container.querySelectorAll("summary")].filter((summary) =>
			summary.textContent?.includes("7 tool calls"),
		);
		expect(summaries).toHaveLength(1);
		expect(summaries[0].textContent).toContain("Inspect file 1");
		expect(summaries[0].textContent).toContain("Inspect file 2");
		expect(summaries[0].textContent).toMatch(/\d+(?:\.\d+)?s/);
		expect((summaries[0].parentElement as HTMLDetailsElement).open).toBe(false);
	});

	it("requires a second expansion before showing tool output", async () => {
		const { container } = await renderPanel();
		await settle();
		tool(1);
		await settle();

		const activity = [...container.querySelectorAll("details")].find((details) =>
			details.querySelector("summary")?.textContent?.includes("tool call"),
		);
		expect(activity).toBeDefined();
		expect(activity?.open).toBe(false);
		activity!.open = true;
		await settle();
		expect(activity?.textContent).toContain("Inspect file 1");
		expect(activity?.textContent).toContain("read");
		expect(activity?.textContent).toContain("Completed");
		const output = activity?.querySelector("details");
		expect(output?.open).toBe(false);
		expect(output?.textContent).toContain("output 1");
	});

	it("marks the activity line failed when any call fails", async () => {
		const { container } = await renderPanel();
		await settle();
		tool(1);
		tool(2, "failed");
		await settle();

		const summary = [...container.querySelectorAll("summary")].find((element) =>
			element.textContent?.includes("2 tool calls"),
		);
		expect(summary?.textContent).toContain("Failed");
	});

	it("keeps open permission and elicitation cards outside collapsed activity", async () => {
		const { container } = await renderPanel();
		await settle();
		tool(1);
		feed({
			kind: "permissionRequested",
			requestId: "req-activity",
			request: {
				sessionId: SESSION,
				toolCall: { title: "Write file" },
				options: [{ optionId: "reject", name: "Reject", kind: "reject_once" }],
			},
		});
		feed({
			kind: "elicitationRequested",
			requestId: "req-activity-form",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Which branch?",
				requestedSchema: { type: "object", properties: { branch: { type: "string" } } },
			},
		});
		await settle();

		const activity = [...container.querySelectorAll("details")].find((details) =>
			details.querySelector("summary")?.textContent?.includes("tool call"),
		);
		expect(activity?.open).toBe(false);
		expect(container.textContent).toContain("Write file");
		expect(container.textContent).toContain("Which branch?");
		expect([...container.querySelectorAll("button")].find((button) => button.textContent === "Reject")).toBeDefined();
	});

	it("keeps calls together across agent text but separates the next user turn", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "First task" } },
		});
		tool(1);
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Checking more." } },
		});
		tool(2);
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "Second task" } },
		});
		tool(3);
		await settle();

		const summaries = [...container.querySelectorAll("summary")].filter((summary) =>
			summary.textContent?.includes("tool call"),
		);
		expect(summaries).toHaveLength(2);
		expect(summaries[0].textContent).toContain("2 tool calls");
		expect(summaries[1].textContent).toContain("1 tool call");
	});

	it("updates the collapsed line when an existing call later fails", async () => {
		const { container } = await renderPanel();
		await settle();
		tool(1);
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "tool_call_update", toolCallId: "call-1", status: "failed" },
		});
		await settle();

		const summary = [...container.querySelectorAll("summary")].find((element) =>
			element.textContent?.includes("1 tool call"),
		);
		expect(summary?.textContent).toContain("Failed");
	});

	it.each(["completed", "failed"] as const)(
		"stops pulsing both dots when a running call becomes %s",
		async (status) => {
			const { container } = await renderPanel();
			await settle();
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "tool_call", toolCallId: "transition", title: "Inspect file", status: "pending" },
			});
			await settle();

			const activity = container.querySelector("details[class*=toolActivity]") as HTMLDetailsElement;
			activity.open = true;
			const dots = () => [...activity.querySelectorAll("summary > span:first-child")];
			expect(dots()).toHaveLength(2);
			for (const dot of dots()) expect(dot.className).toContain("toolCallPending");
			const pendingRowDot = dots()[1];

			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "tool_call_update", toolCallId: "transition", status },
			});
			await settle();

			// WebKit keeps a running CSS animation when only the class of an element
			// inside a closed <details> changes, so the row dot must be a new element.
			expect(dots()[1]).not.toBe(pendingRowDot);
			for (const dot of dots()) {
				expect(dot.className).not.toContain("toolCallPending");
				expect(dot.className).toContain(status === "failed" ? "toolCallFailure" : "toolCallSuccess");
			}
		},
	);

	it.each([
		{ event: { kind: "turnSettled", stopReason: "end_turn", usage: null } as const, status: "completed" },
		{ event: { kind: "turnFailed", message: "tool process exited", state: "idle" } as const, status: "failed" },
	])("stops pulsing an unfinished tool call when the turn ends as $status", async ({ event, status }) => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "tool_call", toolCallId: "unfinished", title: "Inspect file", status: "in_progress" },
		});
		await settle();
		const activity = container.querySelector("details[class*=toolActivity]") as HTMLDetailsElement;
		activity.open = true;
		const dots = () => [...activity.querySelectorAll("summary > span:first-child")];
		expect(dots()).toHaveLength(2);
		for (const dot of dots()) expect(dot.className).toContain("toolCallPending");

		feed(event);
		await settle();
		for (const dot of dots()) {
			expect(dot.className).not.toContain("toolCallPending");
			expect(dot.className).toContain(status === "failed" ? "toolCallFailure" : "toolCallSuccess");
		}
	});

	it("extends observed duration when a later call joins an already completed activity", async () => {
		const realNow = performance.now.bind(performance);
		const now = vi.spyOn(performance, "now");
		let elapsed = 0;
		now.mockImplementation(() => realNow() + elapsed);
		try {
			const { container } = await renderPanel();
			await settle();
			tool(1);
			await settle();
			elapsed = 2500;
			tool(2);
			await settle();

			const summary = [...container.querySelectorAll("summary")].find((element) =>
				element.textContent?.includes("2 tool calls"),
			);
			expect(summary?.textContent).toMatch(/2\.5s observed|2\.6s observed/);
		} finally {
			now.mockRestore();
		}
	});
});

describe("AIChatPanel: permission", () => {
	it("keeps Allow always visibly enabled and answers with its published id", async () => {
		const { container } = await renderPanel();
		feed({
			kind: "permissionRequested",
			requestId: "req-persistent",
			request: {
				sessionId: SESSION,
				toolCall: { title: "Edit src/main.rs" },
				options: [
					{ optionId: "approve-this-run", name: "Allow once", kind: "allow_once" },
					{ optionId: "persist-rule", name: "Allow always", kind: "allow_always" },
				],
			},
		});
		await settle();

		const always = [...container.querySelectorAll("button")].find((button) => button.textContent === "Allow always");
		expect(always).toBeDefined();
		expect(always?.disabled).toBe(false);
		always?.click();
		await settle();
		expect(client.answerPermission).toHaveBeenCalledWith(CONNECTION, "req-persistent", "persist-rule");

		const stylesheet = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		const alwaysStyle = /\.alwaysAllowBtn\s*\{([^}]*)\}/.exec(stylesheet)?.[1];
		expect(alwaysStyle, "persistent approval must use the enabled success color").toContain("var(--success)");
		expect(alwaysStyle).not.toContain("var(--fg-muted)");
	});

	// The option list is the agent's. Answering with anything but one of its own
	// option ids answers a question nobody asked.
	it("renders the options ego published and answers with one of their ids", async () => {
		const { container } = await renderPanel();
		await settle();

		feed({
			kind: "permissionRequested",
			requestId: "req-1",
			request: {
				sessionId: SESSION,
				toolCall: { title: "Write src/main.rs" },
				options: [
					{ optionId: "allow-once", name: "Allow once", kind: "allow_once" },
					{ optionId: "reject-once", name: "Reject", kind: "reject_once" },
				],
			},
		});
		await settle();

		expect(container.textContent).toContain("Write src/main.rs");
		const allow = [...container.querySelectorAll("button")].find((button) => button.textContent === "Allow once");
		expect(allow).toBeDefined();
		allow?.click();
		await settle();

		expect(client.answerPermission).toHaveBeenCalledWith(CONNECTION, "req-1", "allow-once");
	});
});

describe("AIChatPanel: elicitation", () => {
	it("keeps an elicitation with an additional unsupported field in the form", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "elicitationRequested",
			requestId: "mixed-form",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Choose and describe",
				requestedSchema: {
					type: "object",
					properties: { answer: { type: "string", enum: ["yes", "no"] }, context: { type: "array" } },
				},
			},
		});
		await settle();
		expect(container.querySelector("select.formInput")).not.toBeNull();
		expect(container.textContent).toContain("Submit");
	});

	it("keeps a four-choice elicitation in the form", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "elicitationRequested",
			requestId: "choice-4",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Choose one",
				requestedSchema: {
					type: "object",
					properties: { answer: { type: "string", enum: ["one", "two", "three", "four"] } },
					required: ["answer"],
				},
			},
		});
		await settle();
		expect(container.querySelector("select.formInput")).not.toBeNull();
		expect(container.textContent).toContain("Submit");
	});

	it("offers small single-select trust choices as direct buttons plus Cancel", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "turnStarted" }, SESSION, "turn-1");
		feed({
			kind: "elicitationRequested",
			requestId: "trust-1",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Trust this workspace?",
				requestedSchema: {
					type: "object",
					properties: { answer: { type: "string", enum: ["trusted", "untrusted"] } },
					required: ["answer"],
				},
			},
		});
		await settle();
		expect(container.querySelector("select.formInput")).toBeNull();
		expect(container.textContent).not.toContain("Submit");
		const trust = [...container.querySelectorAll("button")].find((button) => button.textContent === "Trust");
		expect(trust).toBeDefined();
		expect([...container.querySelectorAll("button")].some((button) => button.textContent === "Don't trust")).toBe(true);
		expect([...container.querySelectorAll("button")].some((button) => button.textContent === "Cancel")).toBe(true);
		trust?.click();
		await settle();
		expect(client.answerElicitation).toHaveBeenCalledWith(CONNECTION, "trust-1", {
			action: "accept",
			content: { answer: "trusted" },
		});
		feed({
			kind: "elicitationSettled",
			requestId: "trust-1",
			action: { action: "accept", content: { answer: "trusted" } },
		});
		feed({ kind: "turnFailed", message: "model unavailable", state: "idle" }, SESSION, "turn-1");
		await settle();
		expect(container.textContent).toContain("model unavailable");
		expect(
			[...container.querySelectorAll("button")].some((button) => button.getAttribute("aria-label") === "Send"),
		).toBe(true);
	});

	it("renders a form and submits the values that were filled in", async () => {
		const { container } = await renderPanel();
		await settle();

		feed({
			kind: "elicitationRequested",
			requestId: "req-2",
			request: {
				mode: "form",
				sessionId: SESSION,
				message: "Which branch should I use?",
				requestedSchema: {
					type: "object",
					properties: { branch: { type: "string", title: "Branch" } },
					required: ["branch"],
				},
			},
		});
		await settle();

		expect(container.textContent).toContain("Which branch should I use?");
		const input = container.querySelector('input[type="text"]') as HTMLInputElement;
		input.value = "main";
		input.dispatchEvent(new Event("input", { bubbles: true }));
		const submit = [...container.querySelectorAll("button")].find((button) => button.textContent === "Submit");
		submit?.click();
		await settle();

		expect(client.answerElicitation).toHaveBeenCalledWith(CONNECTION, "req-2", {
			action: "accept",
			content: { branch: "main" },
		});
	});

	// The Rust client answers `cancel` to every mode but `form` before it reaches
	// a host, so a mode this client never advertised must never be drawn — a form
	// for it would collect values the agent cannot read back.
	it("draws nothing for a mode this client did not advertise", async () => {
		const { container } = await renderPanel();
		await settle();

		feed({
			kind: "elicitationRequested",
			requestId: "req-3",
			request: { mode: "confirm", sessionId: SESSION, message: "Proceed?", requestedSchema: {} },
		} as unknown as AcpClientEvent);
		await settle();

		expect(container.textContent).not.toContain("Proceed?");
	});
});

describe("elicitationFields", () => {
	it("reads a field per property, with its title and whether it is required", () => {
		expect(
			elicitationFields({
				type: "object",
				properties: {
					branch: { type: "string", title: "Branch" },
					depth: { type: "integer" },
					force: { type: "boolean" },
					mode: { type: "string", enum: ["fast", "safe"] },
				},
				required: ["branch"],
			}),
		).toEqual([
			{ name: "branch", label: "Branch", type: "string", choices: [], required: true },
			{ name: "depth", label: "depth", type: "number", choices: [], required: false },
			{ name: "force", label: "force", type: "boolean", choices: [], required: false },
			{ name: "mode", label: "mode", type: "enum", choices: ["fast", "safe"], required: false },
		]);
	});

	// A nested object has no single control to draw, and guessing one would
	// collect a value the agent cannot read back.
	it("skips a property it cannot draw one control for", () => {
		expect(
			elicitationFields({ type: "object", properties: { nested: { type: "object" }, list: { type: "array" } } }),
		).toEqual([]);
	});

	it("reads no field out of a schema it cannot understand", () => {
		expect(elicitationFields(null)).toEqual([]);
		expect(elicitationFields({ type: "string" })).toEqual([]);
	});
});

describe("AIChatPanel: a gap", () => {
	it("replays every open tab after the ACP connection is replaced", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		acpStore.applyFrame(CONNECTION, {
			kind: "gap",
			code: "stream_gap",
			message: "journal expired",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});
		await settle();
		(
			[...container.querySelectorAll("button")].find((button) => button.textContent === "Recover") as HTMLButtonElement
		).click();
		await settle();
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SESSION, CHAT_ROOT);
	});
	// A gap says the journal no longer holds what the cursor asks for. Skipping
	// ahead would leave a hole in the conversation that nothing on screen admits
	// to; the recovery on record is a fresh process replaying the history.
	it("surfaces the gap and offers the recovery", async () => {
		const { container } = await renderPanel();
		await settle();

		acpStore.applyFrame(CONNECTION, {
			kind: "gap",
			code: "stream_gap",
			message: "sequence 3 is no longer held",
			connectionId: CONNECTION,
			sessionId: null,
			operation: null,
			retryable: false,
		});
		await settle();

		expect(container.textContent).toContain("Missed part of this conversation");
		const recover = [...container.querySelectorAll("button")].find((button) => button.textContent === "Recover");
		recover?.click();
		await settle();

		expect(client.reconnect).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SESSION, CHAT_ROOT);
	});
});

describe("AIChatPanel: the session's own knobs", () => {
	const mode: AcpSessionConfigOption = {
		id: "mode",
		name: "Mode",
		description: "How ego handles tools",
		type: "select",
		currentValue: "ask",
		options: [
			{ value: "ask", name: "Ask" },
			{ value: "auto", name: "Automatic" },
		],
	};

	// Catches: an ACP option is hidden or shown without its published name and choice.
	it("shows every published select with a label, description, and current choice in a dialog", async () => {
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ configOptions: [MODEL_OPTION, mode] })] }));
		await settle();

		expect(container.textContent).toContain("Opus");
		expect(container.textContent).toContain("Ask");
		expect(container.querySelectorAll(".controlBar select")).toHaveLength(0);
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const dialog = container.querySelector('[role="dialog"]') as HTMLElement;
		expect(dialog).not.toBeNull();
		expect(dialog.textContent).toContain("How ego handles tools");
		expect(
			(dialog.querySelector('select[aria-label="Model"]') as HTMLSelectElement).selectedOptions[0].textContent,
		).toBe("Opus");
		expect(
			(dialog.querySelector('select[aria-label="Mode"]') as HTMLSelectElement).selectedOptions[0].textContent,
		).toBe("Ask");
	});

	it("renders ACP grouped choices with their group label and selected value", async () => {
		const grouped: AcpSessionConfigOption = {
			id: "mode",
			name: "Mode",
			type: "select",
			currentValue: "auto",
			options: [
				{
					group: "behavior",
					name: "Behavior",
					options: [
						{ value: "ask", name: "Ask" },
						{ value: "auto", name: "Automatic" },
					],
				},
			],
		};
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ configOptions: [grouped] })] }));
		await settle();
		expect(container.querySelector(".controlBar")?.textContent).toContain("Mode: Automatic");
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const picker = container.querySelector('select[aria-label="Mode"]') as HTMLSelectElement;
		expect(picker.querySelector("optgroup")?.label).toBe("Behavior");
		expect(picker.selectedOptions[0].value).toBe("auto");
		expect(picker.selectedOptions[0].textContent).toBe("Automatic");
	});

	// Catches: a new tab keeps a blank select after its options arrive.
	it("shows the current choice when a second tab receives its options after opening", async () => {
		client.newSession.mockResolvedValueOnce(SESSION).mockResolvedValueOnce(SECOND_SESSION);
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment(),
					attachment({ sessionId: SECOND_SESSION, configOptions: [{ ...MODEL_OPTION, currentValue: "sonnet" }] }),
				],
			}),
		);
		await settle();
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const picker = container.querySelector('select[aria-label="Model"]') as HTMLSelectElement;
		expect(picker.selectedOptions[0].textContent).toBe("Sonnet");
	});

	// Catches: the summary optimistically reports a choice ego has not accepted.
	it("sends a changed choice and updates the summary only from the agent's reply", async () => {
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const picker = container.querySelector('select[aria-label="Model"]') as HTMLSelectElement;

		picker.value = "sonnet";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		expect(client.setConfigOption).toHaveBeenCalledWith(CONNECTION, SESSION, "model", { value: "sonnet" });
		expect(container.querySelector(".controlBar")?.textContent).toContain("Opus");
		acpStore.applySnapshot(
			snapshot({ attachments: [attachment({ configOptions: [{ ...MODEL_OPTION, currentValue: "sonnet" }] })] }),
		);
		await settle();
		expect(container.querySelector(".controlBar")?.textContent).toContain("Sonnet");
	});

	// Catches: a rejected choice appears accepted and its error is lost.
	it("shows a rejected setting change inside the dialog", async () => {
		client.setConfigOption.mockRejectedValueOnce(new Error("Model unavailable"));
		const { container } = await renderPanel();
		await settle();
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const picker = container.querySelector('select[aria-label="Model"]') as HTMLSelectElement;
		picker.value = "sonnet";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		expect(container.querySelector('[role="dialog"]')?.textContent).toContain("Model unavailable");
		expect(picker.value).toBe("opus");
	});
});

describe("AIChatPanel: fork", () => {
	const forkButton = (container: HTMLElement) =>
		container.querySelector<HTMLButtonElement>('button[aria-label="Fork the conversation"]');

	beforeEach(() => {
		supportsFork = true;
		// The real client attaches the child and refreshes the store before it returns.
		client.forkSession.mockImplementation(async () => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: CHILD_SESSION })] }));
			return CHILD_SESSION;
		});
	});

	// Catches: fork replacing the parent binding instead of adding a tab.
	it("opens the child in a new tab and keeps the parent tab", async () => {
		const { container } = await renderPanel();
		await settle();

		forkButton(container)?.click();
		await settle();

		expect(client.forkSession).toHaveBeenCalledWith(CONNECTION, SESSION, CHAT_ROOT);
		expect(aiChatTabs.ids("global")).toEqual([SESSION, CHILD_SESSION]);
		expect(aiChatTabs.active("global")).toBe(CHILD_SESSION);
		expect(acpStore.attachment(CONNECTION, SESSION)).not.toBeNull();
		// The fork already attached the child; a load would be refused as a duplicate.
		expect(client.loadSession).not.toHaveBeenCalledWith(CONNECTION, CHILD_SESSION, expect.anything());
	});

	// Catches: a fork request hitting ego's busy refusal while a turn streams.
	it("is disabled while a turn streams", async () => {
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "prompting" })] }));
		await settle();

		const button = forkButton(container);
		expect(button?.disabled).toBe(true);
		button?.click();
		await settle();

		expect(client.forkSession).not.toHaveBeenCalled();
	});

	// Catches: fork offered to an agent that cannot serve it.
	it("shows no button when the agent lacks the fork capability", async () => {
		supportsFork = false;
		const { container } = await renderPanel();
		await settle();

		expect(forkButton(container)).toBeNull();
	});
});

describe("AIChatPanel: pause, resume and compact", () => {
	// Catches: text controls wrapping below a long model summary or icons losing accessible names.
	it("keeps named icon controls on one row beside a short model summary", async () => {
		const style = document.createElement("style");
		style.textContent = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		document.head.append(style);
		try {
			const { container } = await renderPanel();
			await settle();
			acpStore.applySnapshot(
				snapshot({
					attachments: [
						attachment({
							state: "prompting",
							configOptions: [
								{
									...MODEL_OPTION,
									currentValue: "openai-codex/gpt-6-sol",
									options: [{ value: "openai-codex/gpt-6-sol", name: "openai-codex/gpt-6-sol" }],
								},
							],
						}),
					],
				}),
			);
			await settle();
			const bar = container.querySelector<HTMLElement>(".controlBar")!;
			expect(bar.querySelector(".sessionSettingsSummary")?.textContent).toBe("Model: gpt-6-sol");
			expect(getComputedStyle(bar).flexWrap).toBe("nowrap");
			for (const label of ["Pause the turn", "Compact the conversation", "Start another conversation"]) {
				const button = bar.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
				expect(button.title).toBe(label);
				expect(button.querySelector("svg")).not.toBeNull();
				expect(button.textContent?.trim()).toBe("");
			}
			acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "paused" })] }));
			await settle();
			const resume = bar.querySelector<HTMLButtonElement>('button[aria-label="Resume the turn"]')!;
			expect(resume.title).toBe("Resume the turn");
			expect(resume.querySelector("svg")).not.toBeNull();
		} finally {
			style.remove();
		}
	});

	it("pauses a running turn and resumes a held one", async () => {
		const { container } = await renderPanel();
		await settle();

		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "prompting" })] }));
		await settle();

		const pause = container.querySelector<HTMLButtonElement>('button[aria-label="Pause the turn"]');
		pause?.click();
		await settle();
		expect(client.pause).toHaveBeenCalledWith(CONNECTION, SESSION);

		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "paused" })] }));
		await settle();

		const resume = container.querySelector<HTMLButtonElement>('button[aria-label="Resume the turn"]');
		resume?.click();
		await settle();
		expect(client.resumeTurn).toHaveBeenCalledWith(CONNECTION, SESSION);
	});

	it("compacts the conversation", async () => {
		client.compact.mockResolvedValueOnce({
			sourceSessionId: SESSION,
			targetSessionId: "sess-compacted",
			publication: { kind: "published_durably", diagnostic: null },
		});
		const { container } = await renderPanel();
		await settle();

		const compact = container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]');
		compact?.click();
		await settle();

		expect(client.compact).toHaveBeenCalledWith(CONNECTION, SESSION);
	});

	it("opens the compacted successor session, which is where the conversation continues", async () => {
		client.compact.mockResolvedValueOnce({
			sourceSessionId: SESSION,
			targetSessionId: "sess-compacted",
			publication: { kind: "published_durably", diagnostic: null },
		});
		const { container } = await renderPanel();
		await settle();

		container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]')?.click();
		await settle();

		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, "sess-compacted", expect.anything());
	});

	it("reports a compaction ego did not publish instead of announcing success", async () => {
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		client.compact.mockResolvedValueOnce({
			sourceSessionId: SESSION,
			targetSessionId: "sess-compacted",
			publication: { kind: "not_published", diagnostic: "checkpoint write refused" },
		});
		const { container } = await renderPanel();
		await settle();

		container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]')?.click();
		await settle();

		expect(container.textContent).toContain("checkpoint write refused");
		expect(toastsStore.toasts.map((toast) => toast.title)).not.toContain("Conversation compacted");
		expect(client.loadSession).not.toHaveBeenCalled();
	});

	it("tells the person the conversation was compacted", async () => {
		client.compact.mockResolvedValueOnce({
			sourceSessionId: SESSION,
			targetSessionId: "sess-compacted",
			publication: { kind: "published_durably", diagnostic: null },
		});
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		const { container } = await renderPanel();
		await settle();

		container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]')?.click();
		await settle();

		expect(toastsStore.toasts.map((toast) => toast.title)).toContain("Conversation compacted");
	});

	it("shows why a refused compaction did nothing, and does not claim success", async () => {
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		client.compact.mockRejectedValueOnce(new Error("nothing to compact"));
		const { container } = await renderPanel();
		await settle();

		container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]')?.click();
		await settle();

		expect(container.textContent).toContain("nothing to compact");
		expect(toastsStore.toasts.map((toast) => toast.title)).not.toContain("Conversation compacted");
	});

	it("draws an unavailable control-bar button visibly dimmed", async () => {
		const style = document.createElement("style");
		style.textContent = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		document.head.append(style);
		try {
			const { container } = await renderPanel();
			await settle();
			const pause = container.querySelector<HTMLButtonElement>('button[aria-label="Pause the turn"]')!;
			expect(pause.disabled).toBe(true);
			expect(Number(getComputedStyle(pause).opacity)).toBeLessThan(1);
			expect(getComputedStyle(pause).cursor).toBe("not-allowed");
		} finally {
			style.remove();
		}
	});

	// An ego that did not advertise the extension gets no button, rather than a
	// button that fails when it is pressed.
	it("offers neither when the agent did not advertise them", async () => {
		client.connect.mockImplementation(async () => {
			const opened = snapshot({
				capabilities: { ...snapshot().capabilities!, egoHoldVersion: null, egoCompactVersion: null },
			});
			acpStore.applySnapshot(opened);
			acpStore.markStreaming(CONNECTION);
			return opened;
		});
		client.newSession.mockImplementation(async () => {
			acpStore.applySnapshot(
				snapshot({
					attachments: [attachment()],
					capabilities: { ...snapshot().capabilities!, egoHoldVersion: null, egoCompactVersion: null },
				}),
			);
			return SESSION;
		});

		const { container } = await renderPanel();
		await settle();

		expect(container.querySelector('button[aria-label="Pause the turn"]')).toBeNull();
		expect(container.querySelector('button[aria-label="Compact the conversation"]')).toBeNull();
	});
});

describe("AIChatPanel: provider retry status", () => {
	const CAUSE = "Our servers are currently overloaded. Please try again later.";
	function retry(attempt: number | null) {
		const providerRetry =
			attempt === null
				? null
				: {
						state: "waiting",
						attempt,
						limit: 6,
						delayMs: 1000 * 2 ** (attempt - 1),
						kind: "transient",
						cause: CAUSE,
						text: `${CAUSE} — connection problem, retrying in ${2 ** (attempt - 1)}s (attempt ${attempt}/6)`,
					};
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "session_info_update", _meta: { ego: { providerRetry } } } as never,
		});
	}
	const statusLines = (container: HTMLElement) => [...container.querySelectorAll("[class*=providerRetry]")];

	async function retryingPanel() {
		const view = await renderPanel();
		await settle();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "session_info_update", title: "Audit terminali" } });
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "prompting" })] }));
		await settle();
		return view;
	}

	// Catches: one line per retry, or a retry that never shows its n/6 counter.
	it("shows one status line that a later attempt updates in place", async () => {
		const { container } = await retryingPanel();
		retry(1);
		await settle();
		expect(statusLines(container)).toHaveLength(1);
		expect(statusLines(container)[0].textContent).toContain("connection problem");
		expect(statusLines(container)[0].textContent).toContain("attempt 1/6");

		retry(2);
		await settle();
		expect(statusLines(container)).toHaveLength(1);
		expect(statusLines(container)[0].textContent).toContain("attempt 2/6");
		expect(statusLines(container)[0].textContent).not.toContain("attempt 1/6");
		// The retry patch carries no title, so the conversation keeps its name.
		expect(acpTranscript.title(SESSION)).toBe("Audit terminali");
	});

	// Catches: a stale retry line left on screen after the provider recovered.
	it("removes the line when ego clears the retry", async () => {
		const { container } = await retryingPanel();
		retry(3);
		await settle();
		expect(statusLines(container)).toHaveLength(1);

		retry(null);
		await settle();
		expect(statusLines(container)).toHaveLength(0);
	});

	// Catches: the retry line surviving next to the final error once 6/6 failed.
	it("replaces the line with the final error when the turn fails", async () => {
		const { container } = await retryingPanel();
		retry(6);
		await settle();
		expect(statusLines(container)).toHaveLength(1);
		feed({ kind: "turnFailed", message: "the turn failed: the provider returned HTTP 503", state: "idle" });
		await settle();

		expect(statusLines(container)).toHaveLength(0);
		expect(container.textContent).toContain("the turn failed: the provider returned HTTP 503");
	});
});

describe("AIChatPanel: fork at message", () => {
	// Catches: offering a message fork to an agent that can only fork at its tip.
	it("hides the per-reply fork without the at-message capability", async () => {
		const { container } = await renderPanel();
		await settle();
		acpTranscript.restore(SESSION, [{ id: "reply", kind: "agent", text: "Answer", messageId: "reply-id" }]);
		await settle();
		expect(container.querySelector('[aria-label="Fork from here"]')).toBeNull();
	});
	// Catches: the selected reply ID lost between the rendered action and the shared client.
	it("passes the selected reply ID from the per-message button", async () => {
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(
			snapshot({ capabilities: { ...snapshot().capabilities!, fork: true, forkAtMessage: true } }),
		);
		acpTranscript.restore(SESSION, [{ id: "reply", kind: "agent", text: "Answer", messageId: "reply-id" }]);
		client.forkSession.mockResolvedValue(CHILD_SESSION);
		await settle();
		container.querySelector<HTMLButtonElement>('[aria-label="Fork from here"]')?.click();
		await settle();
		expect(client.forkSession).toHaveBeenCalledWith(CONNECTION, SESSION, CHAT_ROOT, "reply-id");
	});
});

// Catches: inherited replay shown as the child's own conversation or hidden entirely.
it("renders inherited replay above a boundary and child turns below", async () => {
	const { container } = await renderPanel();
	await settle();
	feed({
		kind: "sessionUpdate",
		update: {
			sessionUpdate: "user_message_chunk",
			content: { type: "text", text: "Parent question" },
			_meta: { ego: { inherited: true } },
		},
	});
	feed({
		kind: "sessionUpdate",
		update: {
			sessionUpdate: "agent_message_chunk",
			messageId: "parent",
			content: { type: "text", text: "Parent answer" },
			_meta: { ego: { inherited: true } },
		},
	});
	feed({
		kind: "sessionUpdate",
		update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "Child question" } },
	});
	feed({
		kind: "sessionUpdate",
		update: {
			sessionUpdate: "agent_message_chunk",
			messageId: "child",
			content: { type: "text", text: "Child answer" },
		},
	});
	await settle();
	const transcript = container.querySelector('[aria-label="Chat transcript"]')!;
	const boundary = transcript.querySelector('[role="separator"][aria-label="Inherited history ends"]');
	expect(boundary).not.toBeNull();
	const content = transcript.textContent!;
	expect(content.indexOf("Parent answer")).toBeLessThan(content.indexOf("This conversation"));
	expect(content.indexOf("Child question")).toBeGreaterThan(content.indexOf("This conversation"));
	expect(content).toContain("Child answer");
});

describe("AIChatPanel: lineage picker", () => {
	// Catches: refreshSessions resorting the backend tree or discarding its indentation/deleted rows.
	it("keeps two-level ancestry and a disabled deleted parent in the picker", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{
					sessionId: "root",
					cwd: CHAT_ROOT,
					title: "Root",
					updatedAt: "2026-01-01",
					_meta: { tuicommander: { lineageDepth: 0 } },
				},
				{
					sessionId: "deleted",
					cwd: CHAT_ROOT,
					title: "Deleted conversation",
					_meta: { tuicommander: { lineageDepth: 1, deleted: true } },
				},
				{
					sessionId: "grandchild",
					cwd: CHAT_ROOT,
					title: "Grandchild",
					updatedAt: "2026-10-05",
					_meta: {
						ego: { lineage: { kind: "fork", sourceSessionId: "deleted", rootSessionId: "root", sourceDeleted: true } },
						tuicommander: { lineageDepth: 2 },
					},
				},
			],
		});
		const { container } = await renderPanel();
		await settle();
		const rows = [...container.querySelectorAll<HTMLOptionElement>('select[title="Conversation"] option')];
		expect(rows.map((row) => row.value)).toEqual(["root", "deleted", "grandchild"]);
		expect(rows[1].disabled).toBe(true);
		expect(rows[2].textContent).toContain("    ");
		expect(rows[2].textContent).toContain("Grandchild");
	});
});

describe("conversation launch overrides", () => {
	// Catches: creating a coordinator chat changes settings or sends its next turn through the daily connection.
	it("opens options on a dedicated conversation and returns default chats to their connection", async () => {
		const view = await renderPanel();
		const customConnection = "01932d5e-0000-7000-8000-0000000000c2";
		const launch = {
			executable: "/opt/observer/ego",
			profile: "coordinator",
			workspace: "/srv/observer",
			peerId: "observer-peer",
		};
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {}, ai_chat_launches: { [SECOND_SESSION]: launch } };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			return undefined;
		});
		client.openConversation.mockImplementation(async () => {
			const connection = snapshot({
				connectionId: customConnection,
				attachments: [attachment({ sessionId: SECOND_SESSION })],
			});
			acpStore.applySnapshot(connection);
			acpStore.markStreaming(customConnection);
			return { connection, sessionId: SECOND_SESSION, launch };
		});
		(view.container.querySelector('button[aria-label="New conversation with options"]') as HTMLButtonElement).click();
		await settle();
		for (const [label, value] of [
			["Ego profile", "coordinator"],
			["Workspace", "/srv/observer"],
			["Ego executable", "/opt/observer/ego"],
		]) {
			const input = view.container.querySelector(`input[aria-label="${label}"]`) as HTMLInputElement;
			input.value = value;
			input.dispatchEvent(new Event("input", { bubbles: true }));
		}
		(
			[...view.container.querySelectorAll("button")].find(
				(button) => button.textContent === "Create",
			) as HTMLButtonElement
		).click();
		await settle();
		expect(client.openConversation).toHaveBeenCalledWith({
			profile: "coordinator",
			workspace: "/srv/observer",
			executable: "/opt/observer/ego",
		});
		expect(view.container.textContent).toContain("coordinator · /srv/observer · /opt/observer/ego");
		await typeAndSend(view.container, "observe");
		expect(client.prompt).toHaveBeenLastCalledWith(customConnection, SECOND_SESSION, "observe", [], ROOT);
		(view.container.querySelector(`button[data-chat-session="${SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		await typeAndSend(view.container, "daily");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "daily", [], ROOT);
		expect(settings.egoExecutable).toBe("/usr/local/bin/ego");
	});

	// Catches: a webview reload restores a custom tab on the global connection and loses its launch options.
	it("reopens a persisted custom tab without starting the default ego", async () => {
		const launch = {
			executable: "/opt/observer/ego",
			profile: "coordinator",
			workspace: "/srv/observer",
			peerId: "observer-peer",
		};
		aiChatTabs.add("global", SECOND_SESSION);
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_launches: { [SECOND_SESSION]: launch } };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			return undefined;
		});
		client.openConversation.mockImplementation(async () => {
			const connection = snapshot({ attachments: [attachment({ sessionId: SECOND_SESSION })] });
			acpStore.applySnapshot(connection);
			acpStore.markStreaming(CONNECTION);
			return { connection, sessionId: SECOND_SESSION, launch };
		});
		const view = renderIdlePanel();
		await settle();
		await typeAndSend(view.container, "resume observation");
		expect(client.openConversation).toHaveBeenCalledWith({ sessionId: SECOND_SESSION });
		expect(client.connect).not.toHaveBeenCalled();
		expect(view.container.textContent).toContain("coordinator · /srv/observer · /opt/observer/ego");
	});
});

describe("AIChatPanel: new chat after saved-tab selection", () => {
	// Catches a failed saved selection swallowing the first prompt in a newly opened chat.
	it("sends to the new chat after a saved tab fails to load", async () => {
		const view = await renderPanel();
		aiChatTabs.add("global", SECOND_SESSION);
		client.loadSession.mockRejectedValue(new Error("Saved conversation unavailable"));
		(view.container.querySelector(`[data-chat-session="${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(view.container.textContent).toContain("Saved conversation unavailable");

		client.newSession.mockImplementation(async () => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: CHILD_SESSION })] }));
			return CHILD_SESSION;
		});
		(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		expect(view.container.querySelector(`[data-chat-session="${CHILD_SESSION}"]`)?.getAttribute("aria-selected")).toBe(
			"true",
		);
		await typeAndSend(view.container, "Work in this new conversation");
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, CHILD_SESSION, "Work in this new conversation", [], ROOT);
	});

	// Catches an older saved-tab replay replacing the chat the user opened while it was loading.
	it("keeps the new chat selected when the previous saved-tab replay completes", async () => {
		const view = await renderPanel();
		aiChatTabs.add("global", SECOND_SESSION);
		let completeReplay!: () => void;
		client.loadSession.mockImplementation(
			() =>
				new Promise<void>((resolve) => {
					completeReplay = resolve;
				}),
		);
		(view.container.querySelector(`[data-chat-session="${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(view.container.textContent).toContain("Connecting conversation");
		client.newSession.mockImplementation(async () => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: CHILD_SESSION })] }));
			return CHILD_SESSION;
		});
		(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		expect(view.container.querySelector(`[data-chat-session="${CHILD_SESSION}"]`)?.getAttribute("aria-selected")).toBe(
			"true",
		);
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment(),
					attachment({ sessionId: SECOND_SESSION }),
					attachment({ sessionId: CHILD_SESSION }),
				],
			}),
		);
		completeReplay();
		await settle();
		expect(view.container.querySelector(`[data-chat-session="${CHILD_SESSION}"]`)?.getAttribute("aria-selected")).toBe(
			"true",
		);
	});
});

describe("AIChatPanel: new chat during cold saved connection", () => {
	// Catches New chat waiting for an older saved replay and aborting when that replay fails.
	it("opens a usable new chat when the saved connection was still pending at the click", async () => {
		aiChatTabs.add("global", SECOND_SESSION);
		let finishConnect!: () => void;
		const waiting = new Promise<void>((resolve) => {
			finishConnect = resolve;
		});
		client.connect.mockImplementation(async () => {
			await waiting;
			const opened = snapshot();
			acpStore.applySnapshot(opened);
			acpStore.markStreaming(CONNECTION);
			return opened;
		});
		client.loadSession.mockRejectedValue(new Error("Saved conversation is unavailable"));
		const view = renderIdlePanel();
		await settle();
		(view.container.querySelector(`[data-chat-session="${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		expect(client.connect).toHaveBeenCalledTimes(1);
		(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		finishConnect();
		await settle();
		expect(view.container.querySelector(`[data-chat-session="${SESSION}"]`)?.getAttribute("aria-selected")).toBe(
			"true",
		);
		await typeAndSend(view.container, "Continue in the fresh conversation");
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Continue in the fresh conversation", [], ROOT);
	});
});

describe("desktop composer icon controls", () => {
	it("keeps icon actions named and disabled for an empty draft, and marks parked drafts pressed", async () => {
		// Catches: replacing text with SVG drops accessible names, enables empty sends, or hides parked state.
		const view = await renderPanel();
		const send = view.getByRole("button", { name: "Send" }) as HTMLButtonElement;
		const park = view.getByRole("button", { name: "Park draft" }) as HTMLButtonElement;
		expect(send.title).toBe("Send");
		expect(send.disabled).toBe(true);
		expect(park.disabled).toBe(true);
		expect(park.getAttribute("aria-pressed")).toBe("false");
		expect(send.querySelector("svg")).not.toBeNull();
		expect(park.querySelector("svg")).not.toBeNull();
		expect(send.textContent).toBe("");
		expect(park.textContent).toBe("");
		const textarea = view.container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Keep this draft";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		expect(send.disabled).toBe(false);
		expect(park.disabled).toBe(false);
		park.click();
		expect(park.getAttribute("aria-label")).toBe("Restore parked draft");
		expect(park.title).toBe("Restore parked draft");
		expect(park.getAttribute("aria-pressed")).toBe("true");
		expect(park.disabled).toBe(false);
		expect(send.disabled).toBe(true);
		textarea.value = "Next message";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		expect(park.getAttribute("aria-label")).toBe("Swap parked draft");
		expect(park.title).toBe("Swap parked draft");
		expect(park.getAttribute("aria-pressed")).toBe("true");
		expect(send.disabled).toBe(false);
		park.click();
		expect(textarea.value).toBe("Keep this draft");
	});

	it("names the busy play icon Queue and keeps empty prompts disabled", async () => {
		// Catches: busy changes remove the icon name or allow an empty queued turn.
		const view = await renderPanel();
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						state: "prompting",
						activeTurn: {
							turnId: "running",
							state: "running",
							stopReason: null,
							usage: null,
						},
					}),
				],
			}),
		);
		await settle();
		const queue = view.getByRole("button", { name: "Queue" }) as HTMLButtonElement;
		expect(queue.title).toBe("Queue");
		expect(queue.disabled).toBe(true);
		expect(queue.querySelector("svg")).not.toBeNull();
		const park = view.getByRole("button", { name: "Park draft" }) as HTMLButtonElement;
		expect(park.disabled).toBe(true);
		expect(park.getAttribute("aria-pressed")).toBe("false");
		const textarea = view.container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Queue this";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		expect(queue.disabled).toBe(false);
		expect(park.disabled).toBe(false);
		queue.click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Queue this", [], ROOT);
	});
});

describe("AI Chat shared image paste", () => {
	it("connects before checking image support when a PNG is pasted into an unstarted chat", async () => {
		// Catches: absent pre-connection capabilities are mistaken for explicit image refusal.
		supportsImages = true;
		const { container } = renderIdlePanel();
		await settle();
		expect(client.connect).not.toHaveBeenCalled();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const event = pasteFile(
			textarea,
			new File([Uint8Array.from(atob(PNG_1X1), (byte) => byte.charCodeAt(0))], "clip.png", { type: "image/png" }),
		);
		expect(event.defaultPrevented).toBe(true);
		await vi.waitFor(() => expect(container.querySelector('img[alt="Pasted image"]')).not.toBeNull());
		expect(client.connect).toHaveBeenCalled();
		(container.querySelector('button[aria-label="Send"]') as HTMLButtonElement).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(
			CONNECTION,
			SESSION,
			"",
			[{ type: "image", mimeType: "image/png", data: PNG_1X1 }],
			ROOT,
		);
	});
});

describe("AI Chat clipboard precedence", () => {
	function mixedPaste(textarea: HTMLTextAreaElement, text: string): Event {
		const file = new File([Uint8Array.from(atob(PNG_1X1), (byte) => byte.charCodeAt(0))], "clip.png", {
			type: "image/png",
		});
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", {
			value: {
				items: [
					{ type: "text/plain", getAsFile: () => null },
					{ type: "image/png", getAsFile: () => file },
				],
				getData: (type: string) => (type === "text/plain" ? text : ""),
			},
		});
		textarea.dispatchEvent(event);
		return event;
	}

	it("attaches Finder image copies instead of sending their file-name text", async () => {
		// Catches: shared text precedence rejects a Finder file-name plus image clipboard.
		supportsImages = true;
		const { container } = await renderPanel();
		const event = mixedPaste(
			container.querySelector("textarea") as HTMLTextAreaElement,
			"/Users/Boss/Pictures/clip.png",
		);
		expect(event.defaultPrevented).toBe(true);
		await vi.waitFor(() => expect(container.querySelector('img[alt="Pasted image"]')).not.toBeNull());
		(container.querySelector('button[aria-label="Send"]') as HTMLButtonElement).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(
			CONNECTION,
			SESSION,
			"",
			[{ type: "image", mimeType: "image/png", data: PNG_1X1 }],
			ROOT,
		);
	});

	it("preserves normal text paste and gives substantive mixed clipboard text precedence", async () => {
		// Catches: an incidental rendered image swallows spreadsheet text or bypasses compact-paste handling.
		supportsImages = true;
		const { container } = await renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		const event = mixedPaste(textarea, "copied spreadsheet cells");
		expect(event.defaultPrevented).toBe(false);
		await settle();
		expect(container.querySelector('img[alt="Pasted image"]')).toBeNull();
		const longText = Array.from({ length: 201 }, (_, index) => `word${index}`).join(" ");
		const longPaste = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(longPaste, "clipboardData", { value: { items: [], getData: () => longText } });
		textarea.dispatchEvent(longPaste);
		expect(longPaste.defaultPrevented).toBe(true);
		expect(textarea.value).toBe("[Pasted text #1 +201 words]");
		(container.querySelector('button[aria-label="Send"]') as HTMLButtonElement).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, longText, [], ROOT);
	});
});
