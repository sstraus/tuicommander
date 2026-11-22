// @vitest-environment jsdom
//
// Sending receipts across a connection replacement. Only the ACP client (the IPC
// boundary) and Tauri/transport plumbing are mocked; acpStore and the hook are real.

import { createRoot, createSignal } from "solid-js";
import { beforeEach, expect, it, vi } from "vitest";

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
vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));
vi.mock("../../transport", () => ({
	subscribeEvents: vi.fn(async () => () => {}),
	isTauri: () => true,
	owningConnectionFor: () => undefined,
	rpc: vi.fn(async () => ({})),
}));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));
vi.mock("../../stores/settings", () => ({
	settingsStore: {
		state: { egoExecutable: "/usr/local/bin/ego", defaultFontSize: 13 },
		isAiChatEnabled: () => true,
		isAcpConfigured: () => true,
	},
}));

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
	recallQueued: vi.fn(),
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
import { createAcpChat, resetAcpChatBindings } from "../../components/AIChatPanel/useAcpChat";
import { acpStore } from "../../stores/acp";
import { acpTranscript } from "../../stores/acpTranscript";
import { aiChatTabs } from "../../stores/aiChatTabs";
import type { AcpAttachmentSnapshot, AcpConnectionSnapshot } from "../../types/acp";

const ROOT = "/repo/tuicommander";
const CHAT_ROOT = "/srv/chat-workspace";
const CONNECTION = "01932d5e-0000-7000-8000-0000000000c1";
// acp_reconnect disconnects and connects again: the backend issues a new AcpConnectionId.
const REPLACEMENT = "01932d5e-0000-7000-8000-0000000000c2";
const SESSION = "01932d5e-0000-7000-8000-0000000000aa";

function attachment(sessionId: string): AcpAttachmentSnapshot {
	return {
		sessionId,
		state: "idle",
		cwd: CHAT_ROOT,
		additionalDirectories: [],
		configOptions: [],
		usage: null,
		activeTurn: null,
		queuedPrompts: [],
		pendingPermissionIds: [],
		pendingElicitationIds: [],
	};
}

function snapshot(connectionId: string, attachments: AcpAttachmentSnapshot[] = []): AcpConnectionSnapshot {
	return {
		connectionId,
		generation: 1,
		state: "ready",
		agentInfo: { name: "ego", version: "0.1.0" },
		capabilities: {
			protocol: 1,
			load: true,
			list: false,
			resume: true,
			fork: false,
			delete: false,
			close: true,
			additionalDirectories: true,
			promptImage: false,
			promptAudio: false,
			promptEmbeddedContext: false,
			mcpStdio: false,
			mcpHttp: true,
			mcpSse: false,
			mcpAcp: true,
			clientFormElicitation: true,
			clientBooleanConfig: false,
			egoHoldVersion: null,
			egoCompactVersion: null,
		},
		attachments,
		earliestSequence: 1,
		latestSequence: 1,
		settlement: null,
	};
}

async function settle(): Promise<void> {
	for (let turn = 0; turn < 6; turn += 1) await Promise.resolve();
	await new Promise((resolve) => setTimeout(resolve, 0));
}

beforeEach(() => {
	vi.clearAllMocks();
	vi.mocked(invoke).mockImplementation(async (command) => {
		if (command === "load_config") return { ai_chat_sessions: {} };
		if (command === "acp_workspace_root") return CHAT_ROOT;
		return undefined;
	});
	acpStore.reset();
	acpTranscript.reset();
	resetAcpChatBindings();
	localStorage.clear();
	aiChatTabs.resetMemory();
	client.connect.mockImplementation(async () => {
		const opened = snapshot(CONNECTION);
		acpStore.applySnapshot(opened);
		acpStore.markStreaming(CONNECTION);
		return opened;
	});
	client.newSession.mockImplementation(async (id: string) => {
		acpStore.applySnapshot(snapshot(id, [attachment(SESSION)]));
		return SESSION;
	});
	client.listSessions.mockResolvedValue({ sessions: [], nextCursor: null });
	client.disconnect.mockResolvedValue(undefined);
});

// Catches: a failed receipt keyed to the dead connection id disappearing (payload and
// attachment lost, Retry unreachable) once Recover replaces the connection for the same session.
it("keeps a failed send retryable on its session after the connection is replaced", async () => {
	let dispose = () => {};
	const [viewedRepo, setViewedRepo] = createSignal(ROOT);
	const chat = createRoot((cleanup) => {
		dispose = cleanup;
		return createAcpChat(viewedRepo, () => false, client);
	});
	try {
		expect(await chat.ensureStarted()).toBe(SESSION);
		expect(chat.connectionId()).toBe(CONNECTION);

		// The process dies while the prompt request is in flight.
		client.prompt.mockRejectedValueOnce({ code: "transport", message: "ACP connection closed" });
		const file = { name: "notes.md", path: "/workspace/notes.md" };
		await chat.send("Original payload", [], [file]);
		await settle();
		expect(chat.sendingPrompts().map((prompt) => [prompt.text, prompt.failed])).toEqual([["Original payload", true]]);

		// Recover: a fresh process replays the same durable session under a new connection id.
		client.reconnect.mockImplementation(async () => {
			const opened = snapshot(REPLACEMENT);
			acpStore.applySnapshot(opened);
			acpStore.markStreaming(REPLACEMENT);
			return opened;
		});
		client.loadSession.mockImplementation(async (id: string, session: string) => {
			acpStore.applySnapshot(snapshot(id, [attachment(session)]));
		});
		await chat.recover();
		await settle();
		expect(chat.connectionId()).toBe(REPLACEMENT);
		expect(chat.sessionId()).toBe(SESSION);

		// Recovery must not resend an ambiguous transport failure automatically.
		expect(client.prompt).toHaveBeenCalledTimes(1);
		setViewedRepo("/repos/other-project");
		const failed = chat.sendingPrompts();
		expect(failed.map((prompt) => [prompt.text, prompt.failed, prompt.files])).toEqual([
			["Original payload", true, [file]],
		]);
		client.prompt.mockResolvedValueOnce("turn-retried");
		await chat.retrySending(failed[0].localId);
		expect(client.prompt).toHaveBeenLastCalledWith(REPLACEMENT, SESSION, "Original payload", [], ROOT, [file]);
	} finally {
		dispose();
	}
});

const SUCCESSOR = "01932d5e-0000-7000-8000-0000000000bb";

// Catches: Send/Retry retaining the pair captured before a refused replay is reattached.
it.each(["send", "retry"])("uses the attached connection after failed replay on %s", async (action) => {
	let dispose = () => {};
	const chat = createRoot((cleanup) => {
		dispose = cleanup;
		return createAcpChat(
			() => ROOT,
			() => false,
			client,
		);
	});
	try {
		await chat.ensureStarted();
		client.prompt.mockRejectedValueOnce(new Error("connection lost"));
		await chat.send("Original payload");
		client.reconnect.mockImplementation(async () => {
			const opened = snapshot(REPLACEMENT);
			acpStore.applySnapshot(opened);
			return opened;
		});
		client.loadSession.mockRejectedValueOnce(new Error("replay refused"));
		await chat.recover();
		// The refused replay's process dies too. Selection must start another connection.
		acpStore.applySnapshot({ ...snapshot(REPLACEMENT), state: "closed" });
		client.loadSession.mockImplementation(async (id: string, session: string) => {
			acpStore.applySnapshot(snapshot(id, [attachment(session)]));
		});
		client.prompt.mockResolvedValueOnce("new-turn");
		if (action === "retry") await chat.retrySending(chat.sendingPrompts()[0].localId);
		else await chat.send("New payload");
		expect(chat.connectionId()).toBe(CONNECTION);
		expect(client.prompt).toHaveBeenLastCalledWith(
			CONNECTION,
			SESSION,
			action === "retry" ? "Original payload" : "New payload",
			[],
			ROOT,
		);
		expect(client.prompt).toHaveBeenCalledTimes(2);
	} finally {
		dispose();
	}
});

// Catches: a missing saved session being replaced while Retry still sends to its removed source.
it("rebinds a failed receipt to the missing-session successor before explicit retry", async () => {
	let dispose = () => {};
	const chat = createRoot((cleanup) => {
		dispose = cleanup;
		return createAcpChat(
			() => ROOT,
			() => false,
			client,
		);
	});
	try {
		await chat.ensureStarted();
		const file = { name: "source.md", path: "/repo/source.md" };
		client.prompt.mockRejectedValueOnce(new Error("connection lost"));
		await chat.send("Keep my payload", [], [file]);
		client.reconnect.mockImplementation(async () => {
			const opened = snapshot(REPLACEMENT);
			acpStore.applySnapshot(opened);
			return opened;
		});
		client.loadSession.mockRejectedValueOnce(new Error("replay refused"));
		await chat.recover();
		client.loadSession.mockRejectedValueOnce({
			code: "agent_error",
			operation: "load",
			message: 'Resource not found: "session not found"',
		});
		client.newSession.mockImplementationOnce(async (id: string) => {
			acpStore.applySnapshot(snapshot(id, [attachment(SUCCESSOR)]));
			return SUCCESSOR;
		});
		client.prompt.mockRejectedValueOnce(new Error("still offline"));
		await chat.retrySending(chat.sendingPrompts()[0].localId);
		expect(chat.sessionId()).toBe(SUCCESSOR);
		expect(aiChatTabs.ids("global")).toEqual([SUCCESSOR]);
		expect(client.prompt).toHaveBeenLastCalledWith(REPLACEMENT, SUCCESSOR, "Keep my payload", [], ROOT, [file]);
		expect(chat.sendingPrompts().map((p) => [p.text, p.failed, p.files])).toEqual([["Keep my payload", true, [file]]]);
	} finally {
		dispose();
	}
});

// Catches: compaction dropping the source tab and stranding its failed receipt.
it("keeps failed receipts reachable after compaction without automatically sending them", async () => {
	let dispose = () => {};
	const [repo, setRepo] = createSignal(ROOT);
	const chat = createRoot((cleanup) => {
		dispose = cleanup;
		return createAcpChat(repo, () => false, client);
	});
	try {
		await chat.ensureStarted();
		const source = snapshot(CONNECTION, [attachment(SESSION)]);
		source.capabilities!.egoCompactVersion = 1;
		acpStore.applySnapshot(source);
		const file = { name: "source.md", path: "/repo/source.md" };
		source.attachments[0].activeTurn = { turnId: "stale-source-turn", state: "running", stopReason: null, usage: null };
		acpStore.applySnapshot(source);
		client.prompt.mockRejectedValueOnce(new Error("send refused"));
		await chat.send("Unsent source payload", [], [file]);
		await settle();
		client.compact.mockResolvedValueOnce({
			sourceSessionId: SESSION,
			targetSessionId: SUCCESSOR,
			publication: { kind: "published_durably", diagnostic: null },
		});
		client.loadSession.mockImplementation(async (id: string, session: string) => {
			acpStore.applySnapshot(snapshot(id, [attachment(session)]));
		});
		await chat.compact();
		expect(aiChatTabs.ids("global")).toEqual([SUCCESSOR]);
		expect(client.prompt).toHaveBeenCalledTimes(1);
		const receipt = chat.sendingPrompts()[0];
		expect(receipt).toMatchObject({ text: "Unsent source payload", failed: true, files: [file] });
		setRepo("/repo/other");
		client.prompt.mockResolvedValueOnce("retry-turn");
		await chat.retrySending(receipt.localId);
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SUCCESSOR, "Unsent source payload", [], ROOT, [file]);
	} finally {
		dispose();
	}
});

// Catches: a ready connection with a removed attachment bypassing session/load forever.
it.each(["send", "retry"])("reattaches after a complete snapshot drops the attachment on %s", async (action) => {
	let dispose = () => {};
	const chat = createRoot((cleanup) => {
		dispose = cleanup;
		return createAcpChat(
			() => ROOT,
			() => false,
			client,
		);
	});
	try {
		await chat.ensureStarted();
		if (action === "retry") {
			client.prompt.mockRejectedValueOnce(new Error("send refused"));
			await chat.send("Original payload");
		}
		acpStore.applySnapshot(snapshot(CONNECTION));
		const callsBefore = client.prompt.mock.calls.length;
		client.loadSession.mockImplementationOnce(async (id: string, session: string) => {
			expect(client.prompt).toHaveBeenCalledTimes(callsBefore);
			acpStore.applySnapshot(snapshot(id, [attachment(session)]));
		});
		client.prompt.mockResolvedValueOnce("reattached-turn");
		if (action === "retry") await chat.retrySending(chat.sendingPrompts()[0].localId);
		else await chat.send("Original payload");
		expect(client.loadSession).toHaveBeenLastCalledWith(CONNECTION, SESSION, CHAT_ROOT);
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "Original payload", [], ROOT);
	} finally {
		dispose();
	}
});

// Catches: a receipt with no session disappearing when Recover or New creates the first session.
it.each(["recover", "new"])("keeps a pre-session failure reachable after %s without resending", async (action) => {
	let dispose = () => {};
	const chat = createRoot((cleanup) => {
		dispose = cleanup;
		return createAcpChat(
			() => ROOT,
			() => false,
			client,
		);
	});
	try {
		client.connect.mockRejectedValueOnce(new Error("offline before first session"));
		const file = { name: "notes.md", path: "/repo/notes.md" };
		await chat.send("Keep first payload", [], [file]);
		expect(chat.sessionId()).toBeNull();
		expect(chat.sendingPrompts()[0]).toMatchObject({ failed: true, files: [file] });
		if (action === "recover") await chat.recover();
		else await chat.startSession();
		expect(client.prompt).not.toHaveBeenCalled();
		expect(chat.sessionId()).toBe(SESSION);
		const receipt = chat.sendingPrompts()[0];
		expect(receipt).toMatchObject({ text: "Keep first payload", failed: true, files: [file] });
		client.prompt.mockResolvedValueOnce("first-retry-turn");
		await chat.retrySending(receipt.localId);
		expect(client.prompt).toHaveBeenLastCalledWith(CONNECTION, SESSION, "Keep first payload", [], ROOT, [file]);
	} finally {
		dispose();
	}
});

// Catches: old unselected tabs stranding a failed sessionless payload on any creation path.
it.each(["new", "recover", "custom"])(
	"retains a pre-session failure across %s with old unselected tabs",
	async (action) => {
		let dispose = () => {};
		aiChatTabs.add("global", SUCCESSOR);
		const originalInvoke = vi.mocked(invoke).getMockImplementation()!;
		vi.mocked(invoke).mockImplementation(async (command, args, options) =>
			command === "ai_chat_selection"
				? { workspace: CHAT_ROOT, sessionId: null }
				: originalInvoke(command, args, options),
		);
		const chat = createRoot((cleanup) => {
			dispose = cleanup;
			return createAcpChat(
				() => ROOT,
				() => false,
				client,
			);
		});
		try {
			await settle();
			client.newSession.mockRejectedValueOnce(new Error("first session refused"));
			const file = { name: "notes.md", path: "/repo/notes.md" };
			await chat.send("Keep unselected payload", [], [file]);
			expect(chat.sessionId()).toBeNull();
			expect(aiChatTabs.ids("global")).toEqual([SUCCESSOR]);
			expect(chat.sendingPrompts()[0]).toMatchObject({ failed: true, files: [file] });
			if (action === "recover")
				client.reconnect.mockImplementationOnce(async () => {
					const ready = snapshot(REPLACEMENT);
					acpStore.applySnapshot(ready);
					return ready;
				});
			if (action === "custom")
				client.openConversation.mockImplementationOnce(async () => {
					const ready = snapshot(REPLACEMENT, [attachment(SESSION)]);
					acpStore.applySnapshot(ready);
					return {
						connection: ready,
						sessionId: SESSION,
						replayed: false,
						launch: { masterPrompt: "", executable: "ego", profile: "custom", workspace: CHAT_ROOT, peerId: "" },
					};
				});
			if (action === "recover") await chat.recover();
			else await chat.startSession(action === "custom" ? { profile: "custom" } : undefined);
			expect(chat.sessionId()).toBe(SESSION);
			expect(client.prompt).not.toHaveBeenCalled();
			const receipt = chat.sendingPrompts()[0];
			expect(receipt).toMatchObject({ text: "Keep unselected payload", failed: true, files: [file] });
			client.prompt.mockResolvedValueOnce("retained-retry");
			await chat.retrySending(receipt.localId);
			expect(client.prompt).toHaveBeenCalledExactlyOnceWith(
				action === "new" ? CONNECTION : REPLACEMENT,
				SESSION,
				"Keep unselected payload",
				[],
				ROOT,
				[file],
			);
		} finally {
			dispose();
		}
	},
);

// Catches: selecting an unrelated existing tab adopting a pre-session failed send.
it.each([true, false])(
	"does not attach a pre-session receipt to an unrelated existing conversation (open tab: %s)",
	async (openTab) => {
		let dispose = () => {};
		const chat = createRoot((cleanup) => {
			dispose = cleanup;
			return createAcpChat(
				() => ROOT,
				() => false,
				client,
			);
		});
		try {
			client.connect.mockRejectedValueOnce(new Error("offline before first session"));
			await chat.send("Unowned payload");
			const localId = chat.sendingPrompts()[0].localId;
			if (openTab) aiChatTabs.add("global", SUCCESSOR);
			client.loadSession.mockImplementationOnce(async (id: string, session: string) => {
				acpStore.applySnapshot(snapshot(id, [attachment(session)]));
			});
			await chat.selectSession(SUCCESSOR);
			expect(chat.sendingPrompts()).toEqual([]);
			await chat.retrySending(localId);
			expect(client.prompt).not.toHaveBeenCalled();
		} finally {
			dispose();
		}
	},
);

// Catches: the empty selection sentinel hiding or rejecting the first send after deletion/deselection.
it.each(["delete-last", "host-empty", "host-with-tabs"])(
	"sends the first payload after %s selects no conversation",
	async (action) => {
		let dispose = () => {};
		if (action === "host-with-tabs") aiChatTabs.add("global", SESSION);
		if (action !== "delete-last") {
			const originalInvoke = vi.mocked(invoke).getMockImplementation()!;
			vi.mocked(invoke).mockImplementation(async (command, args, options) =>
				command === "ai_chat_selection"
					? { workspace: CHAT_ROOT, sessionId: null }
					: originalInvoke(command, args, options),
			);
		}
		const chat = createRoot((cleanup) => {
			dispose = cleanup;
			return createAcpChat(
				() => ROOT,
				() => false,
				client,
			);
		});
		try {
			if (action === "delete-last") {
				await chat.ensureStarted();
				const ready = snapshot(CONNECTION, [attachment(SESSION)]);
				ready.capabilities!.delete = true;
				acpStore.applySnapshot(ready);
				client.deleteSession.mockResolvedValueOnce(undefined);
				expect(await chat.deleteSession(SESSION)).toBe(true);
			} else await settle();
			expect(chat.sessionId()).toBeNull();
			let finishSession = () => {};
			client.newSession.mockImplementationOnce(
				(id: string) =>
					new Promise<string>((resolve) => {
						finishSession = () => {
							acpStore.applySnapshot(snapshot(id, [attachment(SUCCESSOR)]));
							resolve(SUCCESSOR);
						};
					}),
			);
			client.prompt.mockResolvedValueOnce("first-after-deselection");
			const sending = chat.send("First payload");
			const immediate = chat.sendingPrompts().map((p) => [p.text, p.failed]);
			await settle();
			const callsBeforeSession = client.prompt.mock.calls.length;
			finishSession();
			await sending;
			expect(immediate).toEqual([["First payload", false]]);
			expect(callsBeforeSession).toBe(0);
			expect(chat.sessionId()).toBe(SUCCESSOR);
			expect(client.prompt).toHaveBeenCalledExactlyOnceWith(CONNECTION, SUCCESSOR, "First payload", [], ROOT);
			expect(chat.sendingPrompts()[0]).toMatchObject({ text: "First payload", failed: false });
		} finally {
			dispose();
		}
	},
);

// Catches: a tab switch during reattachment delivering the old receipt into the new tab.
it("does not deliver to another tab selected while reattachment is pending", async () => {
	let dispose = () => {};
	const chat = createRoot((cleanup) => {
		dispose = cleanup;
		return createAcpChat(
			() => ROOT,
			() => false,
			client,
		);
	});
	try {
		await chat.ensureStarted();
		acpStore.applySnapshot(snapshot(CONNECTION));
		let finishLoad = () => {};
		client.loadSession.mockImplementationOnce(
			() =>
				new Promise<void>((resolve) => {
					finishLoad = resolve;
				}),
		);
		const sending = chat.send("Source payload");
		await settle();
		acpStore.applySnapshot(snapshot(CONNECTION, [attachment(SUCCESSOR)]));
		await chat.selectSession(SUCCESSOR);
		finishLoad();
		await sending;
		expect(chat.sessionId()).toBe(SUCCESSOR);
		expect(client.prompt).not.toHaveBeenCalled();
	} finally {
		dispose();
	}
});
