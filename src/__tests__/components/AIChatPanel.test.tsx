// @vitest-environment jsdom
//
// The panel renders an agent's answer through ContentRenderer, whose DOMPurify
// pass needs a complete NodeIterator; happy-dom's is not.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, render } from "@solidjs/testing-library";
import { createRoot, createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import recordedCommands from "../fixtures/ego-available-commands-2393.json";
import missingCatalogue from "../fixtures/ego-missing-catalogue-2383.json";
import missingSession from "../fixtures/ego-missing-session-2348.json";
import recordedModelChange from "../fixtures/ego-model-change-2388.json";
import recordedProviderRetry from "../fixtures/ego-provider-retry.json";
import { chatFontSize, loadChatTypography } from "../helpers/chatFontSize";
import { cleanupToasts } from "../helpers/toasts";

const { mockDetachPanel, mockReattachPanel, mockClosePanel } = vi.hoisted(() => ({
	mockDetachPanel: vi.fn().mockResolvedValue(undefined),
	mockReattachPanel: vi.fn().mockResolvedValue(undefined),
	mockClosePanel: vi.fn().mockResolvedValue(undefined),
}));

const { mockWriteClipboard, mockOpenFile, mockOpenUrl, mockAddVirtual } = vi.hoisted(() => ({
	mockAddVirtual: vi.fn(),
	mockWriteClipboard: vi.fn().mockResolvedValue(undefined),
	mockOpenFile: vi.fn(),
	mockOpenUrl: vi.fn(),
}));
const { mockSetFolderRoot, mockShowFileBrowser } = vi.hoisted(() => ({
	mockSetFolderRoot: vi.fn(),
	mockShowFileBrowser: vi.fn(),
}));

vi.mock("../../stores/mdTabs", () => ({ mdTabsStore: { addVirtual: mockAddVirtual } }));

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
	subscribeEvents: vi.fn(async () => () => {}),
	isTauri: () => true,
	owningConnectionFor: () => undefined,
	rpc: vi.fn(async () => ({})),
}));

// Whether an ego binary is configured is the one setting this panel reads, and
// criterion 9 turns on it being readable as "not configured" rather than as an
// empty string nobody checked.
const settings = vi.hoisted(() => ({ egoExecutable: "/usr/local/bin/ego", defaultFontSize: 13 }));

vi.mock("../../stores/settings", async () => {
	const { createSignal } = await import("solid-js");
	const [executable, setExecutable] = createSignal(settings.egoExecutable);
	Object.defineProperty(settings, "egoExecutable", { get: executable, set: setExecutable });
	return {
		settingsStore: {
			state: settings,
			isAiChatEnabled: () => true,
			setEgoExecutable: (path: string) => {
				settings.egoExecutable = path;
			},
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
	closeSession: vi.fn(),
	listSessions: vi.fn(),
	prompt: vi.fn(),
	cancel: vi.fn(),
	cancelQueued: vi.fn(),
	recallQueued: vi.fn().mockResolvedValue(null),
	answerPermission: vi.fn(),
	cancelPermission: vi.fn(),
	answerElicitation: vi.fn(),
	setConfigOption: vi.fn(),
	pause: vi.fn(),
	resumeTurn: vi.fn(),
	compact: vi.fn(),
	forkSession: vi.fn(),
	deleteSession: vi.fn(),
}));

vi.mock("../../services/acpClient", () => ({ acpClient: client }));

import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { AIChatPanel } from "../../components/AIChatPanel/AIChatPanel";
import { Composer } from "../../components/AIChatPanel/Composer";
import { aiChatDraft } from "../../components/AIChatPanel/draft";
import { elicitationFields } from "../../components/AIChatPanel/Interactions";
import { createAcpChat, resetAcpChatBindings } from "../../components/AIChatPanel/useAcpChat";
import { useTerminalLifecycle } from "../../hooks/useTerminalLifecycle";
import { useKeyboardCorrection } from "../../mobile/useKeyboardCorrection";
import { aiChatPanelAdapter } from "../../panelAdapters/aiChat";
import { acpStore } from "../../stores/acp";
import { acpTranscript } from "../../stores/acpTranscript";
import { aiChatTabs } from "../../stores/aiChatTabs";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { subscribeEvents } from "../../transport";
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
let supportsDelete = false;
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
			delete: supportsDelete,
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

/** Mirror acpClient.newSession: its successful result includes a refreshed
 * snapshot retaining the connection's other attachments. */
async function openSession(session: string): Promise<string> {
	const current = acpStore.connection(CONNECTION)?.attachments ?? [];
	acpStore.applySnapshot(
		snapshot({
			attachments: [...current.filter((row) => row.sessionId !== session), attachment({ sessionId: session })],
		}),
	);
	return session;
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

/** Opening the configured panel automatically starts or restores its chat. */
async function renderPanel() {
	const view = render(() => <AIChatPanel visible={true} repoPath={ROOT} onClose={() => {}} />);
	await settle();
	return view;
}

/** Type a message and press the active Send or Queue control. */
async function typeAndSend(container: HTMLElement, text: string): Promise<void> {
	const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
	textarea.value = text;
	textarea.dispatchEvent(new Event("input", { bubbles: true }));
	(
		[...container.querySelectorAll("button")].find((button) =>
			["Send", "Queue"].includes(button.getAttribute("aria-label") ?? ""),
		) as HTMLButtonElement
	).click();
	await settle();
}

/** Open the panel without waiting for its automatic attachment. */
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
	supportsDelete = false;
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
	client.newSession.mockImplementation(() => openSession(SESSION));
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
		"closeSession",
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

// Catches: replacing a refused push-linked conversation restores that same refused tab instead of opening a new one.
it("starts a new chat after closing a refused push-linked conversation in a fresh document", async () => {
	aiChatTabs.add("global", SECOND_SESSION);
	client.loadSession.mockRejectedValueOnce(
		new Error(
			'RPC acp_session_load failed: 502 {"kind":"agent_error","message":"the session last run has an uncertain effect; close it with session/close and start a new chat","retryable":false}',
		),
	);
	let dispose = () => {};
	const chat = createRoot((cleanup) => {
		dispose = cleanup;
		return createAcpChat(
			() => ROOT,
			() => true,
			client,
		);
	});
	try {
		await chat.selectSession(SECOND_SESSION);
		expect(chat.error()).toContain("uncertain effect");
		await chat.closeFailed();
		await chat.startSession();
		expect(chat.sessionId()).toBe(SESSION);
		expect(chat.error()).toBeNull();
	} finally {
		dispose();
	}
});

// Catches: a hot-reloaded frontend offers Retry for a command absent from the running backend.
it("dev_missing_command_explains_backend_frontend_mismatch_without_retry", async () => {
	vi.mocked(invoke).mockImplementation(async (command) => {
		if (command === "load_config") return { ai_chat_sessions: {} };
		if (command === "acp_workspace_root") return CHAT_ROOT;
		if (command === "select_ai_chat_conversation") {
			// Recorded incident: story 2409-aa48, Boss's 2026-10-28 AI Chat error.
			throw "Command select_ai_chat_conversation not found";
		}
		return undefined;
	});
	const { container } = await renderPanel();
	await vi.waitFor(() => expect(container.textContent).toContain("Backend/frontend version mismatch"));
	expect(container.textContent).toContain("Restart make dev");
	expect(container.textContent).toContain("select_ai_chat_conversation");
	expect([...container.querySelectorAll("button")].some((button) => button.textContent === "Retry")).toBe(false);
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

// Catches: AI Chat stays at its fixed size when the active terminal zooms or changes.
it("ai_chat_follows_active_terminal_zoom_and_resets_without_a_terminal", async () => {
	const removeStyles = loadChatTypography();
	const previous = terminalsStore.state.activeId;
	const first = terminalsStore.add({
		name: "zoom one",
		sessionId: "zoom-one",
		fontSize: 13,
		cwd: ROOT,
		awaitingInput: null,
	});
	const second = terminalsStore.add({
		name: "zoom two",
		sessionId: "zoom-two",
		fontSize: 19,
		cwd: ROOT,
		awaitingInput: null,
	});
	terminalsStore.setActive(first);
	let zoom!: ReturnType<typeof useTerminalLifecycle>;
	const view = render(() => {
		zoom = useTerminalLifecycle({
			pty: { canSpawn: async () => true, close: async () => {} },
			dialogs: {
				confirmCloseTerminal: async () => false,
				confirmSaveChanges: async () => "cancel",
				confirm: async () => false,
			},
			setStatusInfo: () => {},
			getDefaultFontSize: () => 13,
		});
		return <AIChatPanel visible={true} repoPath={ROOT} onClose={() => {}} />;
	});
	await settle();
	(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
	await settle();
	acpTranscript.restore(SESSION, [{ id: "zoom-answer", kind: "agent", text: "Readable zoom answer" }]);
	await settle();
	const size = () => chatFontSize(view.getByText("Readable zoom answer").closest<HTMLElement>(".assistantMsg")!);
	try {
		expect(size()).toBe("14px");
		zoom.zoomIn();
		expect(size()).toBe("16px");
		zoom.zoomOut();
		expect(size()).toBe("14px");
		terminalsStore.setActive(second);
		expect(size()).toBe("20px");
		zoom.zoomReset();
		expect(size()).toBe("14px");
		terminalsStore.remove(second);
		terminalsStore.remove(first);
		terminalsStore.setActive(null);
		expect(size()).toBe("14px");
	} finally {
		view.unmount();
		removeStyles();
		terminalsStore.remove(first);
		terminalsStore.remove(second);
		if (previous) terminalsStore.setActive(previous);
	}
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
	// Catches: out-of-flow assistant actions escaping their message, oversized user bubbles, or wrapping tool counts.
	it("keeps assistant actions in flow, user bubbles compact and tool counts on one line", async () => {
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
			// The real-browser aiChatGeometry regression measures containment and
			// nonintersection; this DOM environment only verifies normal flow.
			expect(["absolute", "fixed"]).not.toContain(getComputedStyle(actions).position);
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
		expect(container.querySelector('[role="status"][aria-label="Working…"]')).not.toBeNull();
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
	it("copies whole messages including fenced code without redundant code-only actions", async () => {
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
		expect(mockWriteClipboard.mock.calls.map(([text]) => text)).not.toContain("echo three");
		expect(container.querySelector("button[data-copy-code]")).toBeNull();
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

	it("connects when the panel opens but starts nothing else when the repository changes", async () => {
		const { setRepo } = renderSwitchable();
		await settle();
		setRepo("/repo/other");
		await settle();
		setRepo(null);
		await settle();
		expect(client.connect).toHaveBeenCalledExactlyOnceWith(CHAT_ROOT);
		expect(client.newSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, CHAT_ROOT);
		expect(client.loadSession).not.toHaveBeenCalled();
	});

	it("starts one ego on panel open, in the workspace root, and a second tab reuses it", async () => {
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
		const { container } = renderSwitchable();
		await settle();
		await send(container, "hello");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.connect).toHaveBeenCalledWith(CHAT_ROOT);
		expect(client.newSession).toHaveBeenCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.prompt).toHaveBeenCalledTimes(1);
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		// Catches: removal of the duplicate bar action also removing the tab action.
		expect(container.querySelector('button[aria-label="Start another conversation"]')).toBeNull();
		expect(client.newSession).toHaveBeenCalledTimes(2);
		expect(client.newSession).toHaveBeenLastCalledWith(CONNECTION, CHAT_ROOT);
		expect(client.connect).toHaveBeenCalledTimes(1);
	});

	it("keeps the same tabs and the same chat when the repository changes", async () => {
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
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
			if (command === "ai_chat_selection") return { workspace: CHAT_ROOT, sessionId: SESSION };
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
	// Catches: a late startup owner read replaces an explicit tab click (also used by mobile push links).
	it("keeps an explicit tab selection when the initial shared selection arrives late", async () => {
		let finishSubscription!: (unsubscribe: () => void) => void;
		vi.mocked(subscribeEvents).mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finishSubscription = resolve;
				}),
		);
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "load_config") return { ai_chat_sessions: {} };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "ai_chat_selection") return { workspace: CHAT_ROOT, sessionId: SESSION };
			return undefined;
		});
		client.loadSession.mockImplementation(async (_id, session) => {
			await openSession(session);
		});
		aiChatTabs.add("global", SESSION);
		aiChatTabs.add("global", SECOND_SESSION);
		aiChatTabs.add("global", SESSION);
		const { container } = renderIdlePanel();
		await settle();
		(container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`) as HTMLButtonElement).click();
		await settle();
		finishSubscription(() => {});
		await settle();
		expect(
			container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`)?.getAttribute("aria-selected"),
		).toBe("true");
		expect(vi.mocked(invoke)).toHaveBeenCalledWith("select_ai_chat_conversation", { sessionId: SECOND_SESSION });
		expect(client.prompt).not.toHaveBeenCalled();
	});

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
		expect(container.textContent).toContain("Opus");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.prompt).not.toHaveBeenCalled();
	});

	// Catches: launch commits a saved id before load succeeds, so the next send
	// treats that unconfirmed connection/session pair as an attached conversation.
	it("never sends to a saved conversation after its attachment was refused", async () => {
		aiChatTabs.add("global", SECOND_SESSION);
		client.loadSession.mockRejectedValue(new Error("MCP admission refused"));
		await createRoot(async (dispose) => {
			const chat = createAcpChat(
				() => ROOT,
				() => true,
			);
			await chat.send("First attempt");
			await chat.send("Second attempt");
			expect(client.prompt).not.toHaveBeenCalled();
			expect(chat.error()).toContain("MCP admission refused");
			expect(client.newSession).not.toHaveBeenCalled();
			dispose();
		});
	});

	// Catches: a remounted mobile Chat combines the module's connection with a
	// different localStorage tab and sends before that tab has ever been loaded.
	it("attaches the selected saved tab after remount before sending", async () => {
		await createRoot(async (dispose) => {
			const chat = createAcpChat(
				() => ROOT,
				() => true,
			);
			await chat.startSession();
			dispose();
		});
		aiChatTabs.add("global", SECOND_SESSION);
		client.loadSession.mockImplementation(async (_id, session) => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: session })] }));
		});
		await createRoot(async (dispose) => {
			const chat = createAcpChat(
				() => ROOT,
				() => true,
			);
			await settle();
			await chat.send("Phone reply");
			expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
			expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, "Phone reply", [], ROOT);
			dispose();
		});
	});

	// Catches: the optional session/list refusal replaces successful attachment
	// with a fatal missing-session error for both new and restored conversations.
	it.each(["new", "restored"])(
		"keeps a %s conversation usable when ego catalogue reports session not found",
		async (kind) => {
			client.listSessions.mockRejectedValue(missingCatalogue);
			client.loadSession.mockImplementation(async () => {
				acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
			});
			await createRoot(async (dispose) => {
				const chat = createAcpChat(
					() => ROOT,
					() => true,
				);
				if (kind === "new") await chat.startSession();
				else await chat.selectSession(SESSION);
				await settle();
				expect(chat.error()).toBeNull();
				expect(chat.phase()).toBe("live");
				await chat.send("Still connected");
				expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Still connected", [], ROOT);
				dispose();
			});
		},
	);

	// Catches: awaiting optional catalogue I/O leaves an attached new chat on
	// Connecting conversation and disables the composer indefinitely.
	it("enables an attached conversation before its catalogue request settles", async () => {
		let finish!: (value: { sessions: []; nextCursor: null }) => void;
		client.listSessions.mockReturnValue(
			new Promise((resolve) => {
				finish = resolve;
			}),
		);
		const { container } = await renderPanel();
		try {
			expect(container.textContent).not.toContain("Connecting conversation");
			expect(container.querySelector("textarea")?.disabled).toBe(false);
			await typeAndSend(container, "Do not wait for the picker");
			expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Do not wait for the picker", [], ROOT);
		} finally {
			finish({ sessions: [], nextCursor: null });
			await settle();
		}
	});

	// Catches: an old process's delayed discovery refusal poisons the new
	// connection after recovery even though the new catalogue succeeded.
	it("ignores a catalogue refusal from a replaced connection", async () => {
		let refuse!: (error: unknown) => void;
		client.listSessions.mockReturnValueOnce(
			new Promise((_resolve, reject) => {
				refuse = reject;
			}),
		);
		await createRoot(async (dispose) => {
			const chat = createAcpChat(
				() => ROOT,
				() => true,
			);
			await chat.startSession();
			client.reconnect.mockImplementation(async () => {
				const opened = snapshot({ connectionId: "replacement" });
				acpStore.applySnapshot(opened);
				return opened;
			});
			client.loadSession.mockImplementation(async (id, session) => {
				acpStore.applySnapshot(snapshot({ connectionId: id, attachments: [attachment({ sessionId: session })] }));
			});
			await chat.recover();
			refuse(missingCatalogue);
			await settle();
			expect(chat.error()).toBeNull();
			expect(chat.phase()).toBe("live");
			dispose();
		});
	});

	// Catches: a missing background tab publishes its refusal as the active
	// conversation's error when a desktop panel replays several saved tabs.
	it("keeps the selected conversation live when a background saved tab is missing", async () => {
		const first = await renderPanel();
		first.unmount();
		aiChatTabs.add("global", SECOND_SESSION);
		aiChatTabs.add("global", SESSION);
		client.loadSession.mockRejectedValue(missingSession);
		const { container } = renderIdlePanel();
		await settle();
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(container.textContent).not.toContain("session not found");
		await typeAndSend(container, "The selected chat still works");
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "The selected chat still works", [], ROOT);
	});

	// Catches: remount replays the selected missing tab without recovery, leaving
	// the desktop panel failed and its model controls pending until user action.
	it("recovers a missing selected saved tab on desktop remount without user action", async () => {
		const first = await renderPanel();
		first.unmount();
		aiChatTabs.add("global", SECOND_SESSION);
		aiChatTabs.resetMemory();
		client.loadSession.mockRejectedValue(missingSession);
		client.newSession.mockClear();
		client.newSession.mockImplementation(() => openSession(CHILD_SESSION));

		const { container } = renderIdlePanel();
		await settle();

		expect(container.textContent).not.toContain("session not found");
		expect(container.querySelector("textarea")).not.toBeNull();
		expect(container.textContent).toContain("Opus");
		expect(container.querySelector(`button[data-chat-session="${CHILD_SESSION}"]`)?.getAttribute("aria-selected")).toBe(
			"true",
		);
		expect(aiChatTabs.ids("global")).toEqual([SESSION, CHILD_SESSION]);
		expect(client.loadSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(client.newSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, CHAT_ROOT);
		expect(client.prompt).not.toHaveBeenCalled();
	});

	// Catches: a stale mobile-local tab permanently traps Chat on a session that
	// ego no longer stores; exercise both IPC and the browser HTTP error envelope.
	it.each(["ipc", "http"])("replaces a missing saved conversation before sending over %s", async (transport) => {
		aiChatTabs.add("global", SECOND_SESSION);
		client.loadSession.mockRejectedValue(
			transport === "ipc"
				? missingSession
				: new Error(`RPC acp_session_load failed: 502 ${JSON.stringify(missingSession)}`),
		);
		await createRoot(async (dispose) => {
			const chat = createAcpChat(
				() => ROOT,
				() => true,
			);
			await chat.send("Recover my phone chat");
			expect(client.newSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, CHAT_ROOT);
			expect(client.prompt).toHaveBeenCalledExactlyOnceWith(CONNECTION, SESSION, "Recover my phone chat", [], ROOT);
			expect(aiChatTabs.ids("global")).toEqual([SESSION]);
			expect(chat.error()).toBeNull();
			expect(chat.configOptions()).toEqual([MODEL_OPTION]);
			dispose();
		});
	});

	// Catches: explicit tab selection has a separate load path and must not
	// keep a disappeared conversation pending after the first-send path recovers.
	it("replaces a missing explicitly selected conversation without sending", async () => {
		aiChatTabs.add("global", SECOND_SESSION);
		client.loadSession.mockRejectedValue(missingSession);
		await createRoot(async (dispose) => {
			const chat = createAcpChat(
				() => ROOT,
				() => true,
			);
			await chat.selectSession(SECOND_SESSION);
			expect(chat.sessionId()).toBe(SESSION);
			expect(chat.error()).toBeNull();
			expect(client.newSession).toHaveBeenCalledTimes(1);
			expect(client.prompt).not.toHaveBeenCalled();
			dispose();
		});
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
	it("shows connecting until a saved tab attaches, then its model below the input", async () => {
		restoreTabs();
		let finish!: () => void;
		client.loadSession.mockImplementation(async (_id, session) => {
			if (session !== SECOND_SESSION) {
				await openSession(session);
				return;
			}
			await new Promise<void>((resolve) => {
				finish = resolve;
			});
		});
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		expect(container.textContent).toContain("Connecting conversation…");
		expect(container.querySelector("textarea")).toBeNull();
		expect(
			container.querySelector(`button[data-chat-session="${SECOND_SESSION}"]`)?.getAttribute("aria-selected"),
		).toBe("true");
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: SECOND_SESSION })] }));
		finish();
		await settle();
		expect(container.textContent).toContain("Opus");
		expect(container.textContent).not.toContain("Connecting conversation…");
	});

	// Catches: "Start new chat" after a refused load leaves the refused session open and
	// selected, so the new chat reloads it or the dead tab stays.
	it("closes the refused session on the agent and opens a new one in its place", async () => {
		restoreTabs();
		client.loadSession.mockRejectedValueOnce(new Error("uncertain effect"));
		await createRoot(async (dispose) => {
			const chat = createAcpChat(
				() => ROOT,
				() => true,
			);
			await chat.selectSession(SECOND_SESSION);
			await settle();
			expect(chat.error()).toContain("uncertain effect");
			await chat.closeFailed();
			expect(client.closeSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION);
			expect(chat.error()).toBeNull();
			expect(aiChatTabs.ids("global")).not.toContain(SECOND_SESSION);
			await chat.startSession();
			await settle();
			expect(client.newSession).toHaveBeenCalledTimes(1);
			expect(client.loadSession.mock.calls.filter((call) => call[1] === SECOND_SESSION)).toHaveLength(1);
			expect(chat.error()).toBeNull();
			dispose();
		});
	});

	// Catches: Retry clears the error without reattaching the saved tab that failed.
	it("retries the refused saved tab on its existing connection", async () => {
		restoreTabs();
		let refusedSecond = false;
		client.loadSession.mockImplementation(async (_id, session) => {
			if (session === SECOND_SESSION && !refusedSecond) {
				refusedSecond = true;
				throw new Error("Saved conversation unavailable");
			}
			await openSession(session);
			if (session === SECOND_SESSION)
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
		expect(container.textContent).toContain("Opus");
		expect(client.loadSession.mock.calls.filter((call) => call[1] === SECOND_SESSION)).toHaveLength(2);
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
		expect(container.textContent).toContain("Opus");
		expect(client.loadSession).not.toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
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

	// Catches: a recorded pre-prompt offer leaves Session settings hidden.
	it("reveals settings from the recorded async config offer before sending", async () => {
		const { container } = await renderPanel();
		const empty = snapshot({
			attachments: [attachment({ configOptions: [] }), attachment({ sessionId: SECOND_SESSION })],
		});
		acpStore.applySnapshot(empty);
		expect((container.querySelector('[aria-label="Session settings"]') as HTMLButtonElement).disabled).toBe(true);
		const recording = readFileSync(resolve("src-tauri/tests/fixtures/acp/ego-async-config.jsonl"), "utf8")
			.trim()
			.split("\n")
			.map((line) => JSON.parse(line));
		const offer = JSON.parse(recording.at(-1).raw).params.update;
		const actual = await vi.importActual<typeof import("../../services/acpClient")>("../../services/acpClient");
		let receive!: (frame: import("../../types/acp").AcpStreamFrame) => void;
		const realClient = actual.createAcpClient(async (options) => {
			receive = options.onFrame;
			return { close() {} };
		});
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "acp_connect") return empty;
			if (command === "acp_pending_interactions") return [];
			if (command === "acp_connection_snapshot")
				return snapshot({
					attachments: [attachment({ configOptions: offer.configOptions }), attachment({ sessionId: SECOND_SESSION })],
				});
			return undefined;
		});
		await realClient.connect(CHAT_ROOT);
		receive({
			kind: "event",
			connectionId: CONNECTION,
			generation: 1,
			sequence: ++sequence,
			sessionId: SESSION,
			turnId: null,
			event: { kind: "sessionUpdate", update: offer },
		});
		await settle();
		expect(container.querySelector('[aria-label="Session settings"]')).not.toBeNull();
		expect(container.textContent).toContain("default");
		expect(container.textContent).not.toContain("Model: pending");
		expect(acpStore.attachment(CONNECTION, SECOND_SESSION)?.configOptions).toEqual([MODEL_OPTION]);
		expect(client.prompt).not.toHaveBeenCalled();
		await realClient.disconnect(CONNECTION);
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
		expect(container.textContent).toContain(configOptions.length ? "High" : "Model: pending · Mode: pending");
		acpStore.applySnapshot(snapshot({ attachments: [attachment()] }));
		expect(container.textContent).toContain("Opus");
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
		expect(container.textContent).toContain("Opus");
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
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
		const [visible, setVisible] = createSignal(true);
		const { container } = render(() => <AIChatPanel visible={visible()} repoPath={ROOT} onClose={() => {}} />);
		await settle();
		for (const _ of [SECOND_SESSION]) {
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
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
		const view = render(() => <AIChatPanel visible={visible()} repoPath={ROOT} onClose={() => {}} />);
		await settle();
		for (const _ of [SECOND_SESSION]) {
			(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
			await settle();
		}
		// Simulate the backend dropping attachments, rather than making a
		// successful session/new violate its refreshed-snapshot contract.
		acpStore.applySnapshot(snapshot());
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

	// Catches: reference handoff replacing a draft, sending it, or losing focus across mobile remount.
	it("appends a document reference and focuses the composer after it mounts without sending", async () => {
		aiChatDraft.activate(SESSION);
		aiChatDraft.set("Existing question");
		aiChatDraft.insertReference(SESSION, "/repo/map.md:6-6\n\n> Repeated");
		const { container } = await renderPanel();
		await settle();
		const editor = container.querySelector("textarea") as HTMLTextAreaElement;
		expect(editor.value).toBe("Existing question\n\n/repo/map.md:6-6\n\n> Repeated");
		expect(document.activeElement).toBe(editor);
		expect(editor.selectionStart).toBe(editor.value.length);
		expect(client.prompt).not.toHaveBeenCalled();
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
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
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
	it("middle-click closes the chosen tab — catches missing auxclick handling", async () => {
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
		const { container } = await renderPanel();
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		const tab = container.querySelector(`[data-chat-session="${SECOND_SESSION}"]`)!;
		tab.dispatchEvent(new MouseEvent("auxclick", { button: 2, bubbles: true }));
		await settle();
		expect(container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
		tab.dispatchEvent(new MouseEvent("auxclick", { button: 1, bubbles: true, cancelable: true }));
		await settle();
		expect(container.querySelector(`[data-chat-session="${SECOND_SESSION}"]`)).toBeNull();
		expect((container.querySelector('select[aria-label="Conversation"]') as HTMLSelectElement).value).toBe(SESSION);
		expect(container.querySelector('[role="tablist"]')).toBeNull();
	});

	it("Escape stops from composer and transcript without clearing drafts — catches missing Escape stop", async () => {
		const { container } = await renderPanel();
		await typeAndSend(container, "Start work");
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
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Keep this draft";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		for (const target of [textarea, container.querySelector('[aria-label="Chat transcript"]')!]) {
			target.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
			await settle();
		}
		expect(client.cancel).toHaveBeenCalledTimes(2);
		expect(textarea.value).toBe("Keep this draft");
	});

	it("Escape leaves idle drafts intact and closes Find before stopping a busy turn", async () => {
		const { container } = await renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Keep this draft";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
		await settle();
		expect(client.cancel).not.toHaveBeenCalled();
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
		const transcript = container.querySelector('[aria-label="Chat transcript"]')!;
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "f", ctrlKey: true, bubbles: true }));
		await settle();
		container
			.querySelector('[aria-label="Find in chat"]')!
			.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
		await settle();
		expect(container.querySelector('[aria-label="Find in chat"]')).toBeNull();
		expect(client.cancel).not.toHaveBeenCalled();
		expect(textarea.value).toBe("Keep this draft");
	});

	it("Escape in the composer dismisses open Find before cancelling work — catches sibling search bypass", async () => {
		const { container } = await renderPanel();
		await typeAndSend(container, "Keep working");
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
		const transcript = container.querySelector('[aria-label="Chat transcript"]')!;
		transcript.dispatchEvent(new KeyboardEvent("keydown", { key: "f", ctrlKey: true, bubbles: true }));
		await settle();
		expect(container.querySelector('[aria-label="Find in chat"]')).not.toBeNull();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Next question";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
		await settle();
		expect(client.cancel).not.toHaveBeenCalled();
		expect(container.querySelector('[aria-label="Find in chat"]')).toBeNull();
		expect(textarea.value).toBe("Next question");
	});

	it("middle-click does not reopen an uncertain-effect refusal or close it on the agent", async () => {
		restoreTabs();
		client.loadSession.mockImplementation(async (_id, session) => {
			if (session === SECOND_SESSION)
				throw new Error(
					'RPC acp_session_load failed: 502 {"message":"uncertain effect; start a new chat","retryable":false}',
				);
			await openSession(session);
		});
		const { container } = renderIdlePanel();
		await settle();
		selectTab(container, SECOND_SESSION);
		await settle();
		expect(container.textContent).toContain("uncertain effect");
		container
			.querySelector(`[data-chat-session="${SESSION}"]`)!
			.dispatchEvent(new MouseEvent("auxclick", { button: 1, bubbles: true }));
		await settle();
		expect(container.textContent).toContain("uncertain effect");
		expect(client.closeSession).not.toHaveBeenCalled();
	});

	it("pins per-conversation repository context without replacing draft parking", async () => {
		const [repo, setRepo] = createSignal<string | null>(ROOT);
		const { container } = render(() => <AIChatPanel visible={true} repoPath={repo()} onClose={() => {}} />);
		await settle();
		(container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();
		(container.querySelector('button[aria-label="Pin repository"]') as HTMLButtonElement).click();
		expect(container.querySelector('button[aria-label="Unpin repository"]')?.getAttribute("aria-pressed")).toBe("true");
		setRepo("/other/repo");
		await typeAndSend(container, "Pinned context");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "Pinned context", [], ROOT);
		expect(container.querySelector('button[aria-label="Park draft"]')).not.toBeNull();
		(container.querySelector('button[aria-label="Unpin repository"]') as HTMLButtonElement).click();
		await typeAndSend(container, "Follow context");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "Follow context", [], "/other/repo");
	});

	it("routes prompts to the selected session and returns to the neighbor when closing it", async () => {
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
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
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
		const { container } = await renderPanel();
		await settle();
		(container.querySelector("textarea") as HTMLTextAreaElement).dispatchEvent(
			new KeyboardEvent("keydown", { key: "t", metaKey: true, bubbles: true }),
		);
		await settle();
		expect(container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
	});

	it("keeps separate ACP transcripts and composer drafts while switching and closing tabs", async () => {
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
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
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
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
		// Opening restores the selected history before the next message.
		const second = renderIdlePanel();
		await settle();
		expect(second.container.querySelectorAll("[data-chat-session]")).toHaveLength(2);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(second.container.textContent).toContain("Second replay");
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
	// Catches: detection is available but a fresh AI Chat still requires a manual path choice.
	it("offers a detected ego path and configures it only after confirmation", async () => {
		settings.egoExecutable = "";
		const original = vi.mocked(invoke).getMockImplementation()!;
		vi.mocked(invoke).mockImplementation(async (command, args) =>
			command === "detect_all_agent_binaries"
				? { ego: { path: "/detected/bin/ego", version: null } }
				: original(command, args),
		);
		const view = renderIdlePanel();
		await vi.waitFor(() => expect(view.container.textContent).toContain("/detected/bin/ego"));
		expect(settings.egoExecutable).toBe("");
		expect(client.connect).not.toHaveBeenCalled();
		view.getByRole("button", { name: "Use this" }).click();
		expect(settings.egoExecutable).toBe("/detected/bin/ego");
		await settle();
		expect(view.queryByRole("button", { name: "Use this" })).toBeNull();
	});

	// Catches: an existing user choice is overwritten by a later detection result.
	it("does not offer detection over an explicitly configured path", async () => {
		const view = renderIdlePanel();
		await settle();
		expect(view.queryByRole("button", { name: "Use this" })).toBeNull();
		expect(settings.egoExecutable).toBe("/usr/local/bin/ego");
		expect(invoke).not.toHaveBeenCalledWith("detect_all_agent_binaries", expect.anything());
	});

	// An empty `ego_executable` is refused in Rust at connect. Saying so is the
	// difference between a panel that explains itself and one that silently
	// never starts.
	it("rejects setup narration while retaining the configuration recovery action", async () => {
		settings.egoExecutable = "";
		const { container, getByRole } = renderIdlePanel();
		await settle();

		expect(container.textContent).toContain("AI Chat is inactive because the ego executable is not configured");
		expect(container.textContent).not.toContain("Select it in Settings");
		expect(getByRole("button", { name: "Configure ego" })).not.toBeNull();
		expect(client.connect).not.toHaveBeenCalled();
		expect(container.querySelector("textarea")).toBeNull();
		expect(container.textContent).not.toContain("Detected ego:");
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
		expect(client.connect).toHaveBeenCalledExactlyOnceWith(CHAT_ROOT);
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
		// Catches the new card wrapper becoming the 40% height-budget source.
		clientHeight.mockImplementation(function (this: HTMLElement) {
			return this.id === "ai-chat-panel" ? PANEL_PX : 60;
		});
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

		renderIdlePanel();
		await settle();

		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.newSession).toHaveBeenCalledTimes(1);
	});
});

describe("AIChatPanel: durable conversations", () => {
	// Catches: migrating the native picker drops its session selection handler or sends a prompt.
	it("loads the selected conversation through the native picker without sending", async () => {
		client.listSessions.mockResolvedValue({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "First conversation", updatedAt: "2026-09-27T09:00:00Z" },
				{ sessionId: SECOND_SESSION, cwd: CHAT_ROOT, title: "Second conversation", updatedAt: "2026-09-26T09:00:00Z" },
			],
			nextCursor: null,
		});
		client.loadSession.mockImplementation(async (_id, session) => {
			acpStore.applySnapshot(snapshot({ attachments: [attachment({ sessionId: session })] }));
			feed(
				{
					kind: "sessionUpdate",
					update: {
						sessionUpdate: "agent_message_chunk",
						content: { type: "text", text: "Selected conversation history" },
					},
				},
				session,
			);
		});
		const { container } = await renderPanel();
		await settle();
		const picker = container.querySelector<HTMLSelectElement>('select[title="Conversation"]')!;
		picker.value = SECOND_SESSION;
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		expect(client.loadSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		expect(picker.value).toBe(SECOND_SESSION);
		expect(container.textContent).toContain("Selected conversation history");
		expect(client.prompt).not.toHaveBeenCalled();
	});

	// Catches: late catalogue options reset the native picker to another attached session.
	it("keeps the header on the selected session when a two-session catalogue arrives", async () => {
		let finishList!: (value: {
			sessions: { sessionId: string; cwd: string; title: string }[];
			nextCursor: null;
		}) => void;
		client.listSessions.mockImplementation(
			() =>
				new Promise((resolve) => {
					finishList = resolve;
				}),
		);
		client.newSession.mockImplementation(async () => {
			await openSession(SECOND_SESSION);
			return openSession(SESSION);
		});
		const { container } = await renderPanel();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "session_info_update", title: "Selected design review" } });
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Selected transcript" } },
		});
		feed(
			{ kind: "sessionUpdate", update: { sessionUpdate: "session_info_update", title: "Background investigation" } },
			SECOND_SESSION,
		);
		finishList({
			sessions: [
				{ sessionId: SECOND_SESSION, cwd: CHAT_ROOT, title: "Background investigation" },
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Selected design review" },
			],
			nextCursor: null,
		});
		await settle();

		const picker = container.querySelector<HTMLSelectElement>('select[title="Conversation"]')!;
		expect(acpStore.connection(CONNECTION)?.attachments).toHaveLength(2);
		expect(picker.value).toBe(SESSION);
		expect(picker.selectedOptions[0].textContent).toBe("Selected design review");
		expect(container.querySelector('[class*="conversationLabel"]')?.textContent).toBe("Selected design review");
		expect(container.textContent).toContain("Selected transcript");
		await typeAndSend(container, "Continue the selected review");
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Continue the selected review", [], ROOT);

		picker.value = SECOND_SESSION;
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		finishList({
			sessions: [
				{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Selected design review" },
				{ sessionId: SECOND_SESSION, cwd: CHAT_ROOT, title: "Background investigation" },
			],
			nextCursor: null,
		});
		await settle();
		expect(picker.value).toBe(SECOND_SESSION);
		expect(picker.selectedOptions[0].textContent).toBe("Background investigation");
		expect(container.querySelector('[class*="conversationLabel"]')?.textContent).toBe("Background investigation");
		expect(container.textContent).not.toContain("Selected transcript");
		expect(client.loadSession).not.toHaveBeenCalled();

		feed(
			{ kind: "sessionUpdate", update: { sessionUpdate: "session_info_update", title: "Renamed background review" } },
			SECOND_SESSION,
		);
		await settle();
		expect(picker.value).toBe(SECOND_SESSION);
		expect(picker.selectedOptions[0].textContent).toBe("Renamed background review");
		await typeAndSend(container, "Continue the other review");
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SECOND_SESSION, "Continue the other review", [], ROOT);
	});

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
		expect(container.querySelector('[class*="tidyHeader"]')?.textContent).toContain("Review architecture");
	});

	it("renders context occupancy and cost from one ACP usage update", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "usage_update", used: 50000, size: 200000, cost: { amount: 0.001035, currency: "USD" } },
		});
		await settle();

		// Catches: the card footer showing a stale or incorrect context numerator, capacity or remainder.
		expect(container.querySelector('[aria-label="Context usage"]')?.textContent).toContain(
			"Ctx 25% · 50K/200K · ~150K free",
		);
		expect(container.textContent).toContain("USD 0.001035");
	});

	it("renders context occupancy without a missing cost", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "usage_update", used: 100, size: 400 } });
		await settle();

		expect(container.textContent).toContain("Ctx 25%");
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

		expect(container.textContent).toContain("Ctx 50%");
		expect(container.textContent).not.toContain("USD 2");
	});

	it("does not render a percentage for a zero context window", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "usage_update", used: 100, size: 0 } });
		await settle();

		expect(container.querySelector('[aria-label="Context usage"]')).toBeNull();
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
		const next = container.querySelector<HTMLButtonElement>('button[aria-label="New chat tab"]');
		next?.click();
		await settle();
		expect(client.newSession).toHaveBeenCalledTimes(2);
		expect(client.listSessions).not.toHaveBeenCalled();
	});

	it("saves the selected session and restores it from the next document's config", async () => {
		const saved: Record<string, string> = {};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "ai_chat_selection") return { workspace: CHAT_ROOT, sessionId: saved[CHAT_ROOT] ?? null };
			if (command === "load_config") return { ai_chat_sessions: { ...saved } };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			if (command === "select_ai_chat_conversation") saved[CHAT_ROOT] = (args as { sessionId: string }).sessionId;
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
			if (command === "ai_chat_selection") return { workspace: CHAT_ROOT, sessionId: "prior-session" };
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
		expect(client.connect).toHaveBeenCalledExactlyOnceWith(CHAT_ROOT);
		expect(client.loadSession).toHaveBeenCalledWith(CONNECTION, "prior-session", CHAT_ROOT);
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

		const summaries = [...container.querySelectorAll("summary[class*=toolActivitySummary]")].filter((summary) =>
			summary.textContent?.includes("7 reads"),
		);
		expect(summaries).toHaveLength(1);
		expect(summaries[0].textContent).toContain("Inspect file 1");
		expect(summaries[0].textContent).toContain("Inspect file 2");
		expect(summaries[0].textContent).not.toMatch(/s observed/);
		expect((summaries[0].parentElement as HTMLDetailsElement).open).toBe(false);
	});

	it("requires a second expansion before showing tool output", async () => {
		const { container } = await renderPanel();
		await settle();
		tool(1);
		await settle();

		const activity = [...container.querySelectorAll("details")].find((details) =>
			details.querySelector("summary")?.textContent?.includes("read"),
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

		const summary = [...container.querySelectorAll("summary[class*=toolActivitySummary]")].find((element) =>
			element.textContent?.includes("2 reads"),
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
			details.querySelector("summary")?.textContent?.includes("read"),
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

		const summaries = [...container.querySelectorAll("summary[class*=toolActivitySummary]")].filter((summary) =>
			summary.textContent?.includes("read"),
		);
		expect(summaries).toHaveLength(2);
		expect(summaries[0].textContent).toContain("2 reads");
		expect(summaries[1].textContent).toContain("1 read");
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

		const summary = [...container.querySelectorAll("summary[class*=toolActivitySummary]")].find((element) =>
			element.textContent?.includes("1 read"),
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

	it("keeps history untimed when a later call joins completed activity", async () => {
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

			const summary = [...container.querySelectorAll("summary[class*=toolActivitySummary]")].find((element) =>
				element.textContent?.includes("2 reads"),
			);
			expect(summary?.textContent).not.toMatch(/s observed/);
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
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
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

	// Catches: the composer reports settings as inert text instead of opening their controls.
	it("opens model reasoning and mode controls from the composer summary and applies each choice", async () => {
		const effort: AcpSessionConfigOption = {
			id: "effort",
			name: "Reasoning",
			type: "select",
			currentValue: "medium",
			options: [
				{ value: "medium", name: "Medium" },
				{ value: "high", name: "High" },
			],
		};
		const options = [MODEL_OPTION, effort, mode];
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ configOptions: options })] }));
		await settle();
		const summary = container.querySelector<HTMLButtonElement>(
			'.composerMeta button[aria-label="Change model, reasoning and mode"]',
		);
		expect(summary).not.toBeNull();
		summary!.click();
		const dialog = container.querySelector('[role="dialog"]')!;
		expect(dialog).not.toBeNull();
		for (const [name, id, value] of [
			["Model", "model", "sonnet"],
			["Reasoning", "effort", "high"],
			["Mode", "mode", "auto"],
		]) {
			const picker = dialog.querySelector<HTMLSelectElement>(`select[aria-label="${name}"]`)!;
			expect(picker.disabled).toBe(false);
			picker.value = value;
			picker.dispatchEvent(new Event("change", { bubbles: true }));
			await settle();
			expect(client.setConfigOption).toHaveBeenCalledWith(CONNECTION, SESSION, id, { value });
		}
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						configOptions: [
							{ ...MODEL_OPTION, currentValue: "sonnet" },
							{ ...effort, currentValue: "high" },
							{ ...mode, currentValue: "auto" },
						],
					}),
				],
			}),
		);
		await settle();
		expect(summary!.textContent).toBe("Sonnet · High · Automatic");
	});

	// Catches: changing models exposes ego's internal unselected effort value in composer metadata.
	it("uses the published reasoning label after the recorded model change, never the internal sentinel", async () => {
		const { container } = await renderPanel();
		await settle();
		feed({
			kind: "sessionUpdate",
			update: {
				sessionUpdate: "agent_message_chunk",
				content: { type: "text", text: "Answer before changing the model" },
			},
		});
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						configOptions: recordedModelChange.before as AcpSessionConfigOption[],
					}),
				],
			}),
		);
		await settle();
		const summary = container.querySelector<HTMLButtonElement>(
			'.composerMeta button[aria-label="Change model, reasoning and mode"]',
		)!;
		expect(summary.textContent).toBe("gpt-6.1-sol · low · default");
		summary.click();
		const picker = container.querySelector<HTMLSelectElement>('select[aria-label="Model"]')!;
		picker.value = "ollama/gemma4:12b-mlx";
		picker.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		expect(client.setConfigOption).toHaveBeenCalledWith(CONNECTION, SESSION, "model", {
			value: "ollama/gemma4:12b-mlx",
		});
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						configOptions: recordedModelChange.after as AcpSessionConfigOption[],
					}),
				],
			}),
		);
		await settle();
		expect(summary.textContent).not.toContain("__ego_unselected__");
		expect(summary.textContent).toBe("gemma4:12b-mlx · Choose a value · default");
		expect(container.textContent).toContain("Answer before changing the model");
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.newSession).toHaveBeenCalledTimes(1);
		expect(client.reconnect).not.toHaveBeenCalled();
		expect(client.loadSession).not.toHaveBeenCalled();
		await typeAndSend(container, "Continue with the selected model");
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Continue with the selected model", [], ROOT);
		expect(
			acpStore.attachment(CONNECTION, SESSION)?.configOptions.find((option) => option.id === "model")?.currentValue,
		).toBe("ollama/gemma4:12b-mlx");
	});

	// Catches: missing ACP options silently remove the only discoverable settings action.
	it("keeps session settings visible with an unavailable explanation before options arrive", async () => {
		const { container } = await renderPanel();
		await settle();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ configOptions: [] })] }));
		await settle();
		const menu = container.querySelector<HTMLDetailsElement>(".actionsMenu")!;
		menu.querySelector("summary")!.click();
		expect(menu.open).toBe(true);
		const settings = menu.querySelector<HTMLButtonElement>('button[aria-label="Session settings"]');
		expect(settings).not.toBeNull();
		expect(settings!.disabled).toBe(true);
		expect(settings!.title).toBe("Session settings are available after the conversation publishes its options.");
	});

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
		expect(container.querySelector(".composerMeta")?.textContent).toContain("Automatic");
		(container.querySelector('button[aria-label="Session settings"]') as HTMLButtonElement).click();
		const picker = container.querySelector('select[aria-label="Mode"]') as HTMLSelectElement;
		expect(picker.querySelector("optgroup")?.label).toBe("Behavior");
		expect(picker.selectedOptions[0].value).toBe("auto");
		expect(picker.selectedOptions[0].textContent).toBe("Automatic");
	});

	// Catches: a new tab keeps a blank select after its options arrive.
	it("shows the current choice when a second tab receives its options after opening", async () => {
		client.newSession
			.mockImplementationOnce(() => openSession(SESSION))
			.mockImplementationOnce(() => openSession(SECOND_SESSION));
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
		expect(container.querySelector(".composerMeta")?.textContent).toContain("Opus");
		acpStore.applySnapshot(
			snapshot({ attachments: [attachment({ configOptions: [{ ...MODEL_OPTION, currentValue: "sonnet" }] })] }),
		);
		await settle();
		expect(container.querySelector(".composerMeta")?.textContent).toContain("Sonnet");
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
	it("keeps former icon actions labelled inside the overflow menu — catches lost or unnamed menu actions", async () => {
		supportsDelete = true;
		client.listSessions.mockResolvedValue({
			sessions: [{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Current chat" }],
			nextCursor: null,
		});
		const { container } = await renderPanel();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "prompting" })] }));
		await settle();
		const menu = container.querySelector<HTMLDetailsElement>(".actionsMenu")!;
		expect(menu.open).toBe(false);
		expect(menu.querySelector(":scope > summary svg")).not.toBeNull();
		expect(container.querySelector('button[aria-label="New chat tab"] svg')).not.toBeNull();
		expect(container.querySelector(".conversationLabel span + svg")).not.toBeNull();
		menu.querySelector<HTMLElement>(":scope > summary")!.click();
		expect(menu.open).toBe(true);
		expect(menu.querySelectorAll(".menuSeparator")).toHaveLength(3);
		expect(menu.querySelector(".actionsMenuItems > button")?.textContent).toContain("New conversation");
		expect(menu.querySelector(".actionsMenuItems > .conversationList > summary")?.textContent).toBe(
			"Delete conversation…",
		);
		for (const label of ["Session settings", "Pause the turn", "Compact the conversation"]) {
			const button = menu.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
			expect(button.textContent).toContain(label === "Compact the conversation" ? "Compact context" : label);
		}
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "paused" })] }));
		await settle();
		expect(menu.querySelector('button[aria-label="Resume the turn"]')?.textContent).toContain("Resume the turn");
		menu.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
		expect(menu.open).toBe(false);
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

	it.each(["stop", "v1 failure"])(
		"overflow stop compacts and retries (%s) — catches overflow left to the user and infinite retry loops",
		async (shape) => {
			client.compact.mockResolvedValueOnce({
				sourceSessionId: SESSION,
				targetSessionId: SECOND_SESSION,
				publication: { kind: "published_durably", diagnostic: null },
			});
			client.loadSession.mockImplementationOnce(() => openSession(SECOND_SESSION));
			client.prompt.mockResolvedValueOnce("overflow-turn").mockResolvedValueOnce("retry-turn");
			const { container } = await renderPanel();
			await typeAndSend(container, "Continue my work");
			feed({ kind: "turnStarted" }, SESSION, "overflow-turn");
			if (shape === "stop") {
				feed({ kind: "turnSettled", stopReason: "_context_overflow", usage: null }, SESSION, "overflow-turn");
			} else {
				feed(
					{
						kind: "turnFailed",
						message: "the provider context window is exhausted; invoke _ego/compact",
						state: "idle",
					},
					SESSION,
					"overflow-turn",
				);
			}
			await settle();
			expect(client.compact).toHaveBeenCalledExactlyOnceWith(CONNECTION, SESSION);
			expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SECOND_SESSION, "Continue my work", [], ROOT);
			expect(client.prompt).toHaveBeenCalledTimes(2);
			expect(toastsStore.toasts.map((toast) => toast.title)).toContain("Conversation compacted");
			feed({ kind: "turnStarted" }, SECOND_SESSION, "retry-turn");
			feed({ kind: "turnSettled", stopReason: "_context_overflow", usage: null }, SECOND_SESSION, "retry-turn");
			await settle();
			expect(client.compact).toHaveBeenCalledTimes(1);
			expect(client.prompt).toHaveBeenCalledTimes(2);
			expect(container.textContent).toContain("still exhausted after compaction");
		},
	);

	it("keeps original attachments and repository on overflow replay — catches changed context on retry", async () => {
		let dispose = () => {};
		let repo = ROOT;
		const chat = createRoot((cleanup) => {
			dispose = cleanup;
			return createAcpChat(
				() => repo,
				() => true,
				client,
			);
		});
		const images = [{ type: "image" as const, data: "png", mimeType: "image/png" }];
		const files = [{ name: "notes.txt", path: "/repo/notes.txt" }];
		client.prompt.mockResolvedValueOnce("overflow-turn").mockResolvedValueOnce("retry-turn");
		client.compact.mockResolvedValueOnce({
			sourceSessionId: SESSION,
			targetSessionId: SECOND_SESSION,
			publication: { kind: "published_durably", diagnostic: null },
		});
		client.loadSession.mockImplementationOnce(() => openSession(SECOND_SESSION));
		try {
			await chat.send("Read these", images, files);
			repo = "/different/repository";
			feed({ kind: "turnStarted" }, SESSION, "overflow-turn");
			feed({ kind: "turnSettled", stopReason: "_context_overflow", usage: null }, SESSION, "overflow-turn");
			await settle();
			expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SECOND_SESSION, "Read these", images, ROOT, files);
		} finally {
			dispose();
		}
	});

	it.each(["refused", "unpublished", "load refused", "unsupported"])(
		"stops overflow recovery when %s — catches replay without a usable continuation",
		async (failure) => {
			const { container } = await renderPanel();
			client.prompt.mockResolvedValueOnce("overflow-turn");
			if (failure === "refused") client.compact.mockRejectedValueOnce(new Error("compact refused"));
			else if (failure !== "unsupported")
				client.compact.mockResolvedValueOnce({
					sourceSessionId: SESSION,
					targetSessionId: SECOND_SESSION,
					publication:
						failure === "unpublished"
							? { kind: "not_published", diagnostic: "checkpoint refused" }
							: { kind: "published_durably", diagnostic: null },
				});
			if (failure === "load refused") client.loadSession.mockRejectedValueOnce(new Error("successor refused"));
			if (failure === "unsupported")
				acpStore.applySnapshot(
					snapshot({
						capabilities: { ...snapshot().capabilities!, egoCompactVersion: null },
						attachments: [attachment()],
					}),
				);
			await typeAndSend(container, "Continue my work");
			feed({ kind: "turnStarted" }, SESSION, "overflow-turn");
			feed({ kind: "turnSettled", stopReason: "_context_overflow", usage: null }, SESSION, "overflow-turn");
			await settle();
			expect(client.prompt).toHaveBeenCalledTimes(1);
			expect(client.compact).toHaveBeenCalledTimes(failure === "unsupported" ? 0 : 1);
			expect(toastsStore.toasts.map((toast) => toast.title)).not.toContain("Conversation compacted");
			expect(container.textContent).toContain(
				failure === "refused"
					? "compact refused"
					: failure === "unpublished"
						? "checkpoint refused"
						: failure === "load refused"
							? "successor refused"
							: "cannot compact",
			);
		},
	);

	it("ignores overflow from another turn — catches replaying the wrong user's prompt", async () => {
		const { container } = await renderPanel();
		await typeAndSend(container, "My prompt");
		feed({ kind: "turnStarted" }, SESSION, "other-turn");
		feed({ kind: "turnSettled", stopReason: "_context_overflow", usage: null }, SESSION, "other-turn");
		await settle();
		expect(client.compact).not.toHaveBeenCalled();
		expect(client.prompt).toHaveBeenCalledTimes(1);
	});

	it("recovers overflow arriving before prompt acknowledgement — catches a lost early completion", async () => {
		client.prompt
			.mockImplementationOnce(async () => {
				feed({ kind: "turnStarted" }, SESSION, "early-turn");
				feed(
					{
						kind: "turnFailed",
						message: "the provider context window is exhausted; invoke _ego/compact",
						state: "idle",
					},
					SESSION,
					"early-turn",
				);
				return "early-turn";
			})
			.mockResolvedValueOnce("retry-turn");
		client.compact.mockResolvedValueOnce({
			sourceSessionId: SESSION,
			targetSessionId: SECOND_SESSION,
			publication: { kind: "published_durably", diagnostic: null },
		});
		client.loadSession.mockImplementationOnce(() => openSession(SECOND_SESSION));
		const { container } = await renderPanel();
		await typeAndSend(container, "Continue");
		await settle();
		expect(client.compact).toHaveBeenCalledExactlyOnceWith(CONNECTION, SESSION);
		expect(client.prompt).toHaveBeenCalledTimes(2);
	});

	it("keeps a newer selection while compact waits — catches switching and replaying into another tab", async () => {
		let finish!: (value: unknown) => void;
		client.compact.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finish = resolve;
				}),
		);
		let dispose = () => {};
		const chat = createRoot((cleanup) => {
			dispose = cleanup;
			return createAcpChat(
				() => ROOT,
				() => true,
				client,
			);
		});
		try {
			await chat.send("Continue");
			feed({ kind: "turnStarted" }, SESSION, "turn-1");
			feed({ kind: "turnSettled", stopReason: "_context_overflow", usage: null }, SESSION, "turn-1");
			await settle();
			await openSession(SECOND_SESSION);
			await chat.selectSession(SECOND_SESSION);
			finish({
				sourceSessionId: SESSION,
				targetSessionId: "compacted-session",
				publication: { kind: "published_durably", diagnostic: null },
			});
			await settle();
			expect(chat.sessionId()).toBe(SECOND_SESSION);
			expect(client.prompt).toHaveBeenCalledTimes(1);
		} finally {
			dispose();
		}
	});

	it("keeps a message submitted during compaction — catches a silently dropped composer send", async () => {
		let finish!: (value: unknown) => void;
		client.compact.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finish = resolve;
				}),
		);
		client.loadSession.mockImplementationOnce(() => openSession(SECOND_SESSION));
		let dispose = () => {};
		const chat = createRoot((cleanup) => {
			dispose = cleanup;
			return createAcpChat(
				() => ROOT,
				() => true,
				client,
			);
		});
		try {
			await chat.send("First");
			const compacting = chat.compact();
			const sending = chat.send("Next message");
			expect(client.prompt).toHaveBeenCalledTimes(1);
			finish({
				sourceSessionId: SESSION,
				targetSessionId: SECOND_SESSION,
				publication: { kind: "published_durably", diagnostic: null },
			});
			await compacting;
			await sending;
			expect(client.prompt).toHaveBeenCalledTimes(2);
			expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SECOND_SESSION, "Next message", [], ROOT);
		} finally {
			dispose();
		}
	});

	it("sends in a newer selection during compaction — catches concurrent send discarded during stale compaction", async () => {
		let finish!: (value: unknown) => void;
		client.compact.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finish = resolve;
				}),
		);
		const { container } = await renderPanel();
		container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]')!.click();
		await settle();
		await openSession(SECOND_SESSION);
		aiChatTabs.add("global", SECOND_SESSION);
		container.querySelector<HTMLButtonElement>(`button[data-chat-session="${SECOND_SESSION}"]`)!.click();
		await settle();
		const textarea = container.querySelector("textarea")!;
		textarea.value = "Message for B";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
		await settle();
		finish({
			sourceSessionId: SESSION,
			targetSessionId: "compacted-session",
			publication: { kind: "published_durably", diagnostic: null },
		});
		await settle();
		expect(client.prompt).toHaveBeenCalledExactlyOnceWith(CONNECTION, SECOND_SESSION, "Message for B", [], ROOT);
	});

	it.each(["overflow", "manual", "leave"])(
		"waits for another conversation compaction (%s) — catches swallowed, duplicated or stale B actions",
		async (action) => {
			let finish!: (value: unknown) => void;
			client.compact
				.mockReset()
				.mockImplementationOnce(
					() =>
						new Promise((resolve) => {
							finish = resolve;
						}),
				)
				.mockResolvedValue({
					sourceSessionId: SECOND_SESSION,
					targetSessionId: CHILD_SESSION,
					publication: { kind: "published_durably", diagnostic: null },
				});
			const { container } = await renderPanel();
			container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]')!.click();
			await settle();
			await openSession(SECOND_SESSION);
			aiChatTabs.add("global", SECOND_SESSION);
			container.querySelector<HTMLButtonElement>(`button[data-chat-session="${SECOND_SESSION}"]`)!.click();
			await settle();
			if (action !== "leave") client.loadSession.mockImplementationOnce(() => openSession(CHILD_SESSION));
			if (action === "overflow") {
				await typeAndSend(container, "Continue B");
				feed({ kind: "turnStarted" }, SECOND_SESSION, "turn-1");
				feed({ kind: "turnSettled", stopReason: "_context_overflow", usage: null }, SECOND_SESSION, "turn-1");
			} else {
				const button = container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]')!;
				button.click();
				button.click();
			}
			await settle();
			expect(client.compact).toHaveBeenCalledExactlyOnceWith(CONNECTION, SESSION);
			if (action === "leave") {
				container.querySelector<HTMLButtonElement>(`button[data-chat-session="${SESSION}"]`)!.click();
				await settle();
			}
			finish({
				sourceSessionId: SESSION,
				targetSessionId: "compacted-A",
				publication: { kind: "published_durably", diagnostic: null },
			});
			await settle();
			if (action === "leave") {
				expect(client.compact).toHaveBeenCalledTimes(1);
				expect(client.prompt).not.toHaveBeenCalled();
				expect(aiChatTabs.active("global")).toBe(SESSION);
				return;
			}
			expect(client.compact).toHaveBeenCalledTimes(2);
			expect(client.compact).toHaveBeenLastCalledWith(CONNECTION, SECOND_SESSION);
			expect(aiChatTabs.active("global")).toBe(CHILD_SESSION);
			if (action === "overflow") {
				expect(client.prompt).toHaveBeenCalledTimes(2);
				expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, CHILD_SESSION, "Continue B", [], ROOT);
			} else expect(client.prompt).not.toHaveBeenCalled();
		},
	);

	it.each(["refused", "unpublished"])(
		"retains a composer submission during %s compaction — catches concurrent send discarded during compaction",
		async (failure) => {
			let finish!: (value: unknown) => void;
			let refuse!: (error: Error) => void;
			client.compact.mockImplementationOnce(
				() =>
					new Promise((resolve, reject) => {
						finish = resolve;
						refuse = reject;
					}),
			);
			const { container } = await renderPanel();
			container.querySelector<HTMLButtonElement>('button[aria-label="Compact the conversation"]')!.click();
			await settle();
			const files = [{ name: "notes.txt", path: "/repo/notes.txt" }];
			aiChatDraft.stageFile(files[0]);
			const textarea = container.querySelector("textarea")!;
			textarea.value = "Keep this message";
			textarea.dispatchEvent(new Event("input", { bubbles: true }));
			container.querySelector<HTMLButtonElement>('button[aria-label="Queue"]')!.click();
			if (failure === "refused") refuse(new Error("compact refused"));
			else
				finish({
					sourceSessionId: SESSION,
					targetSessionId: SECOND_SESSION,
					publication: { kind: "not_published", diagnostic: "checkpoint refused" },
				});
			await settle();
			expect(client.prompt).not.toHaveBeenCalled();
			expect(textarea.value).toBe("Keep this message");
			expect(aiChatDraft.files()).toEqual(files);
		},
	);

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
		client.loadSession.mockImplementationOnce(() => openSession("sess-compacted"));
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
		style.textContent = [
			"src/components/ui/FormControls.module.css",
			"src/components/AIChatPanel/AIChatPanel.module.css",
		]
			.map((path) => readFileSync(resolve(process.cwd(), path), "utf8"))
			.join("\n");
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

	// Catches: the prompt's pre-wrap leaking into Copy and wrapping its letters vertically.
	it("keeps the user Copy action on one line", async () => {
		const style = document.createElement("style");
		style.textContent = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		document.head.append(style);
		try {
			const { container } = await retryingPanel();
			feed({ kind: "promptSent", text: "A real user prompt" });
			const copy = container.querySelector<HTMLElement>('[aria-label="Copy user message"]');
			expect(copy).not.toBeNull();
			expect(getComputedStyle(copy!).whiteSpace).toBe("nowrap");
		} finally {
			style.remove();
		}
	});

	// Catches: the absolutely positioned user Copy collapsing to 7.9px and wrapping at a 500px panel.
	// jsdom has no layout engine. Chromium measured the one-line label at 27.109375x16.5px;
	// verify the production CSS preserves that intrinsic line and budgets enough transcript gutter.
	it("keeps Copy within the transcript gutter at a 500px AI Chat panel", async () => {
		const style = document.createElement("style");
		style.textContent = readFileSync(
			resolve(process.cwd(), "src/components/AIChatPanel/AIChatPanel.module.css"),
			"utf8",
		);
		document.head.append(style);
		try {
			const { container } = await retryingPanel();
			const panel = container.querySelector<HTMLElement>("#ai-chat-panel")!;
			panel.style.width = "500px";
			feed({ kind: "promptSent", text: "Question for geometry verification" });
			feed({
				kind: "sessionUpdate",
				update: {
					sessionUpdate: "agent_message_chunk",
					content: { type: "text", text: "Assistant answer for geometry verification." },
				},
			});
			await settle();
			const transcript = container.querySelector<HTMLElement>('[aria-label="Chat transcript"]')!;
			const userCopy = container.querySelector<HTMLElement>('[aria-label="Copy user message"]')!;
			const assistantCopy = container.querySelector<HTMLElement>('[aria-label="Copy assistant message"]')!;
			expect(userCopy.textContent).toBe("Copy");
			expect(assistantCopy.textContent).toBe("Copy");
			const copy = getComputedStyle(userCopy);
			const bubble = getComputedStyle(userCopy.parentElement!);
			const list = getComputedStyle(transcript);
			expect(copy.whiteSpace).toBe("nowrap");
			expect(copy.position).toBe("absolute");
			expect(copy.right).toBe("100%");
			const rowWidth = 500 - parseFloat(list.paddingLeft) - parseFloat(list.paddingRight);
			const gutter = rowWidth * (1 - parseFloat(bubble.maxWidth) / 100);
			expect(gutter).toBeGreaterThanOrEqual(27.109375 + parseFloat(copy.marginRight));
		} finally {
			style.remove();
		}
	});

	// Catches: retaining the provider's initial text instead of counting down from update arrival.
	it("counts a recorded 12s provider retry down to 4s after 8s", async () => {
		const { container } = await retryingPanel();
		vi.useFakeTimers();
		try {
			feed({ kind: "sessionUpdate", update: recordedProviderRetry as never });
			expect(statusLines(container)[0].textContent).toContain("retrying in 12s");
			await vi.advanceTimersByTimeAsync(8000);
			expect(statusLines(container)[0].textContent).toContain("retrying in 4s");
			await vi.advanceTimersByTimeAsync(4000);
			expect(statusLines(container)[0].textContent).toContain("retrying now (attempt 1/6)");
			await vi.advanceTimersByTimeAsync(5000);
			expect(statusLines(container)[0].textContent).toContain("retrying now");
		} finally {
			cleanup();
			vi.useRealTimers();
		}
	});

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
			snapshot({
				attachments: [attachment()],
				capabilities: { ...snapshot().capabilities!, fork: true, forkAtMessage: true },
			}),
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
		client.newSession.mockResolvedValueOnce("grandchild");
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
	// Catches: New chat restores a saved greeting first, then leaves that old title selected when replay fails.
	it("opens an untitled new chat without replaying a saved greeting again", async () => {
		aiChatTabs.add("global", SECOND_SESSION);
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "session_info_update", title: "ciao" } }, SECOND_SESSION);
		client.loadSession.mockRejectedValue(new Error("Saved greeting is unavailable"));
		const view = renderIdlePanel();
		await settle();
		// Opening now restores the saved tab; New must not repeat its refused load.
		expect(client.loadSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, SECOND_SESSION, CHAT_ROOT);
		(view.container.querySelector('button[aria-label="New chat tab"]') as HTMLButtonElement).click();
		await settle();

		const conversations = view.container.querySelector<HTMLSelectElement>('select[aria-label="Conversation"]')!;
		expect(conversations.value).toBe(SESSION);
		expect(view.container.querySelector(`[data-chat-session="${SESSION}"]`)?.textContent).toBe("Untitled conversation");
		expect(client.loadSession).toHaveBeenCalledTimes(1);
		expect(acpTranscript.title(SECOND_SESSION)).toBe("ciao");
		await typeAndSend(view.container, "Start a new task");
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Start a new task", [], ROOT);
	});

	// Catches New chat waiting for an older saved replay and aborting when that replay fails.
	it("opens a usable new chat when the saved connection was still pending at the click", async () => {
		aiChatTabs.add("global", SESSION);
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
	it("keeps repository pin and parked-draft actions separate inside the card header", async () => {
		// Catches: moving the draft action to the header accidentally wires it to repository pinning.
		const view = await renderPanel();
		const header = view.container.querySelector('[class*="composerHeader"]')!;
		expect(header.textContent).toContain("Repository · ");
		const pin = header.querySelector('button[aria-label="Pin repository"]') as HTMLButtonElement;
		const park = header.querySelector('button[aria-label="Park draft"]') as HTMLButtonElement;
		const textarea = view.container.querySelector("textarea") as HTMLTextAreaElement;
		textarea.value = "Keep my draft";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		pin.click();
		expect(textarea.value).toBe("Keep my draft");
		expect(pin.getAttribute("aria-pressed")).toBe("true");
		park.click();
		expect(textarea.value).toBe("");
		expect(pin.getAttribute("aria-pressed")).toBe("true");
		expect(park.getAttribute("aria-label")).toBe("Restore parked draft");
	});

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
	it("uses the auto-connected image capabilities when a PNG is pasted", async () => {
		// Catches: absent pre-connection capabilities are mistaken for explicit image refusal.
		supportsImages = true;
		const { container } = renderIdlePanel();
		await settle();
		expect(client.connect).toHaveBeenCalledExactlyOnceWith(CHAT_ROOT);
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

	it("stages a PNG with an empty text flavor — catches empty text masking an image", async () => {
		supportsImages = true;
		const { container } = await renderPanel();
		const event = mixedPaste(container.querySelector("textarea") as HTMLTextAreaElement, "");
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

// Catches: mobile Chat retains a separate Park button or loses the shared +/mic order.
it("mobile plus Chat shares the control order and restores its parked text", async () => {
	vi.stubGlobal("SpeechRecognition", class {});
	let chat!: ReturnType<typeof createAcpChat>;
	const view = render(() => {
		chat = createAcpChat(
			() => ROOT,
			() => true,
			client,
		);
		return <Composer chat={chat} mobileAttachments />;
	});
	await chat.ensureStarted();
	await settle();
	const plus = view.getByRole("button", { name: "More composer actions" });
	const textarea = view.container.querySelector("textarea")!;
	const mic = view.getByRole("button", { name: "Dictate into draft" });
	expect(plus.compareDocumentPosition(textarea) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
	expect(textarea.parentElement?.contains(mic)).toBe(true);
	expect(
		textarea.compareDocumentPosition(view.getByRole("button", { name: "Send" })) & Node.DOCUMENT_POSITION_FOLLOWING,
	).toBeTruthy();
	expect(view.queryByRole("button", { name: "Park draft" })).toBeNull();
	textarea.value = "Keep this mobile draft";
	textarea.dispatchEvent(new Event("input", { bubbles: true }));
	plus.click();
	expect(view.queryByRole("menuitem", { name: "Commands (/)" })).toBeNull();
	expect(view.queryByRole("menuitem", { name: "Queue" })).toBeNull();
	view.getByRole("menuitem", { name: "Park draft" }).click();
	expect(textarea.value).toBe("");
	plus.click();
	view.getByRole("menuitem", { name: "Restore parked draft" }).click();
	expect(textarea.value).toBe("Keep this mobile draft");
	expect(view.queryByRole("menu")).toBeNull();
	vi.unstubAllGlobals();
});

// Catches: the moved Chat picker stages no image, or Queue sends through a different path.
it("mobile plus Chat stages a captured image and queues it through ACP while busy", async () => {
	supportsImages = true;
	let chat!: ReturnType<typeof createAcpChat>;
	const view = render(() => {
		chat = createAcpChat(
			() => ROOT,
			() => true,
			client,
		);
		return <Composer chat={chat} mobileAttachments />;
	});
	await chat.ensureStarted();
	await settle();
	acpStore.applySnapshot(
		snapshot({
			attachments: [
				attachment({
					state: "prompting",
					activeTurn: { turnId: "running", state: "running", stopReason: null, usage: null },
				}),
			],
		}),
	);
	const plus = view.getByRole("button", { name: "More composer actions" });
	plus.click();
	const camera = view.getByLabelText("Take photo") as HTMLInputElement;
	const clickCamera = vi.spyOn(camera, "click");
	view.getByRole("menuitem", { name: "Take photo" }).click();
	expect(clickCamera).toHaveBeenCalledOnce();
	const file = new File([Uint8Array.from(atob(PNG_1X1), (byte) => byte.charCodeAt(0))], "photo.png", {
		type: "image/png",
	});
	Object.defineProperty(camera, "files", { value: [file], configurable: true });
	camera.dispatchEvent(new Event("change", { bubbles: true }));
	await vi.waitFor(() => expect(view.getByAltText("Pasted image")).toBeTruthy());
	plus.click();
	view.getByRole("menuitem", { name: "Queue" }).click();
	await settle();
	expect(client.prompt).toHaveBeenCalledWith(
		CONNECTION,
		SESSION,
		"",
		[{ type: "image", mimeType: "image/png", data: PNG_1X1 }],
		ROOT,
	);
	expect(view.queryByAltText("Pasted image")).toBeNull();
});

// Catches: Chat's new mic writes nowhere or appends stale recognition after manual typing.
it("mobile plus Chat dictates into the visible draft and ignores speech after an edit", async () => {
	class Recognition {
		static current: Recognition;
		onresult: ((event: { results: { 0: { transcript: string }; isFinal: boolean }[] }) => void) | null = null;
		onend: (() => void) | null = null;
		constructor() {
			Recognition.current = this;
		}
		start() {}
		stop() {}
		abort() {
			this.onend?.();
		}
		result(transcript: string) {
			this.onresult?.({ results: [{ 0: { transcript }, isFinal: true }] });
		}
	}
	vi.stubGlobal("SpeechRecognition", Recognition);
	try {
		let chat!: ReturnType<typeof createAcpChat>;
		const view = render(() => {
			chat = createAcpChat(
				() => ROOT,
				() => true,
				client,
			);
			return <Composer chat={chat} mobileAttachments />;
		});
		await chat.ensureStarted();
		await settle();
		view.getByRole("button", { name: "Dictate into draft" }).click();
		Recognition.current.result("Review this change");
		const textarea = view.container.querySelector("textarea")!;
		expect(textarea.value).toBe("Review this change");
		textarea.value = "Review this change carefully";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		Recognition.current.result("stale words");
		expect(textarea.value).toBe("Review this change carefully");
		view.getByRole("button", { name: "Send" }).click();
		await settle();
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "Review this change carefully", [], ROOT);
	} finally {
		vi.unstubAllGlobals();
	}
});

// Catches: the mounted mobile Chat textarea ignores the shared keyboard correction setting.
it("mobile Chat updates keyboard correction without changing desktop attributes", () => {
	const view = render(() => {
		const chat = createAcpChat(
			() => ROOT,
			() => true,
			client,
		);
		const correction = useKeyboardCorrection();
		return (
			<>
				<button onClick={correction.toggle}>Toggle correction</button>
				<Composer chat={chat} mobileAttachments />
				<Composer chat={chat} />
			</>
		);
	});
	const [mobile, desktop] = view.container.querySelectorAll("textarea");
	const expectAttributes = (enabled: boolean) => {
		expect(mobile.getAttribute("autocorrect")).toBe(enabled ? "on" : "off");
		expect(mobile.getAttribute("spellcheck")).toBe(String(enabled));
		expect(mobile.getAttribute("autocapitalize")).toBe(enabled ? "sentences" : "off");
		for (const attribute of ["autocorrect", "spellcheck", "autocapitalize"]) {
			expect(desktop.hasAttribute(attribute)).toBe(false);
		}
	};
	expectAttributes(true);
	view.getByRole("button", { name: "Toggle correction" }).click();
	expectAttributes(false);
	view.getByRole("button", { name: "Toggle correction" }).click();
	expectAttributes(true);
});

// Catches: a delayed async-offer snapshot rolls back the model after a successful settings change.
it("does not let a delayed async offer snapshot revert a confirmed model selection", async () => {
	const { createAcpClient } =
		await vi.importActual<typeof import("../../services/acpClient")>("../../services/acpClient");
	let deliver: ((frame: import("../../types/acp").AcpStreamFrame) => void) | undefined;
	const realClient = createAcpClient(async ({ onFrame }) => {
		deliver = onFrame;
		return { close() {} };
	});
	const offered = snapshot({ attachments: [attachment()] });
	const selected = snapshot({
		attachments: [attachment({ configOptions: [{ ...MODEL_OPTION, currentValue: "sonnet" }] })],
	});
	let releaseOffer!: (value: AcpConnectionSnapshot) => void;
	const delayedOffer = new Promise<AcpConnectionSnapshot>((resolve) => {
		releaseOffer = resolve;
	});
	let selectionConfirmed = false;
	vi.mocked(invoke).mockImplementation(async (command) => {
		if (command === "acp_connect") return offered;
		if (command === "acp_pending_interactions") return [];
		if (command === "acp_connection_snapshot") return selectionConfirmed ? selected : delayedOffer;
		if (command === "acp_session_set_config_option") {
			selectionConfirmed = true;
			return selected.attachments[0].configOptions;
		}
		return undefined;
	});
	await realClient.connect(ROOT);
	deliver?.({
		kind: "event",
		connectionId: CONNECTION,
		generation: 1,
		sequence: 2,
		sessionId: SESSION,
		turnId: null,
		event: {
			kind: "sessionUpdate",
			update: { sessionUpdate: "config_option_update", configOptions: [MODEL_OPTION] },
		},
	});
	await realClient.setConfigOption(CONNECTION, SESSION, "model", { value: "sonnet" });
	expect(acpStore.attachment(CONNECTION, SESSION)?.configOptions[0].currentValue).toBe("sonnet");
	releaseOffer(offered);
	await settle();
	expect(acpStore.attachment(CONNECTION, SESSION)?.configOptions[0].currentValue).toBe("sonnet");
});

// Catches: finishing delete cleanup overwrites a newer explicit conversation selection.
it("does not replace a newer selection while deleted conversation config cleanup is pending", async () => {
	supportsDelete = true;
	let dispose = () => {};
	const chat = createRoot((cleanup) => {
		dispose = cleanup;
		return createAcpChat(
			() => ROOT,
			() => true,
			client,
		);
	});
	try {
		await chat.ensureStarted();
		aiChatTabs.add("global", SECOND_SESSION);
		aiChatTabs.add("global", CHILD_SESSION);
		aiChatTabs.add("global", SESSION);
		client.deleteSession.mockResolvedValue(undefined);
		let releaseSave: (() => void) | undefined;
		const originalInvoke = vi.mocked(invoke).getMockImplementation()!;
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "save_config" && !releaseSave) {
				await new Promise<void>((resolve) => {
					releaseSave = resolve;
				});
				return undefined;
			}
			return originalInvoke(command, args);
		});
		const deleting = chat.deleteSession(SESSION);
		await settle();
		expect(releaseSave).toBeDefined();
		const selecting = chat.selectSession(CHILD_SESSION);
		await settle();
		expect(chat.sessionId()).toBe(CHILD_SESSION);
		releaseSave!();
		await Promise.all([deleting, selecting]);
		expect(chat.sessionId()).toBe(CHILD_SESSION);
		expect(aiChatTabs.active("global")).toBe(CHILD_SESSION);
	} finally {
		dispose();
	}
});

describe("AIChatPanel: delete conversation", () => {
	async function listedPanel() {
		supportsDelete = true;
		client.listSessions.mockResolvedValue({
			sessions: [{ sessionId: SESSION, cwd: CHAT_ROOT, title: "Delete this chat" }],
			nextCursor: null,
		});
		client.deleteSession.mockResolvedValue(undefined);
		return renderPanel();
	}

	// Catches: deleting the selected last conversation leaves a tab pointing at a missing session.
	it("deletes the selected last conversation and removes its tab and list entry", async () => {
		const { container } = await listedPanel();
		const button = container.querySelector<HTMLButtonElement>('button[aria-label="Delete Delete this chat"]');
		expect(button).not.toBeNull();
		button?.click();
		expect(container.textContent).toContain("Delete “Delete this chat”?");
		expect(client.deleteSession).not.toHaveBeenCalled();
		[...container.querySelectorAll<HTMLButtonElement>("button")]
			.find((button) => button.textContent === "Delete")
			?.click();
		await settle();
		expect(client.deleteSession).toHaveBeenCalledWith(CONNECTION, SESSION);
		expect(aiChatTabs.ids("global")).toEqual([]);
		expect(container.querySelector('button[aria-label="Delete Delete this chat"]')).toBeNull();
	});

	// Catches: deletion of an inactive tab closes the selected live neighbor.
	it("deletes an inactive conversation without changing the selected tab", async () => {
		supportsDelete = true;
		let dispose = () => {};
		const chat = createRoot((cleanup) => {
			dispose = cleanup;
			return createAcpChat(
				() => ROOT,
				() => true,
				client,
			);
		});
		await chat.ensureStarted();
		aiChatTabs.add("global", SECOND_SESSION);
		aiChatTabs.add("global", SESSION);
		client.deleteSession.mockResolvedValue(undefined);
		await chat.deleteSession(SECOND_SESSION);
		expect(client.deleteSession).toHaveBeenCalledWith(CONNECTION, SECOND_SESSION);
		expect(chat.sessionId()).toBe(SESSION);
		expect(aiChatTabs.ids("global")).toEqual([SESSION]);
		dispose();
	});

	// Catches: an ego refusal silently removes a still-running conversation.
	it("keeps the conversation and tab and shows a refused delete", async () => {
		const { container } = await listedPanel();
		client.deleteSession.mockRejectedValueOnce(new Error("conversation is running"));
		container.querySelector<HTMLButtonElement>('button[aria-label="Delete Delete this chat"]')?.click();
		[...container.querySelectorAll<HTMLButtonElement>("button")]
			.find((button) => button.textContent === "Delete")
			?.click();
		await settle();
		expect(container.textContent).toContain("conversation is running");
		expect(aiChatTabs.ids("global")).toEqual([SESSION]);
		expect(container.querySelector('button[aria-label="Delete Delete this chat"]')).not.toBeNull();
	});

	// Catches: a destructive control offered without the advertised ACP capability.
	it("hides deletion when the agent does not advertise it", async () => {
		const { container } = await listedPanel();
		acpStore.applySnapshot(
			snapshot({ capabilities: { ...snapshot().capabilities!, delete: false }, attachments: [attachment()] }),
		);
		await settle();
		expect(container.querySelector('button[aria-label="Delete Delete this chat"]')).toBeNull();
	});
});

// Catches: replayed completed calls being timed from the panel mount (1958).
it("does not invent observed execution time for loaded chat history", async () => {
	const { container } = await renderPanel();
	feed({
		kind: "sessionUpdate",
		update: {
			sessionUpdate: "tool_call",
			toolCallId: "history-read",
			title: "Read history",
			kind: "read",
			status: "completed",
		},
	});
	await settle();
	expect(container.querySelector("summary[class*=toolActivitySummary]")?.textContent).not.toMatch(/\d+\.\d+s observed/);
});

// Catches: the only tab retaining an unnecessary chrome row.
it("shows the tab strip only for multiple open tabs", async () => {
	const { container } = await renderPanel();
	expect(container.querySelector('[role="tablist"]')).toBeNull();
	client.newSession.mockImplementation(async () => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: SECOND_SESSION })] }));
		return SECOND_SESSION;
	});
	container.querySelector<HTMLButtonElement>('button[aria-label="New chat tab"]')!.click();
	await settle();
	expect(container.querySelectorAll('[role="tab"]')).toHaveLength(2);
});

// Catches: table documents opening in the detached WebView instead of the main document area.
it("routes detached table documents to the main window with exact source", async () => {
	window.history.replaceState(null, "", "/?mode=panel&panel=ai-chat");
	const { container } = await renderPanel();
	const source = "| A | B |\n| --- | --- |\n| first | second |";
	feed({
		kind: "sessionUpdate",
		update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: source } },
	});
	await settle();
	[...container.querySelectorAll<HTMLButtonElement>("button")]
		.find((button) => button.textContent === "Open as document")!
		.click();
	await settle();
	expect(emitTo).toHaveBeenCalledWith("main", "panel-action", {
		panelId: "ai-chat",
		action: "open-table",
		data: { source },
	});
	expect(mockAddVirtual).not.toHaveBeenCalled();
	aiChatPanelAdapter.handleAction?.("open-table", { source });
	expect(mockAddVirtual).toHaveBeenCalledWith("AI Chat table", expect.stringContaining("aichat-table:"));
});

// Catches: tidy header dropping inline close or detached window actions.
it("closes the inline panel from its header", () => {
	const close = vi.fn();
	const { container } = render(() => <AIChatPanel visible repoPath={ROOT} onClose={close} />);
	container.querySelector<HTMLButtonElement>('button[title="Close"]')!.click();
	expect(close).toHaveBeenCalledOnce();
});
it("reattaches the detached panel from its header", async () => {
	window.history.replaceState(null, "", "/?mode=panel&panel=ai-chat");
	const { container } = await renderPanel();
	container.querySelector<HTMLButtonElement>('button[title="Bring back to main window"]')!.click();
	expect(mockReattachPanel).toHaveBeenCalledWith("ai-chat");
});
it("closes the detached window from its header", async () => {
	window.history.replaceState(null, "", "/?mode=panel&panel=ai-chat");
	const { container } = await renderPanel();
	container.querySelector<HTMLButtonElement>('button[title="Close"]')!.click();
	expect(mockClosePanel).toHaveBeenCalledWith("ai-chat");
});

describe("AIChatPanel: ACP slash commands", () => {
	const offer = () => feed({ kind: "sessionUpdate", update: recordedCommands });
	const type = (container: HTMLElement, text: string) => {
		const input = container.querySelector("textarea")!;
		input.value = text;
		input.dispatchEvent(new Event("input", { bubbles: true }));
		return input;
	};
	it("shows descriptions and input hints — catches available_commands_update dropped so no menu appears", async () => {
		const { container } = await renderPanel();
		offer();
		type(container, "/capture");
		await settle();
		const menu = container.querySelector('[aria-label="Slash commands"]');
		expect(menu?.textContent).toContain("/capture-meeting");
		expect(menu?.textContent).toContain("Extract a meeting into the operational memory graph");
		expect(menu?.textContent).toContain("<path-to-notes | pasted text | Spinach reference>");
		expect(menu?.textContent).not.toContain("/imagegen");
	});
	it("runs the existing compact action — catches /compact sent to the model", async () => {
		const { container } = await renderPanel();
		client.compact.mockResolvedValueOnce({
			sourceSessionId: SESSION,
			targetSessionId: "compacted",
			publication: { kind: "not_published", diagnostic: "empty" },
		});
		await typeAndSend(container, "/compact");
		expect(client.compact).toHaveBeenCalledWith(CONNECTION, SESSION);
		expect(client.prompt).not.toHaveBeenCalled();
	});
	it.each(["/quit", "/q"])("ends the chat for %s — catches quit sent to the model", async (command) => {
		const { container } = await renderPanel();
		await typeAndSend(container, command);
		expect(client.disconnect).toHaveBeenCalledWith(CONNECTION);
		expect(client.prompt).not.toHaveBeenCalled();
	});
	it("forks the chat — catches /fork sent to the model", async () => {
		supportsFork = true;
		client.forkSession.mockResolvedValueOnce(CHILD_SESSION);
		const { container } = await renderPanel();
		await typeAndSend(container, "/fork");
		expect(client.forkSession).toHaveBeenCalledWith(CONNECTION, SESSION, CHAT_ROOT);
		expect(client.prompt).not.toHaveBeenCalled();
	});

	it("selects with keyboard and mouse and preserves prompt arguments — catches completion submitting or rewriting agent commands", async () => {
		const { container } = await renderPanel();
		offer();
		const input = type(container, "/capt");
		input.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
		expect(input.value).toBe("/capture-meeting ");
		expect(client.prompt).not.toHaveBeenCalled();
		await typeAndSend(container, "/capture-meeting notes with spaces");
		expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, "/capture-meeting notes with spaces", [], ROOT);
		type(container, "/ima");
		container.querySelector<HTMLButtonElement>('[role="option"]')!.click();
		expect(input.value).toBe("/imagegen ");
	});
	it("navigates, dismisses and completes without sending — catches arrows ignored or Escape cancelling a busy turn", async () => {
		const { container } = await renderPanel();
		const input = type(container, "/");
		input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
		input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
		expect(input.value).toBe("/clear ");
		expect(client.prompt).not.toHaveBeenCalled();
		type(container, "/");
		input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
		expect(container.querySelector('[aria-label="Slash commands"]')).toBeNull();
		expect(client.cancel).not.toHaveBeenCalled();
	});
	it.each([
		"/Users/x/file.ts is broken",
		"/tmp/error.log",
		"/quit/file.ts needs review",
		"//server/share/file.ts is broken",
		"/Users/x/My Project/file.ts is broken",
	])(
		"composer_sends_absolute_path_prompt: %s — catches filesystem paths rejected as unknown commands",
		async (text) => {
			const { container } = await renderPanel();
			await typeAndSend(container, text);
			expect(client.prompt).toHaveBeenCalledWith(CONNECTION, SESSION, text, [], ROOT);
			expect(container.querySelector("textarea")!.value).toBe("");
			expect(container.textContent).not.toContain("not available in AI Chat");
			expect(client.disconnect).not.toHaveBeenCalled();
		},
	);
	it.each(["/not-a-command", "/name new title", "/retry"])(
		"retains %s with available actions — catches unknown commands silently sent",
		async (command) => {
			const { container } = await renderPanel();
			await typeAndSend(container, command);
			expect(client.prompt).not.toHaveBeenCalled();
			expect(container.querySelector("textarea")!.value).toBe(command);
			expect(container.textContent).toContain("not available in AI Chat");
			expect(container.textContent).toContain("Available commands: /new, /clear");
		},
	);
	it("revokes stale offers and isolates sessions — catches commands leaking across tabs or surviving an empty replacement", async () => {
		const { container } = await renderPanel();
		offer();
		feed({ kind: "sessionUpdate", update: { sessionUpdate: "available_commands_update", availableCommands: [] } });
		type(container, "/capture");
		expect(container.querySelector('[aria-label="Slash commands"]')).toBeNull();
		feed({ kind: "sessionUpdate", update: recordedCommands }, SECOND_SESSION);
		expect(container.querySelector('[aria-label="Slash commands"]')).toBeNull();
	});
	it("keeps unadvertised host verbs unavailable — catches unsupported /fork or /compact falling through to a model", async () => {
		const { container } = await renderPanel();
		const current = acpStore.connection(CONNECTION)!;
		acpStore.applySnapshot({
			...current,
			capabilities: { ...current.capabilities!, fork: false, egoHoldVersion: null, egoCompactVersion: null },
		});
		for (const command of ["/fork", "/compact", "/pause", "/resume"]) {
			await typeAndSend(container, command);
			expect(container.textContent).toContain("not available in AI Chat");
		}
		expect(client.prompt).not.toHaveBeenCalled();
		expect(client.forkSession).not.toHaveBeenCalled();
		expect(client.compact).not.toHaveBeenCalled();
	});
	it("clears only the composer and reports existing usage — catches local actions becoming model turns", async () => {
		const { container } = await renderPanel();
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "usage_update", used: 100, size: 400, cost: { amount: 2, currency: "USD" } },
		});
		await typeAndSend(container, "/usage");
		expect(container.textContent).toContain("Context 100 / 400 tokens (25%) · USD 2");
		await typeAndSend(container, "/clear");
		expect(container.querySelector("textarea")!.value).toBe("");
		expect(acpTranscript.usage(SESSION)?.used).toBe(100);
		expect(client.prompt).not.toHaveBeenCalled();
	});
	it("starts a new conversation without sending /new — catches new action becoming a model turn", async () => {
		const { container } = await renderPanel();
		client.newSession.mockImplementationOnce(() => openSession(SECOND_SESSION));
		await typeAndSend(container, "/new");
		expect(aiChatTabs.active("global")).toBe(SECOND_SESSION);
		expect(client.prompt).not.toHaveBeenCalled();
	});
	it("uses hold actions and preserves busy drafts — catches pause/resume or busy compact sent as prompts", async () => {
		const { container } = await renderPanel();
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "prompting" })] }));
		await typeAndSend(container, "/compact");
		expect(container.querySelector("textarea")!.value).toBe("/compact");
		expect(client.compact).not.toHaveBeenCalled();
		await typeAndSend(container, "/pause");
		expect(client.pause).toHaveBeenCalledWith(CONNECTION, SESSION);
		acpStore.applySnapshot(snapshot({ attachments: [attachment({ state: "paused" })] }));
		await typeAndSend(container, "/resume");
		expect(client.resumeTurn).toHaveBeenCalledWith(CONNECTION, SESSION);
		expect(client.prompt).not.toHaveBeenCalled();
	});
});

describe("AI Chat reconnect lifecycle", () => {
	it("connects on opening without a message — catches manual-only startup", async () => {
		const view = renderIdlePanel();
		await settle();
		expect(client.connect).toHaveBeenCalledExactlyOnceWith(CHAT_ROOT);
		expect(client.newSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, CHAT_ROOT);
		expect(client.prompt).not.toHaveBeenCalled();
		view.unmount();
	});

	it("loads saved history on opening — catches a blank disconnected transcript", async () => {
		vi.mocked(invoke).mockImplementation(async (command) => {
			if (command === "ai_chat_selection") return { workspace: CHAT_ROOT, sessionId: SESSION };
			if (command === "load_config") return { ai_chat_sessions: { [CHAT_ROOT]: SESSION } };
			if (command === "acp_workspace_root") return CHAT_ROOT;
			return undefined;
		});
		client.loadSession.mockImplementation(async () => {
			await openSession(SESSION);
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Saved conversation answer" } },
			});
		});
		const { container } = renderIdlePanel();
		await settle();
		expect(container.textContent).toContain("Saved conversation answer");
		expect(client.loadSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, SESSION, CHAT_ROOT);
		expect(client.newSession).not.toHaveBeenCalled();
		expect(client.prompt).not.toHaveBeenCalled();
	});

	it("keeps the stopped conversation readable and resumes it on reopen — catches clearing selection on disconnect", async () => {
		await createRoot(async (dispose) => {
			const [active, setActive] = createSignal(true);
			const chat = createAcpChat(() => ROOT, active);
			await chat.ensureStarted();
			feed({
				kind: "sessionUpdate",
				update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Retained answer" } },
			});
			await chat.stop();
			expect(chat.sessionId()).toBe(SESSION);
			expect(chat.entries().map((entry) => (entry.kind === "agent" ? entry.text : ""))).toContain("Retained answer");
			expect(client.connect).toHaveBeenCalledTimes(1);
			client.loadSession.mockImplementation(async () => {
				await openSession(SESSION);
			});
			setActive(false);
			setActive(true);
			await settle();
			expect(client.connect).toHaveBeenCalledTimes(2);
			expect(client.loadSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, SESSION, CHAT_ROOT);
			expect(chat.sessionId()).toBe(SESSION);
			dispose();
		});
	});
});

describe("AI Chat automatic connection boundaries", () => {
	it.each(["succeeds", "fails"])(
		"cold Retry starts a connection without Send when the attempt %s — catches clearing the error without connecting",
		async (outcome) => {
			client.connect.mockReset().mockRejectedValueOnce(new Error("Agent unavailable"));
			const { container } = renderIdlePanel();
			await settle();
			expect(container.textContent).toContain("Agent unavailable");
			expect(client.connect).toHaveBeenCalledTimes(1);
			let finish!: (value: AcpConnectionSnapshot) => void;
			let fail!: (reason: Error) => void;
			client.connect.mockImplementationOnce(
				() =>
					new Promise<AcpConnectionSnapshot>((resolve, reject) => {
						finish = resolve;
						fail = reject;
					}),
			);
			const retry = [...container.querySelectorAll("button")].find((button) => button.textContent === "Retry");
			expect(retry).toBeDefined();
			retry?.click();
			await settle();
			expect(client.connect).toHaveBeenCalledTimes(2);
			expect(client.connect).toHaveBeenLastCalledWith(CHAT_ROOT);
			expect(container.textContent).toContain("Connecting conversation…");
			expect(client.newSession).not.toHaveBeenCalled();
			if (outcome === "succeeds") {
				const opened = snapshot();
				acpStore.applySnapshot(opened);
				acpStore.markStreaming(CONNECTION);
				finish(opened);
			} else {
				fail(new Error("Retry unavailable"));
			}
			await settle();
			await settle();
			expect(container.textContent).not.toContain("Connecting conversation…");
			if (outcome === "succeeds") {
				expect(container.textContent).not.toContain("Agent unavailable");
				expect(client.newSession).toHaveBeenCalledExactlyOnceWith(CONNECTION, CHAT_ROOT);
			} else {
				expect(container.textContent).toContain("Retry unavailable");
				expect(client.newSession).not.toHaveBeenCalled();
			}
			expect(client.connect).toHaveBeenCalledTimes(2);
			expect(client.reconnect).not.toHaveBeenCalled();
			expect(client.prompt).not.toHaveBeenCalled();
		},
	);

	it("does not start a hidden panel — catches eager background process launches", async () => {
		const view = render(() => <AIChatPanel visible={false} repoPath={ROOT} onClose={() => {}} />);
		await settle();
		expect(client.connect).not.toHaveBeenCalled();
		view.unmount();
	});

	it("keeps one failed open attempt without a retry storm — catches reactive reconnect loops", async () => {
		client.connect.mockRejectedValue(new Error("Agent unavailable"));
		const { container } = renderIdlePanel();
		await settle();
		await settle();
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(container.textContent).toContain("Agent unavailable");
		expect(client.newSession).not.toHaveBeenCalled();
	});

	it("shares a cold start between two views — catches duplicate processes on concurrent mount", async () => {
		const first = renderIdlePanel();
		const second = renderIdlePanel();
		await settle();
		expect(client.connect).toHaveBeenCalledTimes(1);
		expect(client.newSession).toHaveBeenCalledTimes(1);
		feed({
			kind: "sessionUpdate",
			update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Both views share this answer" } },
		});
		await settle();
		expect(first.container.textContent).toContain("Both views share this answer");
		expect(second.container.textContent).toContain("Both views share this answer");
	});
});

// Catches: a phone selection updates saved metadata but leaves desktop attached to the old transcript.
it("follows shared selection events and reconnect without republishing the adopted session", async () => {
	const view = await renderPanel();
	let selected = SECOND_SESSION;
	const originalInvoke = vi.mocked(invoke).getMockImplementation()!;
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "ai_chat_selection") return { workspace: CHAT_ROOT, sessionId: selected };
		return originalInvoke(command, args);
	});
	client.loadSession.mockImplementation(async (_id, session) => {
		acpStore.applySnapshot(snapshot({ attachments: [attachment(), attachment({ sessionId: session })] }));
	});
	const writesBefore = vi
		.mocked(invoke)
		.mock.calls.filter(([command]) => command === "select_ai_chat_conversation").length;
	const [handlers, options] = vi.mocked(subscribeEvents).mock.calls[0];
	handlers["ai-chat-selection-changed"]({});
	await settle();
	expect(view.container.querySelector(`[data-chat-session="${SECOND_SESSION}"]`)).not.toBeNull();
	await typeAndSend(view.container, "from the phone");
	expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SECOND_SESSION, "from the phone", [], ROOT);
	selected = CHILD_SESSION;
	options?.onResync?.("reconnect");
	await settle();
	await typeAndSend(view.container, "after reconnect");
	expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, CHILD_SESSION, "after reconnect", [], ROOT);
	expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "select_ai_chat_conversation")).toHaveLength(
		writesBefore,
	);
});

describe("queued prompt recall", () => {
	async function queuedPanel() {
		const view = await renderPanel();
		acpStore.applySnapshot(
			snapshot({
				attachments: [
					attachment({
						state: "prompting",
						activeTurn: { turnId: "running", state: "running", stopReason: null, usage: null },
						queuedPrompts: [
							{ turnId: "older", summary: "older" },
							{ turnId: "latest", summary: "short preview" },
						],
					}),
				],
			}),
		);
		await settle();
		return { ...view, textarea: view.container.querySelector("textarea") as HTMLTextAreaElement };
	}

	it("restores the latest full text only after withdrawal — catches summary recall and duplicate queued delivery", async () => {
		const { textarea, container } = await queuedPanel();
		let acknowledge!: (text: string) => void;
		client.recallQueued.mockImplementationOnce(
			() =>
				new Promise<string>((resolve) => {
					acknowledge = resolve;
				}),
		);
		const event = new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true, cancelable: true });
		textarea.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(true);
		expect(client.recallQueued).toHaveBeenCalledWith(CONNECTION, SESSION, "latest");
		expect(textarea.value).toBe("");
		expect(container.textContent).toContain("short preview");
		const original = "complete prompt\n".repeat(30);
		feed({ kind: "promptQueueChanged", queuedPrompts: [{ turnId: "older", summary: "older" }] });
		acknowledge(original);
		await settle();
		expect(textarea.value).toBe(original);
		expect(container.textContent).not.toContain("short preview");
		expect(client.prompt).not.toHaveBeenCalled();
		await typeAndSend(container, "edited prompt");
		expect(client.prompt).toHaveBeenCalledTimes(1);
	});

	it("keeps native ArrowUp for text and slash menus — catches recall stealing caret or command navigation", async () => {
		const { textarea } = await queuedPanel();
		for (const text of ["already typing", "/"]) {
			textarea.value = text;
			textarea.dispatchEvent(new Event("input", { bubbles: true }));
			textarea.setSelectionRange(0, 0);
			textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true, cancelable: true }));
			expect(textarea.value).toBe(text);
		}
		expect(client.recallQueued).not.toHaveBeenCalled();
	});

	it("keeps edits made during withdrawal — catches acknowledged recall overwriting the next draft", async () => {
		const { textarea } = await queuedPanel();
		let acknowledge!: (text: string) => void;
		client.recallQueued.mockImplementationOnce(
			() =>
				new Promise<string>((resolve) => {
					acknowledge = resolve;
				}),
		);
		textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
		textarea.value = "new typing";
		textarea.dispatchEvent(new Event("input", { bubbles: true }));
		acknowledge("recalled");
		await settle();
		expect(textarea.value).toBe("recalled\n\nnew typing");
	});

	it("leaves the queue and draft alone on refusal — catches already-dispatched or multimodal prompts being fabricated locally", async () => {
		const { textarea, container } = await queuedPanel();
		client.recallQueued.mockRejectedValueOnce(new Error("queued prompt not found"));
		textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
		await settle();
		expect(textarea.value).toBe("");
		expect(container.textContent).toContain("short preview");
		expect(container.textContent).toContain("queued prompt not found");
		expect(client.prompt).not.toHaveBeenCalled();
	});
});
