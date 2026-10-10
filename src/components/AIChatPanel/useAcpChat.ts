/**
 * One ego for the whole app, with one ACP session per open chat tab.
 *
 * The chat is global (Boss, 2026-09-28): the same tabs and the same
 * conversation whichever repository is on screen, and every session is rooted
 * at the workspace (`~/Gits`) rather than at one repository. The repository
 * being viewed travels with each prompt as a context hint, never as the scope.
 *
 * Nothing starts until a person sends a message or opens/selects a tab. Opening the
 * panel or switching repository starts no process and loads no session: the
 * per-repository binding this replaced did both, and a webview reload that
 * lost it launched a fresh ego for every repository visited — 26 at once.
 *
 * The panel binds to a session, never to a terminal. A turn ego runs outlives
 * any tab, may touch files no tab is showing, and two windows are looking at
 * the same conversation.
 */

import { createEffect, createSignal, untrack } from "solid-js";
import { invoke } from "../../invoke";
import { type AcpListedSession, acpClient, type ChatLaunch, type ChatOpenRequest } from "../../services/acpClient";
import { acpStore } from "../../stores/acp";
import { type AcpTranscriptEntry, acpTranscript } from "../../stores/acpTranscript";
import { aiChatTabs } from "../../stores/aiChatTabs";
import { appLogger } from "../../stores/appLogger";
import { settingsStore } from "../../stores/settings";
import { toastsStore } from "../../stores/toasts";
import type {
	AcpAttachmentSnapshot,
	AcpClientError,
	AcpConnectionId,
	AcpConnectionSnapshot,
	AcpContentBlock,
	AcpElicitationAction,
	AcpHostRequestId,
	AcpPendingInteraction,
	AcpSessionConfigOption,
	AcpSessionConfigOptionValue,
	AcpSessionId,
} from "../../types/acp";
import { updateAppConfig } from "../../utils/updateAppConfig";

/** The one connection this app uses and the tab it last showed. Module scope,
 *  so a panel that unmounts and comes back finds it rather than starting another. */
type Binding = { connectionId: AcpConnectionId; sessionId: AcpSessionId | null; launch?: ChatLaunch };
let binding: Binding | null = null;
let defaultBinding: Binding | null = null;
const [launches, setLaunches] = createSignal<Record<string, ChatLaunch>>({});

/** The start in flight, so a second send while ego launches waits for it. */
let starting: Promise<Started | null> | null = null;
let connectingDefault: Promise<Binding> | null = null;

/** Where every chat runs, asked of the backend once. */
let workspaceRoot: Promise<string> | null = null;

/** Tabs are one list for the app, not one per repository. */
const TABS = "global";

/** `session/load` calls in flight, keyed by connection and session. Each load
 *  makes ego admit every MCP server again, so one tab gets one load at a time. */
const replaying = new Map<string, Promise<boolean>>();

/** Loads that failed. Only an explicit action retries them: a reactive re-run
 *  that retried would turn one refusal into a storm of MCP initializes. */
const refused = new Map<string, string>();

/** What a start produced: the session to use, and whether it was just created. */
interface Started {
	connectionId: AcpConnectionId;
	sessionId: AcpSessionId;
	fresh: boolean;
}

/** Tests only: forget the connection and everything resolved for it. */
export function resetAcpChatBindings(): void {
	binding = null;
	defaultBinding = null;
	setLaunches({});
	starting = null;
	connectingDefault = null;
	workspaceRoot = null;
	replaying.clear();
	refused.clear();
}

/** The root every chat session runs in, as the backend resolves it from the
 *  `ai_chat_workspace` setting. */
function chatRoot(): Promise<string> {
	workspaceRoot ??= invoke<string>("acp_workspace_root").catch((failure: unknown) => {
		workspaceRoot = null;
		throw failure;
	});
	return workspaceRoot;
}

/** `ready` is configured with nothing started yet: the composer is live, and
 *  the first message is what launches ego. */
export type AcpChatPhase = "unconfigured" | "ready" | "starting" | "failed" | "live";

/** The client surface this hook drives, named so a test can hand it another. */
export type AcpChatClient = Pick<
	typeof acpClient,
	| "openConversation"
	| "connect"
	| "reconnect"
	| "disconnect"
	| "newSession"
	| "loadSession"
	| "listSessions"
	| "prompt"
	| "cancel"
	| "cancelQueued"
	| "answerPermission"
	| "cancelPermission"
	| "answerElicitation"
	| "setConfigOption"
	| "pause"
	| "resumeTurn"
	| "compact"
	| "forkSession"
>;

/** A refusal, as a person reads it. Rust answers with an `AcpClientError`. */
function describe(error: unknown): string {
	if (typeof error === "string") return error;
	if (error && typeof error === "object" && "message" in error) {
		return String((error as Partial<AcpClientError>).message);
	}
	return String(error);
}

export function createAcpChat(
	viewedRepo: () => string | null,
	active: () => boolean,
	client: AcpChatClient = acpClient,
) {
	const [connectionId, setConnectionId] = createSignal<AcpConnectionId | null>(null);
	const [sessionId, setSessionId] = createSignal<AcpSessionId | null>(null);
	const [root, setRoot] = createSignal<string | null>(null);
	const [connecting, setConnecting] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const [listedSessions, setListedSessions] = createSignal<AcpListedSession[]>([]);
	let selection = 0;
	let failedSession: AcpSessionId | null = null;

	aiChatTabs.refresh();
	async function readLaunches(): Promise<void> {
		const config = await invoke<{ ai_chat_launches?: Record<string, ChatLaunch> }>("load_config");
		setLaunches(config?.ai_chat_launches ?? {});
	}
	void readLaunches().catch((failure: unknown) =>
		appLogger.error("ai-chat", "reading conversation launches failed", failure),
	);

	async function openCustom(request: ChatOpenRequest, isCurrent = () => true): Promise<boolean> {
		const opened = await guard("opening configured conversation", () => client.openConversation(request), isCurrent);
		if (!opened || !isCurrent()) return false;
		binding = { connectionId: opened.connection.connectionId, sessionId: opened.sessionId, launch: opened.launch };
		setLaunches((current) => ({ ...current, [opened.sessionId]: opened.launch }));
		aiChatTabs.add(TABS, opened.sessionId);
		setRoot(opened.launch.workspace);
		setConnectionId(binding.connectionId);
		setSessionId(opened.sessionId);
		return true;
	}

	async function remember(session: AcpSessionId): Promise<void> {
		const target = await chatRoot();
		await updateAppConfig<{ ai_chat_sessions?: Record<string, string> }>((config) => {
			config.ai_chat_sessions = { ...config.ai_chat_sessions, [target]: session };
		});
	}

	async function refreshSessions(id: AcpConnectionId, target: string): Promise<void> {
		const rows: AcpListedSession[] = [];
		let cursor: string | undefined;
		do {
			const page = await client.listSessions(id, target, cursor);
			rows.push(...page.sessions.filter((session) => session.cwd === target));
			cursor = page.nextCursor || undefined;
		} while (cursor);
		setListedSessions(rows);
	}

	/** Share the default connection even when several tab selections start it. */
	function connectDefault(target: string): Promise<Binding> {
		if (defaultBinding) return Promise.resolve(defaultBinding);
		connectingDefault ??= (async () => {
			const snapshot = await client.connect(target);
			defaultBinding = { connectionId: snapshot.connectionId, sessionId: null };
			return defaultBinding;
		})().finally(() => {
			connectingDefault = null;
		});
		return connectingDefault;
	}

	/** Selecting a saved tab is an explicit request to attach, even before
	 *  the first message. Only the latest selection may change the visible tab. */
	async function selectSession(session: AcpSessionId): Promise<void> {
		if (session === sessionId() && attachment() && !connecting() && !failedSession) return;
		const request = ++selection;
		const isCurrent = () => request === selection;
		failedSession = session;
		setConnecting(true);
		if (aiChatTabs.ids(TABS).includes(session)) {
			aiChatTabs.add(TABS, session);
			setSessionId(session);
			setConnectionId(null);
		}
		try {
			if ((await guard("reading conversation launches", readLaunches, isCurrent)) === null || !isCurrent()) return;
			if (launches()[session]) {
				if (await openCustom({ sessionId: session }, isCurrent)) failedSession = null;
				return;
			}
			const target = await guard("finding the workspace", chatRoot, isCurrent);
			if (!target || !isCurrent()) return;
			const current = await guard("connecting to ego", () => connectDefault(target), isCurrent);
			if (!current || !isCurrent()) return;
			setRoot(target);
			setConnectionId(current.connectionId);
			if (!acpStore.attachment(current.connectionId, session)) {
				if (!acpStore.connection(current.connectionId)?.capabilities?.load) {
					setError("This agent cannot load saved conversations.");
					return;
				}
				if (
					!(await replay("loading conversation", current.connectionId, session, target, true, isCurrent)) ||
					!isCurrent()
				)
					return;
			}
			binding = current;
			current.sessionId = session;
			aiChatTabs.add(TABS, session);
			setSessionId(session);
			failedSession = null;
			await guard("saving conversation", () => remember(session), isCurrent);
			if (isCurrent() && connection()?.capabilities?.list)
				await guard("listing conversations", () => refreshSessions(current.connectionId, target), isCurrent);
		} finally {
			if (isCurrent()) setConnecting(false);
		}
	}

	/** Run one action, holding what it refused rather than throwing at the panel. */
	async function guard<T>(what: string, action: () => Promise<T>, isCurrent = () => true): Promise<T | null> {
		try {
			const result = await action();
			if (isCurrent()) setError(null);
			return result;
		} catch (failure) {
			appLogger.error("ai-chat", `${what} failed`, failure);
			if (isCurrent()) setError(describe(failure));
			return null;
		}
	}

	/** Replay one session, at most once at a time. `explicit` is a person asking
	 *  again, the only thing that retries a refused load. Answers whether the
	 *  session can be shown: false when this load failed or a refusal stands. */
	async function replay(
		what: string,
		id: AcpConnectionId,
		session: AcpSessionId,
		target: string,
		explicit = false,
		isCurrent = () => true,
	): Promise<boolean> {
		// A connection taken back after a reload still holds its attachments,
		// and a second load of one is refused.
		if (acpStore.attachment(id, session)) return true;
		const key = `${id}/${session}`;
		let loading = replaying.get(key);
		if (!loading) {
			if (refused.has(key) && !explicit) {
				if (isCurrent()) setError(refused.get(key) ?? null);
				return false;
			}
			refused.delete(key);
			loading = (async () => {
				try {
					await client.loadSession(id, session, target);
					return true;
				} catch (failure) {
					appLogger.error("ai-chat", `${what} failed`, failure);
					refused.set(key, describe(failure));
					return false;
				}
			})();
			replaying.set(key, loading);
		}
		const shown = await loading;
		replaying.delete(key);
		if (!shown && isCurrent()) setError(refused.get(key) ?? null);
		return shown;
	}

	/** The session to talk to, starting ego for it on first use. One start at a
	 *  time for the app: a second caller waits on the first. */
	function start(): Promise<Started | null> {
		const current = binding;
		if (current?.sessionId) {
			return Promise.resolve({ connectionId: current.connectionId, sessionId: current.sessionId, fresh: false });
		}
		starting ??= launch().finally(() => {
			starting = null;
		});
		return starting;
	}

	async function launch(): Promise<Started | null> {
		setConnecting(true);
		try {
			if ((await guard("reading conversation launches", readLaunches)) === null) return null;
			const selected = aiChatTabs.active(TABS);
			if (selected && launches()[selected]) {
				if (!(await openCustom({ sessionId: selected }))) return null;
				const current = binding;
				return current?.sessionId
					? { connectionId: current.connectionId, sessionId: current.sessionId, fresh: false }
					: null;
			}
			const target = await guard("finding the workspace", chatRoot);
			if (!target) return null;
			setRoot(target);
			if (!binding) {
				binding = await guard("connecting to ego", () => connectDefault(target));
				if (!binding) return null;
			}
			const current = binding;
			setConnectionId(current.connectionId);
			const capabilities = acpStore.connection(current.connectionId)?.capabilities;
			if (capabilities?.list) await guard("listing conversations", () => refreshSessions(current.connectionId, target));
			const config = await guard("reading saved conversation", () =>
				invoke<{ ai_chat_sessions?: Record<string, string> }>("load_config"),
			);
			if (!config) return null;
			const saved = aiChatTabs.active(TABS) ?? config.ai_chat_sessions?.[target];
			if (saved && !capabilities?.load) {
				setError("This agent cannot load saved conversations.");
				return null;
			}
			if (saved && capabilities?.load) {
				aiChatTabs.ensure(TABS, saved);
				current.sessionId = saved;
				setSessionId(saved);
				const shown = await replay("replaying the conversation", current.connectionId, saved, target);
				for (const tab of aiChatTabs.ids(TABS)) {
					if (tab !== saved && !launches()[tab])
						await replay("replaying a chat tab", current.connectionId, tab, target);
				}
				return shown ? { connectionId: current.connectionId, sessionId: saved, fresh: false } : null;
			}
			const session = await guard("opening a session", () => client.newSession(current.connectionId, target));
			if (!session) return null;
			current.sessionId = session;
			aiChatTabs.replace(TABS, session);
			setSessionId(session);
			await guard("saving conversation", () => remember(session));
			if (capabilities?.list) await guard("listing conversations", () => refreshSessions(current.connectionId, target));
			return { connectionId: current.connectionId, sessionId: session, fresh: true };
		} finally {
			setConnecting(false);
		}
	}

	// Showing the panel launches nothing. It takes back a connection this app
	// already has, with the tab last shown; with none, it shows the saved tabs
	// and waits for a message. The repository on screen is deliberately not read
	// here: switching it must never start or load anything.
	createEffect(() => {
		if (!active()) return;
		const current = binding;
		const selected = untrack(() => aiChatTabs.active(TABS));
		if (!current) {
			setConnectionId(null);
			setSessionId(selected);
			return;
		}
		if (current.launch) setRoot(current.launch.workspace);
		setConnectionId(current.connectionId);
		setSessionId(selected ?? current.sessionId);
		untrack(() => {
			const target = root();
			if (!target) return;
			for (const tab of aiChatTabs.ids(TABS)) {
				if (!launches()[tab] && !current.launch && !acpStore.attachment(current.connectionId, tab))
					void replay("replaying a chat tab", current.connectionId, tab, target);
			}
		});
	});

	const connection = (): AcpConnectionSnapshot | null => {
		const id = connectionId();
		return id ? acpStore.connection(id) : null;
	};

	const attachment = (): AcpAttachmentSnapshot | null => {
		const id = connectionId();
		const session = sessionId();
		return id && session ? acpStore.attachment(id, session) : null;
	};

	const noticedProfiles = new Set<AcpSessionId>();
	createEffect(() => {
		const current = attachment();
		if (!current?.profileWarnings?.length || noticedProfiles.has(current.sessionId)) return;
		noticedProfiles.add(current.sessionId);
		toastsStore.add("Repository ego profile", current.profileWarnings.join("\n"), "warn");
	});

	/** Only this session's questions. Another session on the same connection may
	 *  be blocked on its own, and answering it from here would answer blind. */
	const interactions = (): AcpPendingInteraction[] => {
		const id = connectionId();
		const session = sessionId();
		if (!id || !session) return [];
		return acpStore.interactions(id).filter((interaction) => interaction.sessionId === session);
	};

	const entries = (): AcpTranscriptEntry[] => {
		const session = sessionId();
		return session ? acpTranscript.entries(session) : [];
	};

	const phase = (): AcpChatPhase => {
		if (!settingsStore.isAcpConfigured() && !launches()[sessionId() ?? ""]?.executable) return "unconfigured";
		if (connecting()) return "starting";
		if (error()) return "failed";
		if (!connectionId() || !sessionId()) return "ready";
		return "live";
	};

	/** A turn is running, so the composer sends nothing and offers to stop. */
	const busy = (): boolean => {
		const state = attachment()?.state;
		return state === "prompting" || state === "cancelling";
	};

	const held = (): boolean => {
		const state = attachment()?.state;
		return state === "paused" || state === "pause_pending";
	};

	function pair(): { id: AcpConnectionId; session: AcpSessionId } | null {
		const id = connectionId();
		const session = sessionId();
		return id && session ? { id, session } : null;
	}

	return {
		/** The workspace every session runs in, once ego has been started. */
		root,
		/** The repository on screen, sent with each prompt as a hint. */
		viewedRepo,
		connectionId,
		sessionId,
		phase,
		error,
		clearError: () => setError(null),
		connection,
		attachment,
		interactions,
		entries,
		title: () => {
			const session = sessionId();
			return session ? acpTranscript.title(session) : null;
		},
		usage: () => {
			const session = sessionId();
			return session ? acpTranscript.usage(session) : null;
		},
		retry: () => {
			const session = sessionId();
			return session ? acpTranscript.retry(session) : null;
		},
		busy,
		queuedPrompts: () => attachment()?.queuedPrompts ?? [],
		held,

		capabilities: () => connection()?.capabilities ?? null,
		ensureStarted: async () => {
			if (failedSession) await selectSession(failedSession);
			else if (!pair()) await start();
		},
		configOptions: (): AcpSessionConfigOption[] => attachment()?.configOptions ?? [],
		gap: () => {
			const id = connectionId();
			return id ? acpStore.gap(id) : null;
		},
		isStreaming: () => {
			const id = connectionId();
			return id ? acpStore.isStreaming(id) : false;
		},
		/** Durable sessions published by ego, newest first. */
		launch: () => (sessionId() ? (launches()[sessionId() as string] ?? null) : null),
		sessions: (): AcpListedSession[] => [
			...listedSessions(),
			...Object.entries(launches())
				.filter(([id]) => !listedSessions().some((row) => row.sessionId === id))
				.map(([sessionId, launch]) => ({
					sessionId,
					cwd: launch.workspace,
				})),
		],
		tabs: () => aiChatTabs.ids(TABS),

		async closeTab(tab: AcpSessionId): Promise<void> {
			const active = aiChatTabs.close(TABS, tab);
			if (!active) return;
			await selectSession(active);
		},

		/** Send a turn, starting ego first when this is the first message. */
		async send(
			text: string,
			images: Extract<AcpContentBlock, { type: "image" }>[] = [],
			files: { name: string; path: string }[] = [],
		): Promise<void> {
			if (!text.trim() && images.length === 0 && files.length === 0) return;
			if (failedSession) {
				await selectSession(failedSession);
				if (failedSession) return;
			}
			let current = pair();
			if (!current) {
				const started = await start();
				current = started ? { id: started.connectionId, session: started.sessionId } : null;
			}
			if (!current) return;
			const { id, session } = current;
			await guard("sending the turn", () =>
				files.length
					? client.prompt(id, session, text, images, viewedRepo(), files)
					: client.prompt(id, session, text, images, viewedRepo()),
			);
		},

		async cancel(): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("cancelling the turn", () => client.cancel(current.id, current.session));
		},

		async cancelQueued(turnId: string): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("cancelling a queued prompt", () => client.cancelQueued(current.id, current.session, turnId));
		},

		async pause(): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("pausing the turn", () => client.pause(current.id, current.session));
		},

		async resume(): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("resuming the turn", () => client.resumeTurn(current.id, current.session));
		},

		async compact(): Promise<void> {
			const current = pair();
			if (!current) return;
			const compacted = await guard("compacting the conversation", () => client.compact(current.id, current.session));
			if (compacted === null) return;
			// ego continues the conversation in a new session. Without opening it the
			// button appeared to do nothing: the source tab stayed exactly as it was.
			if (compacted.publication.kind === "not_published") {
				setError(`Compaction was not published: ${compacted.publication.diagnostic}`);
				return;
			}
			toastsStore.add("Conversation compacted", "Continuing in the compacted conversation", "info");
			await selectSession(compacted.targetSessionId);
			const target = root();
			if (target && connection()?.capabilities?.list)
				await guard("listing conversations", () => refreshSessions(current.id, target));
		},

		/** Branch this conversation at its tip into a new tab; the parent keeps its own. */
		async fork(atMessageId?: string): Promise<void> {
			const current = pair();
			const target = root();
			if (!current || !target || busy()) return;
			const child = await guard("forking the conversation", () =>
				atMessageId
					? client.forkSession(current.id, current.session, target, atMessageId)
					: client.forkSession(current.id, current.session, target),
			);
			if (child === null) return;
			await selectSession(child);
			if (connection()?.capabilities?.list)
				await guard("listing conversations", () => refreshSessions(current.id, target));
		},

		async setOption(configId: string, value: AcpSessionConfigOptionValue): Promise<void> {
			const current = pair();
			if (!current) return;
			await guard("setting a session option", () =>
				client.setConfigOption(current.id, current.session, configId, value),
			);
		},

		async answerPermission(requestId: AcpHostRequestId, optionId: string): Promise<void> {
			const id = connectionId();
			if (!id) return;
			await guard("answering a permission request", () => client.answerPermission(id, requestId, optionId));
		},

		async cancelPermission(requestId: AcpHostRequestId): Promise<void> {
			const id = connectionId();
			if (!id) return;
			await guard("dismissing a permission request", () => client.cancelPermission(id, requestId));
		},

		async answerElicitation(requestId: AcpHostRequestId, action: AcpElicitationAction): Promise<void> {
			const id = connectionId();
			if (!id) return;
			await guard("answering a form", () => client.answerElicitation(id, requestId, action));
		},

		/** Open another conversation tab, starting ego when none is running. */
		async startSession(options?: ChatOpenRequest): Promise<void> {
			const request = ++selection;
			const isCurrent = () => request === selection;
			setConnecting(true);
			try {
				if (options && Object.keys(options).length > 0) {
					if (await openCustom(options, isCurrent)) failedSession = null;
					return;
				}
				if (binding?.launch || (!binding && failedSession)) {
					binding = defaultBinding;
					setConnectionId(binding?.connectionId ?? null);
					setSessionId(binding?.sessionId ?? null);
					const target = await chatRoot();
					if (!isCurrent()) return;
					setRoot(target);
					// A default new chat must not restore the currently selected custom chat.
					if (!binding) {
						const current = await guard("connecting to ego", () => connectDefault(target), isCurrent);
						if (!current || !isCurrent()) return;
						binding = current;
						setConnectionId(current.connectionId);
					}
				}
				if (!pair() && !binding) {
					const started = await start();
					if (!started || !isCurrent()) return;
					if (started.fresh) {
						failedSession = null;
						return;
					}
				}
				const id = connectionId();
				const target = root();
				if (!id || !target) return;
				const session = await guard("opening a session", () => client.newSession(id, target), isCurrent);
				if (!session || !isCurrent()) return;
				if (binding) binding.sessionId = session;
				aiChatTabs.add(TABS, session);
				setSessionId(session);
				failedSession = null;
				await guard("saving conversation", () => remember(session), isCurrent);
				if (isCurrent() && connection()?.capabilities?.list)
					await guard("listing conversations", () => refreshSessions(id, target), isCurrent);
			} finally {
				if (isCurrent()) setConnecting(false);
			}
		},

		selectSession,

		/**
		 * Replace the process and pick the conversation back up.
		 *
		 * The recovery for a gap, and for a connection that died: the journal no
		 * longer holds what the cursor asks for, so a fresh process replays the
		 * history through `session/load` instead. An agent that cannot load starts
		 * a new session — an empty panel is honest, a silently truncated one is not.
		 */
		async recover(): Promise<void> {
			if (failedSession || !connectionId()) {
				const session = failedSession ?? sessionId();
				if (session) await selectSession(session);
				else setError(null);
				return;
			}
			const target = root();
			const id = connectionId();
			// Nothing was started, so there is nothing to replace: the refusal is
			// cleared and the next message starts ego again.
			if (!target || !id) {
				setError(null);
				return;
			}
			const previous = sessionId();
			if (previous && launches()[previous]) {
				await guard("disconnecting configured conversation", () => client.disconnect(id));
				await openCustom({ sessionId: previous });
				return;
			}
			setConnecting(true);
			try {
				const snapshot = await guard("reconnecting to ego", () => client.reconnect(id, target));
				if (!snapshot) return;
				setConnectionId(snapshot.connectionId);
				binding = { connectionId: snapshot.connectionId, sessionId: previous };
				defaultBinding = binding;
				if (snapshot.capabilities?.list)
					await guard("listing conversations", () => refreshSessions(snapshot.connectionId, target));
				let resumeId = previous;
				if (!resumeId) {
					const config = await guard("reading saved conversation", () =>
						invoke<{ ai_chat_sessions?: Record<string, string> }>("load_config"),
					);
					if (!config) return;
					resumeId = config.ai_chat_sessions?.[target] ?? null;
				}
				if (resumeId && snapshot.capabilities?.load) {
					binding.sessionId = resumeId;
					setSessionId(resumeId);
					await replay("replaying the conversation", snapshot.connectionId, resumeId, target, true);
					for (const tab of aiChatTabs.ids(TABS)) {
						if (tab !== resumeId && !launches()[tab])
							await replay("replaying a chat tab", snapshot.connectionId, tab, target, true);
					}
					return;
				}
				const session = await guard("opening a session", () => client.newSession(snapshot.connectionId, target));
				if (!session) return;
				binding.sessionId = session;
				aiChatTabs.replace(TABS, session);
				setSessionId(session);
				await guard("saving conversation", () => remember(session));
				if (snapshot.capabilities?.list)
					await guard("listing conversations", () => refreshSessions(snapshot.connectionId, target));
			} finally {
				setConnecting(false);
			}
		},

		/** Stop ego for the app. The transcript stays: the panel is not where a
		 *  conversation is deleted. */
		async stop(): Promise<void> {
			const id = connectionId();
			if (!id) return;
			if (!binding?.launch) defaultBinding = null;
			binding = null;
			setConnectionId(null);
			setSessionId(null);
			await guard("disconnecting", () => client.disconnect(id));
		},
	};
}

export type AcpChat = ReturnType<typeof createAcpChat>;
