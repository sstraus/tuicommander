/**
 * One ego for the whole app, with one ACP session per open chat tab.
 *
 * The chat is global (Boss, 2026-09-28): the same tabs and the same
 * conversation whichever repository is on screen, and every session is rooted
 * at the workspace (`~/Gits`) rather than at one repository. The repository
 * being viewed travels with each prompt as a context hint, never as the scope.
 *
 * Opening the panel connects ego and restores the selected conversation.
 * Switching repository starts no process and loads no session: the
 * per-repository binding this replaced did both, and a webview reload that
 * lost it launched a fresh ego for every repository visited — 26 at once.
 *
 * The panel binds to a session, never to a terminal. A turn ego runs outlives
 * any tab, may touch files no tab is showing, and two windows are looking at
 * the same conversation.
 */

import { createEffect, createSignal, onCleanup, untrack } from "solid-js";
import { invoke } from "../../invoke";
import { type AcpListedSession, acpClient, type ChatLaunch, type ChatOpenRequest } from "../../services/acpClient";
import { createAiChatSelectionSync } from "../../services/aiChatSelection";
import { stopChatVoice } from "../../services/chatVoice";
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
import { createRetryCountdown } from "./retryCountdown";

/** The one connection this app uses and the tab it last showed. Module scope,
 *  so a panel that unmounts and comes back finds it rather than starting another. */
type Binding = { connectionId: AcpConnectionId; sessionId: AcpSessionId | null; root: string; launch?: ChatLaunch };
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
const refused = new Map<string, unknown>();

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

/** `ready` is configured but disconnected: retained history stays readable,
 *  and reopening the panel or sending a message attaches it again. */
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
	| "closeSession"
	| "deleteSession"
	| "listSessions"
	| "prompt"
	| "cancel"
	| "cancelQueued"
	| "recallQueued"
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

/** ego 0.2's recorded session/load refusal (ego-missing-session-2348.json).
 * The host currently preserves this ACP error as agent_error, not not_found. */
function missingSavedSession(failure: unknown): boolean {
	let error = failure;
	if (error instanceof Error || typeof error === "string") {
		const message = error instanceof Error ? error.message : error;
		const start = message.indexOf("{");
		if (start < 0) return false;
		try {
			error = JSON.parse(message.slice(start));
		} catch {
			return false;
		}
	}
	if (!error || typeof error !== "object") return false;
	const refusal = error as Partial<AcpClientError>;
	return (
		refusal.code === "agent_error" &&
		refusal.operation === "load" &&
		refusal.message === 'Resource not found: "session not found"'
	);
}

export function createAcpChat(
	viewedRepo: () => string | null,
	active: () => boolean,
	client: AcpChatClient = acpClient,
) {
	const [connectionId, setConnectionId] = createSignal<AcpConnectionId | null>(null);
	const [sessionId, setSessionId] = createSignal<AcpSessionId | null>(null);
	const retry = createRetryCountdown(() => {
		const session = sessionId();
		return session ? acpTranscript.retry(session) : null;
	});
	const [root, setRoot] = createSignal<string | null>(null);
	const [connecting, setConnecting] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const [listedSessions, setListedSessions] = createSignal<AcpListedSession[]>([]);
	let catalogueRequest = 0;
	let selection = 0;
	let selecting: { session: AcpSessionId; promise: Promise<void> } | null = null;
	let failedSession: AcpSessionId | null = null;
	type SubmittedPrompt = {
		id: AcpConnectionId;
		session: AcpSessionId;
		turnId: string;
		text: string;
		images: Extract<AcpContentBlock, { type: "image" }>[];
		files: { name: string; path: string }[];
		repo: string | null;
		retried: boolean;
	};
	const [submitted, setSubmitted] = createSignal<SubmittedPrompt[]>([]);
	const [compactingSource, setCompactingSource] = createSignal<{ id: AcpConnectionId; session: AcpSessionId } | null>(
		null,
	);
	let compaction: Promise<boolean> | null = null;

	aiChatTabs.refresh();
	async function readLaunches(): Promise<void> {
		const config = await invoke<{ ai_chat_launches?: Record<string, ChatLaunch> }>("load_config");
		setLaunches(config?.ai_chat_launches ?? {});
	}
	void readLaunches().catch((failure: unknown) =>
		appLogger.error("ai-chat", "reading conversation launches failed", failure),
	);

	async function openCustom(request: ChatOpenRequest, isCurrent = () => true, publish = true): Promise<boolean> {
		const opened = await guard("opening configured conversation", () => client.openConversation(request), isCurrent);
		if (!opened || !isCurrent()) return false;
		binding = {
			connectionId: opened.connection.connectionId,
			sessionId: opened.sessionId,
			root: opened.launch.workspace,
			launch: opened.launch,
		};
		setLaunches((current) => ({ ...current, [opened.sessionId]: opened.launch }));
		aiChatTabs.add(TABS, opened.sessionId);
		setRoot(opened.launch.workspace);
		setConnectionId(binding.connectionId);
		setSessionId(opened.sessionId);
		if (publish) await guard("saving conversation", () => remember(opened.sessionId), isCurrent);
		return true;
	}

	async function remember(session: AcpSessionId): Promise<void> {
		await sharedSelection.select(session);
	}

	/** Listing is optional discovery, never evidence that the selected session
	 * failed to attach. A slow or broken catalogue must not hold the composer. */
	async function refreshSessions(id: AcpConnectionId, target: string): Promise<void> {
		const request = ++catalogueRequest;
		const isCurrent = () => request === catalogueRequest && connectionId() === id;
		try {
			const rows: AcpListedSession[] = [];
			let cursor: string | undefined;
			do {
				const page = await client.listSessions(id, target, cursor);
				if (!isCurrent()) return;
				rows.push(...page.sessions.filter((session) => session.cwd === target));
				cursor = page.nextCursor || undefined;
			} while (cursor);
			setListedSessions(rows);
		} catch (failure) {
			appLogger.warn("ai-chat", "listing conversations failed", failure);
		}
	}

	/** Share the default connection even when several tab selections start it. */
	function connectDefault(target: string): Promise<Binding> {
		if (defaultBinding && acpStore.connection(defaultBinding.connectionId)?.state === "ready")
			return Promise.resolve(defaultBinding);
		defaultBinding = null;
		connectingDefault ??= (async () => {
			const snapshot = await client.connect(target);
			defaultBinding = { connectionId: snapshot.connectionId, sessionId: null, root: target };
			return defaultBinding;
		})().finally(() => {
			connectingDefault = null;
		});
		return connectingDefault;
	}

	/** Selecting a saved tab requests attachment before the first message.
	 *  Remounts preserve refused loads; explicit selections may retry them.
	 *  Only the latest selection may change the visible tab. */
	async function selectSession(session: AcpSessionId, explicit = true, publish = true): Promise<void> {
		if (sessionId() && sessionId() !== session) await stopChatVoice(sessionId());
		if (publish) {
			await sharedSelection.ready;
			sharedSelection.invalidate();
		}
		// Automatic open, Send and a tab click share the same attachment request.
		// Returning it also makes callers wait for its success or refusal.
		if (selecting?.session === session && session === sessionId()) return selecting.promise;
		const promise = attachSelected(session, explicit, publish).finally(() => {
			if (selecting?.promise === promise) selecting = null;
		});
		selecting = { session, promise };
		return promise;
	}

	async function attachSelected(session: AcpSessionId, explicit: boolean, publish: boolean): Promise<void> {
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
				if (await openCustom({ sessionId: session }, isCurrent, publish)) failedSession = null;
				return;
			}
			const target = await guard("finding the workspace", chatRoot, isCurrent);
			if (!target || !isCurrent()) return;
			const current = await guard("connecting to ego", () => connectDefault(target), isCurrent);
			if (!current || !isCurrent()) return;
			setRoot(target);
			setConnectionId(current.connectionId);
			let attached = session;
			if (!acpStore.attachment(current.connectionId, session)) {
				if (!acpStore.connection(current.connectionId)?.capabilities?.load) {
					setError("This agent cannot load saved conversations.");
					return;
				}
				const loaded = await loadSelected(current.connectionId, session, target, explicit, isCurrent);
				if (!loaded || !isCurrent()) return;
				attached = loaded;
			}
			binding = current;
			current.sessionId = attached;
			aiChatTabs.add(TABS, attached);
			setSessionId(attached);
			failedSession = null;
			if (publish || attached !== session) await guard("saving conversation", () => remember(attached), isCurrent);
			if (isCurrent() && connection()?.capabilities?.list) void refreshSessions(current.connectionId, target);
		} finally {
			if (isCurrent()) setConnecting(false);
		}
	}

	/** Run one action, holding what it refused rather than throwing at the panel. */
	async function guard<T>(what: string, action: () => Promise<T>, isCurrent = () => true): Promise<T | null> {
		try {
			const result = await action();
			if (isCurrent()) setError(null);
			return isCurrent() ? result : null;
		} catch (failure) {
			appLogger.error("ai-chat", `${what} failed`, failure);
			if (isCurrent()) setError(describe(failure));
			return null;
		}
	}

	/** Replay one session, at most once at a time. `explicit` is a person asking
	 *  again, the only thing that retries a refused load. Answers whether the
	 *  session can be shown: false when this load failed or a refusal stands.
	 *  Background tab refusals stay cached for selection, never fail the visible tab. */
	async function replay(
		what: string,
		id: AcpConnectionId,
		session: AcpSessionId,
		target: string,
		explicit = false,
		isCurrent = () => connectionId() === id && sessionId() === session,
	): Promise<boolean> {
		// A connection taken back after a reload still holds its attachments,
		// and a second load of one is refused.
		if (acpStore.attachment(id, session)) return true;
		const key = `${id}/${session}`;
		let loading = replaying.get(key);
		if (!loading) {
			if (refused.has(key) && !explicit) {
				if (isCurrent()) setError(describe(refused.get(key)));
				return false;
			}
			refused.delete(key);
			loading = (async () => {
				try {
					await client.loadSession(id, session, target);
					return true;
				} catch (failure) {
					appLogger.error("ai-chat", `${what} failed`, failure);
					refused.set(key, failure);
					return false;
				}
			})();
			replaying.set(key, loading);
		}
		const shown = await loading;
		replaying.delete(key);
		if (!shown && isCurrent()) setError(describe(refused.get(key)));
		return shown;
	}

	/** A stale browser tab is selection intent, not proof that ego still has it.
	 * Replace only the exact missing-session refusal; admission, trust and I/O
	 * failures must stay visible and must never create another conversation. */
	async function loadSelected(
		id: AcpConnectionId,
		session: AcpSessionId,
		target: string,
		explicit = false,
		isCurrent = () => true,
	): Promise<AcpSessionId | null> {
		if (await replay("loading conversation", id, session, target, explicit, isCurrent)) return session;
		if (!isCurrent() || !missingSavedSession(refused.get(`${id}/${session}`))) return null;
		const replacement = await guard(
			"opening a replacement conversation",
			() => client.newSession(id, target),
			isCurrent,
		);
		if (!replacement || !isCurrent()) return null;
		// Drop only this client's stale tab, never delete a durable conversation.
		aiChatTabs.remove(TABS, session);
		aiChatTabs.add(TABS, replacement);
		return replacement;
	}

	/** The session to talk to, starting ego for it on first use. One start at a
	 *  time for the app: a second caller waits on the first. */
	function start(): Promise<Started | null> {
		if (binding && acpStore.connection(binding.connectionId)?.state !== "ready") binding = null;
		const current = binding;
		const selected = sessionId() ?? aiChatTabs.active(TABS) ?? current?.sessionId;
		if (current && selected && acpStore.attachment(current.connectionId, selected)) {
			setRoot(current.root);
			setConnectionId(current.connectionId);
			setSessionId(selected);
			current.sessionId = selected;
			return Promise.resolve({ connectionId: current.connectionId, sessionId: selected, fresh: false });
		}
		starting ??= launch().finally(() => {
			starting = null;
		});
		return starting;
	}

	async function launch(): Promise<Started | null> {
		await sharedSelection.ready;
		const request = selection;
		const isCurrent = () => request === selection;
		setConnecting(true);
		try {
			if ((await guard("reading conversation launches", readLaunches, isCurrent)) === null) return null;
			const selected = aiChatTabs.active(TABS);
			if (selected && launches()[selected]) {
				if (!(await openCustom({ sessionId: selected }, isCurrent, false))) return null;
				const current = binding;
				return current?.sessionId
					? { connectionId: current.connectionId, sessionId: current.sessionId, fresh: false }
					: null;
			}
			const target = await guard("finding the workspace", chatRoot, isCurrent);
			if (!target) return null;
			setRoot(target);
			if (!binding) {
				const connected = await guard("connecting to ego", () => connectDefault(target), isCurrent);
				if (!connected) return null;
				binding = connected;
			}
			const current = binding;
			setConnectionId(current.connectionId);
			const capabilities = acpStore.connection(current.connectionId)?.capabilities;
			const config = await guard(
				"reading saved conversation",
				() => invoke<{ ai_chat_sessions?: Record<string, string> }>("load_config"),
				isCurrent,
			);
			if (!config) return null;
			const saved = aiChatTabs.active(TABS);
			if (saved && !capabilities?.load) {
				setError("This agent cannot load saved conversations.");
				return null;
			}
			if (saved && capabilities?.load) {
				aiChatTabs.ensure(TABS, saved);
				setSessionId(saved);
				const loaded = await loadSelected(current.connectionId, saved, target, false, isCurrent);
				if (!loaded) {
					if (isCurrent()) failedSession = saved;
					return null;
				}
				current.sessionId = loaded;
				setSessionId(loaded);
				if (loaded !== saved) await guard("saving conversation", () => remember(loaded));
				for (const tab of aiChatTabs.ids(TABS)) {
					if (tab !== saved && !launches()[tab])
						await replay("replaying a chat tab", current.connectionId, tab, target);
				}
				if (capabilities?.list) void refreshSessions(current.connectionId, target);
				return { connectionId: current.connectionId, sessionId: loaded, fresh: false };
			}
			const session = await guard(
				"opening a session",
				() => client.newSession(current.connectionId, target),
				isCurrent,
			);
			if (!session) return null;
			current.sessionId = session;
			aiChatTabs.replace(TABS, session);
			setSessionId(session);
			await guard("saving conversation", () => remember(session));
			if (capabilities?.list) void refreshSessions(current.connectionId, target);
			return { connectionId: current.connectionId, sessionId: session, fresh: true };
		} finally {
			if (isCurrent()) setConnecting(false);
		}
	}

	const [sharedRevision, setSharedRevision] = createSignal(0);
	const [selectionReady, setSelectionReady] = createSignal(false);
	const sharedSelection = createAiChatSelectionSync((snapshot) => {
		workspaceRoot = Promise.resolve(snapshot.workspace);
		if (defaultBinding && defaultBinding.root !== snapshot.workspace) {
			if (binding === defaultBinding) binding = null;
			defaultBinding = null;
		}
		const session = snapshot.sessionId;
		if (session === aiChatTabs.active(TABS) && session === sessionId() && attachment()) return;
		aiChatTabs.adopt(TABS, session);
		++selection; // Revoke an older attachment before it can republish stale focus.
		failedSession = null;
		if (!session) {
			if (binding) binding.sessionId = null;
			setSessionId(null);
			setConnecting(false);
		}
		setSharedRevision((revision) => revision + 1);
	});
	onCleanup(() => sharedSelection.dispose());
	void sharedSelection.ready.then(() => setSelectionReady(true));

	// One attempt per open/configuration or owner-selection edge, never per
	// repository or stream update. Read the owner before starting any session.
	createEffect(() => {
		if (!active() || !settingsStore.isAcpConfigured()) return;
		sharedRevision();
		if (!selectionReady()) return;
		untrack(() => {
			const current = binding;
			const selected = aiChatTabs.active(TABS);
			setSessionId(selected ?? current?.sessionId ?? null);
			if (!current || acpStore.connection(current.connectionId)?.state !== "ready") {
				if (selected && !starting) {
					void selectSession(selected, false, false);
					return;
				}
				const request = selection;
				void start().then((started) => {
					if (!started || request !== selection) return;
					setConnectionId(started.connectionId);
					setSessionId(started.sessionId);
					if (binding) setRoot(binding.root);
				});
				return;
			}
			setRoot(current.root);
			setConnectionId(current.connectionId);
			if (selected) void selectSession(selected, false, false);
			for (const tab of aiChatTabs.ids(TABS)) {
				if (
					tab !== selected &&
					!launches()[tab] &&
					!current.launch &&
					!acpStore.attachment(current.connectionId, tab)
				) {
					void replay("replaying a chat tab", current.connectionId, tab, current.root);
				}
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

	const compacting = (): boolean => {
		const source = compactingSource();
		return source != null && source.id === connectionId() && source.session === sessionId();
	};

	/** A turn is running, so the composer sends nothing and offers to stop. */
	const busy = (): boolean => {
		const state = attachment()?.state;
		return compacting() || state === "prompting" || state === "cancelling";
	};

	const held = (): boolean => {
		const state = attachment()?.state;
		return state === "paused" || state === "pause_pending";
	};

	function pair(): { id: AcpConnectionId; session: AcpSessionId } | null {
		const id = connectionId();
		const session = sessionId();
		return id && session && acpStore.attachment(id, session) ? { id, session } : null;
	}

	const pinnedRepo = () => aiChatTabs.pinnedRepo(TABS, sessionId() ?? "");
	const operatingRepo = () => pinnedRepo() ?? viewedRepo();
	/** Button, slash command and overflow recovery all use this one transition. */
	async function compact(): Promise<boolean> {
		const current = pair();
		if (!current) return false;
		const request = selection;
		while (compaction) {
			const source = compactingSource();
			if (source?.id === current.id && source.session === current.session) return compaction;
			await compaction;
			if (request !== selection || connectionId() !== current.id || sessionId() !== current.session) return false;
		}
		compaction = performCompact().finally(() => {
			compaction = null;
		});
		return compaction;
	}

	async function performCompact(): Promise<boolean> {
		const current = pair();
		if (!current) return false;
		if (connection()?.capabilities?.egoCompactVersion == null) {
			setError("This agent cannot compact conversations.");
			return false;
		}
		const request = selection;
		const isCurrent = () => request === selection && connectionId() === current.id && sessionId() === current.session;
		setCompactingSource(current);
		toastsStore.add("Compacting conversation", "Reducing context before continuing", "info");
		try {
			const compacted = await guard(
				"compacting the conversation",
				() => client.compact(current.id, current.session),
				isCurrent,
			);
			if (!compacted || !isCurrent()) return false;
			if (compacted.publication.kind === "not_published") {
				setError(`Compaction was not published: ${compacted.publication.diagnostic}`);
				return false;
			}
			await selectSession(compacted.targetSessionId);
			if (sessionId() !== compacted.targetSessionId || !attachment() || error()) return false;
			toastsStore.add("Conversation compacted", "Continuing in the compacted conversation", "info");
			const target = root();
			if (target && connection()?.capabilities?.list) void refreshSessions(connectionId()!, target);
			return true;
		} finally {
			setCompactingSource(null);
		}
	}

	async function submitPrompt(prompt: Omit<SubmittedPrompt, "turnId">): Promise<void> {
		const turnId = await guard("sending the turn", () =>
			prompt.files.length
				? client.prompt(prompt.id, prompt.session, prompt.text, prompt.images, prompt.repo, prompt.files)
				: client.prompt(prompt.id, prompt.session, prompt.text, prompt.images, prompt.repo),
		);
		if (turnId !== null) setSubmitted((pending) => [...pending, { ...prompt, turnId }]);
	}

	createEffect(() => {
		for (const prompt of submitted()) {
			const outcome = acpStore.turnOutcome(prompt.id, prompt.session);
			if (outcome?.turnId !== prompt.turnId) continue;
			// Consume before awaiting: a repaint or a repeated settlement cannot retry twice.
			setSubmitted((pending) => pending.filter((item) => item !== prompt));
			const event = outcome.event;
			const overflow =
				event.kind === "turnSettled"
					? event.stopReason === "_context_overflow"
					: event.message.includes("the provider context window is exhausted; invoke _ego/compact");
			if (!overflow || connectionId() !== prompt.id || sessionId() !== prompt.session) continue;
			if (prompt.retried) {
				setError("The context is still exhausted after compaction. Start a new conversation or shorten the prompt.");
				continue;
			}
			untrack(() => {
				void (async () => {
					if (!(await compact())) return;
					const successor = pair();
					if (successor) await submitPrompt({ ...prompt, ...successor, retried: true });
				})();
			});
		}
	});

	return {
		pinnedRepo,
		operatingRepo,
		toggleRepoPin(): void {
			const session = sessionId();
			if (session) aiChatTabs.pinRepo(TABS, session, pinnedRepo() ? null : viewedRepo());
		},
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
		retry,
		busy,
		compacting,
		queuedPrompts: () => attachment()?.queuedPrompts ?? [],
		held,

		capabilities: () => connection()?.capabilities ?? null,
		ensureStarted: async () => {
			if (failedSession) {
				await selectSession(failedSession);
				if (failedSession) return null;
			}
			const current = pair();
			return current ? current.session : ((await start())?.sessionId ?? null);
		},
		configOptions: (): AcpSessionConfigOption[] => attachment()?.configOptions ?? [],
		availableCommands: () => attachment()?.availableCommands ?? [],
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

		/**
		 * Give up on the conversation the agent refused to reopen: close it on the
		 * agent, drop its tab and clear the refusal, so a new session replaces it.
		 */
		async closeFailed(): Promise<void> {
			const id = connectionId();
			const session = failedSession ?? sessionId();
			failedSession = null;
			setError(null);
			if (!session) return;
			if (id) {
				try {
					await client.closeSession(id, session);
				} catch (failure) {
					// The agent may already have dropped it; the new chat must still open.
					appLogger.warn("ai-chat", "closing the refused session failed", failure);
				}
			}
			aiChatTabs.close(TABS, session);
			if (binding?.sessionId === session) binding.sessionId = null;
			setSessionId(null);
			// The last tab survives aiChatTabs.close; a launch would restore the refused
			// session, so the new chat reuses the default connection instead.
			if (!binding) {
				const target = await chatRoot();
				setRoot(target);
				const current = await guard("connecting to ego", () => connectDefault(target));
				if (current) {
					binding = current;
					setConnectionId(current.connectionId);
				}
			}
		},

		async deleteSession(session: AcpSessionId): Promise<boolean> {
			await stopChatVoice(session);
			const id = connectionId();
			if (!id || !connection()?.capabilities?.delete) return false;
			// A custom launch may use another executable/store, so use its owner.
			const deleted = await guard("deleting the conversation", async () => {
				const owner =
					launches()[session] && session !== sessionId()
						? (await client.openConversation({ sessionId: session })).connection.connectionId
						: !launches()[session] && binding?.launch
							? (await connectDefault(await chatRoot())).connectionId
							: id;
				await client.deleteSession(owner, session);
				return true;
			});
			if (!deleted) return false;
			setListedSessions((rows) => rows.filter((row) => row.sessionId !== session));
			setLaunches((current) => {
				const next = { ...current };
				delete next[session];
				return next;
			});
			acpTranscript.clear(session);
			const wasSelected = sessionId() === session;
			const next = aiChatTabs.remove(TABS, session);
			if (binding?.sessionId === session) binding.sessionId = null;
			if (defaultBinding?.sessionId === session) defaultBinding.sessionId = null;
			if (wasSelected) {
				++selection;
				failedSession = null;
				setSessionId(null);
			}
			const request = selection;
			await guard("forgetting the deleted conversation", () =>
				updateAppConfig<{
					ai_chat_sessions?: Record<string, string>;
					ai_chat_launches?: Record<string, ChatLaunch>;
				}>((config) => {
					for (const [root, saved] of Object.entries(config.ai_chat_sessions ?? {})) {
						if (saved === session) {
							if (next) config.ai_chat_sessions![root] = next;
							else delete config.ai_chat_sessions![root];
						}
					}
					if (config.ai_chat_launches) delete config.ai_chat_launches[session];
				}),
			);
			if (wasSelected && next && request === selection) await selectSession(next);
			return true;
		},

		async closeTab(tab: AcpSessionId): Promise<void> {
			await stopChatVoice(tab);
			const active = aiChatTabs.close(TABS, tab);
			// Closing another tab is not an explicit retry of the refused conversation.
			if (!active || active === failedSession) return;
			await selectSession(active);
		},

		/** Send a turn, attaching ego first when the conversation is disconnected. */
		async send(
			text: string,
			images: Extract<AcpContentBlock, { type: "image" }>[] = [],
			files: { name: string; path: string }[] = [],
		): Promise<void> {
			if (!text.trim() && images.length === 0 && files.length === 0) return;
			// The composer can queue while busy. Wait for the continuation rather
			// than dropping that new message or sending it to the exhausted source.
			if (compacting() && compaction && !(await compaction)) return;
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
			await submitPrompt({
				...current,
				text,
				images: [...images],
				files: [...files],
				repo: operatingRepo(),
				retried: false,
			});
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

		async recallQueued(): Promise<{ sessionId: string; text: string } | null> {
			const current = pair();
			const queued = attachment()?.queuedPrompts.at(-1);
			if (!current || !queued) return null;
			const text = await guard("recalling a queued prompt", () =>
				client.recallQueued(current.id, current.session, queued.turnId),
			);
			if (text === null) return null;
			setSubmitted((pending) => pending.filter((prompt) => prompt.turnId !== queued.turnId));
			return { sessionId: current.session, text };
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
			await compact();
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
			if (connection()?.capabilities?.list) void refreshSessions(current.id, target);
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
			if (sessionId()) await stopChatVoice(sessionId());
			await sharedSelection.ready;
			sharedSelection.invalidate();
			const request = ++selection;
			const isCurrent = () => request === selection;
			setConnecting(true);
			try {
				if (options && Object.keys(options).length > 0) {
					if (await openCustom(options, isCurrent)) failedSession = null;
					return;
				}
				if (binding?.launch || !binding) {
					binding = defaultBinding;
					setConnectionId(binding?.connectionId ?? null);
					setSessionId(binding?.sessionId ?? null);
					const target = await guard("finding the workspace", chatRoot, isCurrent);
					if (!target || !isCurrent()) return;
					setRoot(target);
					// New chat never replays the saved selection or inherits its title.
					if (!binding) {
						const current = await guard("connecting to ego", () => connectDefault(target), isCurrent);
						if (!current || !isCurrent()) return;
						binding = current;
						setConnectionId(current.connectionId);
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
				if (isCurrent() && connection()?.capabilities?.list) void refreshSessions(id, target);
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
				else await start();
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
				await openCustom({ sessionId: previous }, () => true, false);
				return;
			}
			setConnecting(true);
			try {
				const snapshot = await guard("reconnecting to ego", () => client.reconnect(id, target));
				if (!snapshot) return;
				setConnectionId(snapshot.connectionId);
				binding = { connectionId: snapshot.connectionId, sessionId: null, root: target };
				defaultBinding = binding;
				let resumeId = previous;
				if (!resumeId) {
					const config = await guard("reading saved conversation", () =>
						invoke<{ ai_chat_sessions?: Record<string, string> }>("load_config"),
					);
					if (!config) return;
					resumeId = config.ai_chat_sessions?.[target] ?? null;
				}
				if (resumeId && snapshot.capabilities?.load) {
					const loaded = await loadSelected(snapshot.connectionId, resumeId, target, true);
					if (!loaded) {
						failedSession = resumeId;
						return;
					}
					binding.sessionId = loaded;
					setSessionId(loaded);
					if (loaded !== resumeId) await guard("saving conversation", () => remember(loaded));
					for (const tab of aiChatTabs.ids(TABS)) {
						if (tab !== resumeId && !launches()[tab])
							await replay("replaying a chat tab", snapshot.connectionId, tab, target, true);
					}
					if (snapshot.capabilities?.list) void refreshSessions(snapshot.connectionId, target);
					return;
				}
				const session = await guard("opening a session", () => client.newSession(snapshot.connectionId, target));
				if (!session) return;
				binding.sessionId = session;
				aiChatTabs.replace(TABS, session);
				setSessionId(session);
				await guard("saving conversation", () => remember(session));
				if (snapshot.capabilities?.list) void refreshSessions(snapshot.connectionId, target);
			} finally {
				setConnecting(false);
			}
		},

		/** Stop ego for the app. The transcript stays: the panel is not where a
		 *  conversation is deleted. */
		async stop(): Promise<void> {
			if (sessionId()) await stopChatVoice(sessionId());
			const id = connectionId();
			if (!id) return;
			if (!binding?.launch) defaultBinding = null;
			binding = null;
			setConnectionId(null);
			// Selection and its transcript outlive the process. Reopening or Send
			// attaches this conversation again instead of presenting an empty one.
			await guard("disconnecting", () => client.disconnect(id));
		},
	};
}

export type AcpChat = ReturnType<typeof createAcpChat>;
