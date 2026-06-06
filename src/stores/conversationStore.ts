/**
 * Unified conversation store — merges aiChatStore and aiAgentStore.
 *
 * Per-terminal architecture: each terminal gets its own PerTerminalConversationState
 * keyed by tuicSession/id. Proxy accessors (messages, isStreaming, etc.) route through
 * activeConversation() so all callers remain unchanged.
 *
 * Both chat (assisted) and agent (autonomous) modes now drive the same Tauri command
 * `start_conversation`. Events arrive via Channel<ConversationEvent> and are applied
 * to the matching terminal's state.
 */

import type { Accessor, Setter } from "solid-js";
import { batch, createSignal } from "solid-js";
import { isTauri } from "../transport";
import { appLogger } from "./appLogger";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ConversationMessage {
	role: "user" | "assistant" | "system";
	content: string;
	timestamp: number;
}

/** Re-exported as AiChatMessage for backward compat */
export type AiChatMessage = ConversationMessage;

interface ChatUsage {
	promptTokens?: number;
	completionTokens?: number;
	totalTokens?: number;
	cachedTokens?: number;
	cacheCreationTokens?: number;
	costUsd?: number;
}

export type AgentState = "running" | "paused" | "completed" | "cancelled" | "error" | "idle";

export type ToolCallEntry =
	| { status: "pending"; toolName: string; args: Record<string, unknown>; startedAt: number }
	| {
			status: "done";
			toolName: string;
			args: Record<string, unknown>;
			startedAt: number;
			result: { success: boolean; output: string };
			duration: number;
	  };

export interface PendingApproval {
	sessionId: string;
	command: string;
	reason: string;
}

// Backend conversation types (mirror ai_agent::conversation)
interface BackendChatMessage {
	role: "user" | "assistant" | "system";
	content: string;
	timestamp: number;
}
interface BackendConversationMeta {
	id: string;
	title: string;
	session_id?: string | null;
	created: number;
	updated: number;
	message_count: number;
	provider?: string;
	model?: string;
}
interface BackendConversation {
	meta: BackendConversationMeta;
	messages: BackendChatMessage[];
	schema_version?: number;
}

export type ConversationMeta = BackendConversationMeta;

// ConversationEvent variants from Rust (tag = "type", rename_all = "snake_case")
type ConversationEvent =
	| { type: "thinking"; iteration: number }
	| { type: "text_chunk"; text: string }
	| { type: "reasoning_chunk"; text: string }
	| { type: "tool_call"; tool_name: string; args: Record<string, unknown> }
	| { type: "tool_result"; tool_name: string; success: boolean; output: string }
	| { type: "needs_approval"; tool_name: string; command: string; reason: string }
	| { type: "bypassed"; tool_name: string }
	| { type: "paused" }
	| { type: "resumed" }
	| { type: "rate_limited"; wait_ms: number }
	| { type: "error"; message: string }
	| { type: "completed"; reason: string; usage: { input_tokens: number; output_tokens: number } | null };

// Legacy AgentEvent for backward compat with old agent-loop-event listener (removed in 1617)
type LegacyAgentEvent =
	| { type: "started"; session_id: string }
	| { type: "thinking"; session_id: string; iteration: number }
	| { type: "text_chunk"; session_id: string; text: string }
	| { type: "reasoning_chunk"; session_id: string; text: string }
	| { type: "tool_call"; session_id: string; tool_name: string; args: Record<string, unknown> }
	| { type: "tool_result"; session_id: string; tool_name: string; success: boolean; output: string }
	| { type: "needs_approval"; session_id: string; tool_name: string; command: string; reason: string }
	| { type: "paused"; session_id: string }
	| { type: "resumed"; session_id: string }
	| { type: "rate_limited"; session_id: string; wait_ms: number }
	| { type: "error"; session_id: string; message: string }
	| { type: "completed"; session_id: string; iterations: number; reason: string };

// Registry subscription for cross-window sync
type RegistryChatEvent =
	| {
			kind: "snapshot";
			messages: Array<{ role: string; content: string; timestamp: number }>;
			isStreaming: boolean;
			streamingText: string;
			error: string | null;
			attachedSessionId: string | null;
			pinned: boolean;
	  }
	| { kind: "chunk"; delta: string }
	| { kind: "error"; message: string }
	| { kind: "cleared" };

interface RegistrySubscription {
	chatId: string;
	subscriptionId: number;
	cleanup: () => Promise<void>;
}

/** Conversation mode: "assisted" = chat streaming, "autonomous" = agent with tool cards */
type ConversationMode = "assisted" | "autonomous";

export interface PerTerminalConversationState {
	// Chat state (formerly aiChatStore)
	messages: Accessor<ConversationMessage[]>;
	setMessages: Setter<ConversationMessage[]>;
	isStreaming: Accessor<boolean>;
	setIsStreaming: Setter<boolean>;
	streamingText: Accessor<string>;
	setStreamingText: Setter<string>;
	error: Accessor<string | null>;
	setError: Setter<string | null>;
	chatId: Accessor<string>;
	setChatId: Setter<string>;
	sessionUsage: Accessor<ChatUsage | null>;
	setSessionUsage: Setter<ChatUsage | null>;
	// Agent state (formerly aiAgentStore)
	agentState: Accessor<AgentState>;
	setAgentState: Setter<AgentState>;
	currentIteration: Accessor<number>;
	setCurrentIteration: Setter<number>;
	toolCalls: Accessor<ToolCallEntry[]>;
	setToolCalls: Setter<ToolCallEntry[]>;
	textChunks: Accessor<string>;
	setTextChunks: Setter<string>;
	reasoningChunks: Accessor<string>;
	setReasoningChunks: Setter<string>;
	pendingApproval: Accessor<PendingApproval | null>;
	setPendingApproval: Setter<PendingApproval | null>;
	agentError: Accessor<string | null>;
	setAgentError: Setter<string | null>;
	completionReason: Accessor<string | null>;
	setCompletionReason: Setter<string | null>;
	unrestricted: Accessor<boolean>;
	setUnrestricted: Setter<boolean>;
	isThinking: Accessor<boolean>;
	setIsThinking: Setter<boolean>;
	// Bookkeeping
	currentMode: ConversationMode | null;
	activeSessionId: string | null;
	persistTimer: ReturnType<typeof setTimeout> | null;
	initialized: boolean;
	registrySubscription: RegistrySubscription | null;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_MESSAGES = 100;
const MAX_TOOL_CALLS = 500;
const PERSIST_DEBOUNCE_MS = 500;
const DEFAULT_KEY = "__default__";

// ---------------------------------------------------------------------------
// Per-terminal state map
// ---------------------------------------------------------------------------

const stateMap = new Map<string, PerTerminalConversationState>();
const [activeKey, setActiveKey] = createSignal<string>(DEFAULT_KEY);

function generateChatId(): string {
	return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
}

function createState(): PerTerminalConversationState {
	const [messages, setMessages] = createSignal<ConversationMessage[]>([]);
	const [isStreaming, setIsStreaming] = createSignal(false);
	const [streamingText, setStreamingText] = createSignal("");
	const [error, setError] = createSignal<string | null>(null);
	const [chatId, setChatId] = createSignal(generateChatId());
	const [sessionUsage, setSessionUsage] = createSignal<ChatUsage | null>(null);
	const [agentState, setAgentState] = createSignal<AgentState>("idle");
	const [currentIteration, setCurrentIteration] = createSignal(0);
	const [toolCalls, setToolCalls] = createSignal<ToolCallEntry[]>([]);
	const [textChunks, setTextChunks] = createSignal("");
	const [reasoningChunks, setReasoningChunks] = createSignal("");
	const [pendingApproval, setPendingApproval] = createSignal<PendingApproval | null>(null);
	const [agentError, setAgentError] = createSignal<string | null>(null);
	const [completionReason, setCompletionReason] = createSignal<string | null>(null);
	const [unrestricted, setUnrestricted] = createSignal(false);
	const [isThinking, setIsThinking] = createSignal(false);
	return {
		messages,
		setMessages,
		isStreaming,
		setIsStreaming,
		streamingText,
		setStreamingText,
		error,
		setError,
		chatId,
		setChatId,
		sessionUsage,
		setSessionUsage,
		agentState,
		setAgentState,
		currentIteration,
		setCurrentIteration,
		toolCalls,
		setToolCalls,
		textChunks,
		setTextChunks,
		reasoningChunks,
		setReasoningChunks,
		pendingApproval,
		setPendingApproval,
		agentError,
		setAgentError,
		completionReason,
		setCompletionReason,
		unrestricted,
		setUnrestricted,
		isThinking,
		setIsThinking,
		currentMode: null,
		activeSessionId: null,
		persistTimer: null,
		initialized: false,
		registrySubscription: null,
	};
}

function getOrCreate(key: string): PerTerminalConversationState {
	let s = stateMap.get(key);
	if (!s) {
		s = createState();
		stateMap.set(key, s);
	}
	return s;
}

function activeConversation(): PerTerminalConversationState {
	return getOrCreate(activeKey());
}

function setActiveTerminal(key: string): void {
	setActiveKey(key);
}

// ---------------------------------------------------------------------------
// Convenience accessors — proxy through activeConversation()
// ---------------------------------------------------------------------------

function messages(): ConversationMessage[] {
	return activeConversation().messages();
}
function isStreaming(): boolean {
	return activeConversation().isStreaming();
}
function streamingText(): string {
	return activeConversation().streamingText();
}
function error(): string | null {
	return activeConversation().error();
}
function chatId(): string {
	return activeConversation().chatId();
}
function sessionUsage(): ChatUsage | null {
	return activeConversation().sessionUsage();
}
function agentState(): AgentState {
	return activeConversation().agentState();
}
function currentIteration(): number {
	return activeConversation().currentIteration();
}
function toolCalls(): ToolCallEntry[] {
	return activeConversation().toolCalls();
}
function textChunks(): string {
	return activeConversation().textChunks();
}
function reasoningChunks(): string {
	return activeConversation().reasoningChunks();
}
function pendingApproval(): PendingApproval | null {
	return activeConversation().pendingApproval();
}
function agentError(): string | null {
	return activeConversation().agentError();
}
function completionReason(): string | null {
	return activeConversation().completionReason();
}
function unrestricted(): boolean {
	return activeConversation().unrestricted();
}
function setUnrestricted(value: boolean): void {
	activeConversation().setUnrestricted(value);
}
function isThinking(): boolean {
	return activeConversation().isThinking();
}

// ---------------------------------------------------------------------------
// Message management
// ---------------------------------------------------------------------------

function addMessage(role: ConversationMessage["role"], content: string): void {
	const key = activeKey();
	const s = getOrCreate(key);
	s.setMessages((prev) => {
		const msg: ConversationMessage = { role, content, timestamp: Date.now() };
		const next = [...prev, msg];
		return next.length > MAX_MESSAGES ? next.slice(next.length - MAX_MESSAGES) : next;
	});
	schedulePersist(key);
}

function addUserMessage(content: string): void {
	addMessage("user", content);
}
function addAssistantMessage(content: string): void {
	addMessage("assistant", content);
}
function addSystemMessage(content: string): void {
	addMessage("system", content);
}

function accumulateUsage(usage: ChatUsage): void {
	const s = activeConversation();
	s.setSessionUsage((prev) => ({
		promptTokens: (prev?.promptTokens ?? 0) + (usage.promptTokens ?? 0),
		completionTokens: (prev?.completionTokens ?? 0) + (usage.completionTokens ?? 0),
		totalTokens: (prev?.totalTokens ?? 0) + (usage.totalTokens ?? 0),
		cachedTokens: (prev?.cachedTokens ?? 0) + (usage.cachedTokens ?? 0),
		cacheCreationTokens: (prev?.cacheCreationTokens ?? 0) + (usage.cacheCreationTokens ?? 0),
		costUsd: usage.costUsd != null ? (prev?.costUsd ?? 0) + usage.costUsd : prev?.costUsd,
	}));
}

function clearHistory(): void {
	const s = activeConversation();
	if (s.persistTimer) {
		clearTimeout(s.persistTimer);
		s.persistTimer = null;
	}
	batch(() => {
		s.setMessages([]);
		s.setStreamingText("");
		s.setIsStreaming(false);
		s.setError(null);
		s.setSessionUsage(null);
	});
	const oldId = s.chatId();
	void (async () => {
		if (!isTauri()) return;
		try {
			const { invoke } = await import("@tauri-apps/api/core");
			await invoke("delete_conversation", { id: oldId });
			const newId = await invoke<string>("new_conversation_id");
			s.setChatId(newId);
		} catch (e) {
			appLogger.warn("conversation", "clearHistory: backend wipe failed", { error: String(e) });
		}
	})();
}

// ---------------------------------------------------------------------------
// Persistence (debounced autosave + init load)
// ---------------------------------------------------------------------------

function schedulePersist(key?: string): void {
	if (!isTauri()) return;
	const resolvedKey = key ?? activeKey();
	const s = getOrCreate(resolvedKey);
	if (s.persistTimer) clearTimeout(s.persistTimer);
	s.persistTimer = setTimeout(() => {
		s.persistTimer = null;
		void persistNow(resolvedKey);
	}, PERSIST_DEBOUNCE_MS);
}

async function persistNow(key?: string): Promise<void> {
	if (!isTauri()) return;
	const resolvedKey = key ?? activeKey();
	const s = getOrCreate(resolvedKey);
	const msgs = s.messages();
	if (msgs.length === 0) return;
	try {
		const id = s.chatId();
		const now = Date.now();
		const firstUser = msgs.find((m) => m.role === "user");
		const title = firstUser ? firstUser.content.slice(0, 60).replace(/\s+/g, " ").trim() : "New chat";
		const { invoke } = await import("@tauri-apps/api/core");
		let provider: string | undefined;
		let model: string | undefined;
		try {
			const cfg = await invoke<{ provider: string; model: string }>("load_ai_chat_config");
			provider = cfg.provider || undefined;
			model = cfg.model || undefined;
		} catch (e) {
			appLogger.debug("conversation", "load_ai_chat_config unavailable, omitting provider metadata", {
				error: String(e),
			});
		}
		const conv: BackendConversation = {
			meta: {
				id,
				title: title || "New chat",
				session_id: resolvedKey === DEFAULT_KEY ? null : resolvedKey,
				created: msgs[0]?.timestamp ?? now,
				updated: now,
				message_count: msgs.length,
				provider,
				model,
			},
			messages: msgs.map((m) => ({ role: m.role, content: m.content, timestamp: m.timestamp })),
			schema_version: 1,
		};
		await invoke("save_conversation", { conversation: conv });
	} catch (e) {
		appLogger.warn("conversation", "persistNow failed", { error: String(e) });
	}
}

async function initFromDisk(tuicSession?: string): Promise<void> {
	const s = activeConversation();
	if (s.initialized) return;
	s.initialized = true;
	if (!isTauri()) return;
	try {
		const { invoke } = await import("@tauri-apps/api/core");
		if (tuicSession) {
			try {
				const metas = await invoke<BackendConversationMeta[]>("list_conversations");
				const match = metas
					.filter((m) => m.session_id === tuicSession)
					.reduce<BackendConversationMeta | undefined>(
						(best, m) => (!best || m.updated > best.updated ? m : best),
						undefined,
					);
				if (match) {
					const conv = await invoke<BackendConversation>("load_conversation", { id: match.id });
					batch(() => {
						s.setChatId(conv.meta.id);
						s.setMessages(
							conv.messages
								.filter((m) => m.role === "user" || m.role === "assistant" || m.role === "system")
								.map((m) => ({
									role: m.role as ConversationMessage["role"],
									content: m.content ?? "",
									timestamp: m.timestamp,
								}))
								.slice(-MAX_MESSAGES),
						);
						s.setStreamingText("");
						s.setIsStreaming(false);
						s.setError(null);
					});
					return;
				}
			} catch (e) {
				appLogger.info("conversation", "no saved conversation for session, starting new", {
					tuicSession,
					error: String(e),
				});
			}
			const newId = await invoke<string>("new_conversation_id");
			s.setChatId(newId);
			return;
		}
		const newId = await invoke<string>("new_conversation_id");
		s.setChatId(newId);
	} catch (e) {
		appLogger.warn("conversation", "initFromDisk failed", { error: String(e) });
	}
}

// ---------------------------------------------------------------------------
// ConversationEvent handler (Channel-based, used by both chat and agent)
// ---------------------------------------------------------------------------

function applyConversationEvent(s: PerTerminalConversationState, event: ConversationEvent, ownerKey?: string): void {
	const mode = s.currentMode ?? "assisted";
	switch (event.type) {
		case "thinking":
			s.setCurrentIteration(event.iteration);
			s.setAgentState("running");
			s.setIsThinking(true);
			break;

		case "text_chunk":
			s.setIsThinking(false);
			if (mode === "autonomous") {
				s.setTextChunks((prev) => prev + event.text);
			} else {
				s.setStreamingText((prev) => prev + event.text);
			}
			break;

		case "reasoning_chunk":
			// Extended-thinking stream (Opus 4.7+). Accumulate for the disclosure;
			// reset happens at the start of each new user turn (see sendMessage).
			s.setReasoningChunks((prev) => prev + event.text);
			break;

		case "tool_call": {
			s.setIsThinking(false);
			const entry: ToolCallEntry = {
				status: "pending",
				toolName: event.tool_name,
				args: event.args as Record<string, unknown>,
				startedAt: Date.now(),
			};
			s.setToolCalls((prev) => {
				const next = [...prev, entry];
				return next.length > MAX_TOOL_CALLS ? next.slice(next.length - MAX_TOOL_CALLS) : next;
			});
			break;
		}

		case "tool_result":
			s.setToolCalls((prev) => {
				const updated = [...prev];
				for (let i = updated.length - 1; i >= 0; i--) {
					if (updated[i].toolName === event.tool_name && updated[i].status === "pending") {
						updated[i] = {
							...updated[i],
							status: "done",
							result: { success: event.success, output: event.output },
							duration: Date.now() - updated[i].startedAt,
						};
						break;
					}
				}
				return updated;
			});
			break;

		case "needs_approval":
			if (s.activeSessionId) {
				s.setPendingApproval({ sessionId: s.activeSessionId, command: event.command, reason: event.reason });
			} else {
				appLogger.warn("conversation", "needs_approval event dropped — no active session");
			}
			break;

		case "bypassed":
			// Silently skip — bypassed tools run without approval
			break;

		case "paused":
			s.setAgentState("paused");
			s.setIsThinking(false);
			break;

		case "resumed":
			s.setAgentState("running");
			s.setIsThinking(false);
			break;

		case "rate_limited":
			appLogger.info("conversation", `Rate limited, waiting ${event.wait_ms}ms`);
			break;

		case "error":
			batch(() => {
				s.setIsThinking(false);
				if (mode === "autonomous") {
					s.setAgentState("error");
					s.setAgentError(event.message);
				} else {
					s.setIsStreaming(false);
					s.setStreamingText("");
					s.setError(event.message);
				}
			});
			break;

		case "completed": {
			const usage = event.usage;
			s.setIsThinking(false);
			if (usage) {
				appLogger.info("conversation", `usage: input=${usage.input_tokens} output=${usage.output_tokens}`);
				accumulateUsageForState(s, usage);
			}
			if (mode === "autonomous") {
				batch(() => {
					s.setAgentState("completed");
					s.setCompletionReason(event.reason);
				});
			} else {
				const full = s.streamingText();
				batch(() => {
					s.setIsStreaming(false);
					s.setStreamingText("");
					if (full) {
						s.setMessages((prev) => {
							const msg: ConversationMessage = { role: "assistant", content: full, timestamp: Date.now() };
							const next = [...prev, msg];
							return next.length > MAX_MESSAGES ? next.slice(next.length - MAX_MESSAGES) : next;
						});
					}
				});
				schedulePersist(ownerKey ?? activeKey());
			}
			break;
		}
	}
}

function accumulateUsageForState(
	s: PerTerminalConversationState,
	usage: { input_tokens: number; output_tokens: number },
): void {
	s.setSessionUsage((prev) => ({
		promptTokens: (prev?.promptTokens ?? 0) + usage.input_tokens,
		completionTokens: (prev?.completionTokens ?? 0) + usage.output_tokens,
		totalTokens: (prev?.totalTokens ?? 0) + usage.input_tokens + usage.output_tokens,
		cachedTokens: prev?.cachedTokens,
		cacheCreationTokens: prev?.cacheCreationTokens,
		costUsd: prev?.costUsd,
	}));
}

// ---------------------------------------------------------------------------
// Conversation control
// ---------------------------------------------------------------------------

async function sendMessage(text: string, sessionId: string | null): Promise<void> {
	if (!isTauri()) return;
	const s = activeConversation();
	if (s.isStreaming()) return;
	if (!sessionId) {
		s.setError("No terminal attached — focus a terminal first");
		return;
	}

	const capturedKey = activeKey();
	addUserMessage(text);
	batch(() => {
		s.setError(null);
		s.setIsStreaming(true);
		s.setStreamingText("");
		s.setReasoningChunks("");
	});
	s.currentMode = "assisted";
	s.activeSessionId = sessionId;

	try {
		const { invoke, Channel } = await import("@tauri-apps/api/core");
		const onEvent = new Channel<ConversationEvent>();
		onEvent.onmessage = (event) => applyConversationEvent(s, event, capturedKey);
		await invoke("start_conversation", { sessionId, message: text, autonomy: "assisted", onEvent });
	} catch (e) {
		batch(() => {
			s.setIsStreaming(false);
			s.setStreamingText("");
			s.setError(String(e));
		});
		appLogger.warn("conversation", "start_conversation (assisted) failed", { error: String(e) });
	}
}

async function cancelStream(): Promise<void> {
	const s = activeConversation();
	if (!s.isStreaming()) return;
	if (!s.activeSessionId) return;
	try {
		const { invoke } = await import("@tauri-apps/api/core");
		await invoke("cancel_conversation", { sessionId: s.activeSessionId });
	} catch (e) {
		appLogger.warn("conversation", "cancel_conversation failed", { error: String(e) });
	}
}

async function startAgent(sessionId: string, goal: string, isUnrestricted?: boolean): Promise<void> {
	if (!isTauri()) return;
	const s = activeConversation();
	const capturedKey = activeKey();
	if (s.agentState() === "running" || s.agentState() === "paused") return;

	batch(() => {
		s.setAgentState("running");
		s.setToolCalls([]);
		s.setTextChunks("");
		s.setReasoningChunks("");
		s.setAgentError(null);
		s.setCompletionReason(null);
		s.setCurrentIteration(0);
		s.setPendingApproval(null);
		s.setIsThinking(false);
	});
	s.currentMode = "autonomous";
	s.activeSessionId = sessionId;

	const bypassed = isUnrestricted ? ["*"] : [];

	try {
		const { invoke, Channel } = await import("@tauri-apps/api/core");
		const onEvent = new Channel<ConversationEvent>();
		onEvent.onmessage = (event) => applyConversationEvent(s, event, capturedKey);
		await invoke("start_conversation", {
			sessionId,
			message: goal,
			autonomy: "autonomous",
			bypassedTools: bypassed,
			onEvent,
		});
	} catch (e) {
		batch(() => {
			s.setAgentState("error");
			s.setAgentError(String(e));
		});
		appLogger.warn("conversation", "start_conversation (autonomous) failed", { error: String(e) });
	}
}

async function cancelAgent(sessionId: string): Promise<void> {
	if (!isTauri()) return;
	const s = activeConversation();
	try {
		const { invoke } = await import("@tauri-apps/api/core");
		await invoke("cancel_conversation", { sessionId });
		s.setAgentState("cancelled");
	} catch (e) {
		s.setAgentState("error");
		s.setAgentError(String(e));
		appLogger.warn("conversation", "cancel_conversation failed", { error: String(e) });
	}
}

async function pauseAgent(sessionId: string): Promise<void> {
	if (!isTauri()) return;
	const s = activeConversation();
	try {
		const { invoke } = await import("@tauri-apps/api/core");
		await invoke("pause_conversation", { sessionId });
		s.setAgentState("paused");
	} catch (e) {
		s.setAgentState("error");
		s.setAgentError(String(e));
		appLogger.warn("conversation", "pause_conversation failed", { error: String(e) });
	}
}

async function resumeAgent(sessionId: string): Promise<void> {
	if (!isTauri()) return;
	const s = activeConversation();
	try {
		const { invoke } = await import("@tauri-apps/api/core");
		await invoke("resume_conversation", { sessionId });
		s.setAgentState("running");
	} catch (e) {
		s.setAgentState("error");
		s.setAgentError(String(e));
		appLogger.warn("conversation", "resume_conversation failed", { error: String(e) });
	}
}

async function approveAction(sessionId: string, approved: boolean): Promise<void> {
	if (!isTauri()) return;
	const s = activeConversation();
	try {
		const { invoke } = await import("@tauri-apps/api/core");
		await invoke("approve_conversation_action", { sessionId, approved });
		s.setPendingApproval(null);
	} catch (e) {
		s.setAgentState("error");
		s.setAgentError(String(e));
		appLogger.warn("conversation", "approve_conversation_action failed", { error: String(e) });
	}
}

function resetAgent(): void {
	const s = activeConversation();
	batch(() => {
		s.setAgentState("idle");
		s.setToolCalls([]);
		s.setTextChunks("");
		s.setReasoningChunks("");
		s.setAgentError(null);
		s.setCompletionReason(null);
		s.setCurrentIteration(0);
		s.setPendingApproval(null);
		s.setIsThinking(false);
	});
}

// ---------------------------------------------------------------------------
// Legacy event processing (backward compat with old agent-loop-event — removed in 1617)
// ---------------------------------------------------------------------------

function isLegacyAgentEvent(v: unknown): v is LegacyAgentEvent {
	return typeof v === "object" && v !== null && "type" in v && typeof (v as { type: unknown }).type === "string";
}

function processEvent(raw: unknown): void {
	if (!isLegacyAgentEvent(raw)) return;
	const s = activeConversation();
	const event = raw;
	switch (event.type) {
		case "started":
			s.setAgentState("running");
			break;
		case "thinking":
			s.setCurrentIteration(event.iteration);
			break;
		case "text_chunk":
			s.setTextChunks((prev) => prev + event.text);
			break;
		case "reasoning_chunk":
			s.setReasoningChunks((prev) => prev + event.text);
			break;
		case "tool_call": {
			const entry: ToolCallEntry = {
				status: "pending",
				toolName: event.tool_name,
				args: event.args,
				startedAt: Date.now(),
			};
			s.setToolCalls((prev) => {
				const next = [...prev, entry];
				return next.length > MAX_TOOL_CALLS ? next.slice(next.length - MAX_TOOL_CALLS) : next;
			});
			break;
		}
		case "tool_result":
			s.setToolCalls((prev) => {
				const updated = [...prev];
				for (let i = updated.length - 1; i >= 0; i--) {
					if (updated[i].toolName === event.tool_name && updated[i].status === "pending") {
						updated[i] = {
							...updated[i],
							status: "done",
							result: { success: event.success, output: event.output },
							duration: Date.now() - updated[i].startedAt,
						};
						break;
					}
				}
				return updated;
			});
			break;
		case "needs_approval":
			s.setPendingApproval({ sessionId: event.session_id, command: event.command, reason: event.reason });
			break;
		case "paused":
			s.setAgentState("paused");
			break;
		case "resumed":
			s.setAgentState("running");
			break;
		case "rate_limited":
			appLogger.info("conversation", `Rate limited, waiting ${event.wait_ms}ms`);
			break;
		case "error":
			batch(() => {
				s.setAgentState("error");
				s.setAgentError(event.message);
			});
			break;
		case "completed":
			batch(() => {
				s.setAgentState("completed");
				s.setCompletionReason(event.reason);
			});
			break;
	}
}

// ---------------------------------------------------------------------------
// Terminal lifecycle
// ---------------------------------------------------------------------------

async function onTerminalClose(key: string): Promise<void> {
	const s = stateMap.get(key);
	if (!s) return;

	if (s.persistTimer) {
		clearTimeout(s.persistTimer);
		s.persistTimer = null;
	}

	if (s.registrySubscription) {
		await s.registrySubscription.cleanup();
		s.registrySubscription = null;
	}

	if ((s.isStreaming() || s.agentState() === "running") && s.activeSessionId && isTauri()) {
		try {
			const { invoke } = await import("@tauri-apps/api/core");
			await invoke("cancel_conversation", { sessionId: s.activeSessionId });
		} catch (e) {
			appLogger.warn("conversation", "onTerminalClose: cancel_conversation failed", { error: String(e) });
		}
	}

	if (s.messages().length > 0) {
		await persistNow(key);
	}

	stateMap.delete(key);
}

// ---------------------------------------------------------------------------
// Chat ID helpers
// ---------------------------------------------------------------------------

function resetChatId(): void {
	activeConversation().setChatId(generateChatId());
}

function setChatId(id: string): void {
	activeConversation().setChatId(id);
}

function setError(e: string | null): void {
	activeConversation().setError(e);
}

// ---------------------------------------------------------------------------
// Registry subscription (cross-window sync)
// ---------------------------------------------------------------------------

function applyRegistryEvent(s: PerTerminalConversationState, event: RegistryChatEvent): void {
	switch (event.kind) {
		case "snapshot":
			batch(() => {
				s.setMessages(
					event.messages
						.filter((m) => m.role === "user" || m.role === "assistant" || m.role === "system")
						.map((m) => ({ role: m.role as ConversationMessage["role"], content: m.content, timestamp: m.timestamp }))
						.slice(-MAX_MESSAGES),
				);
				s.setIsStreaming(event.isStreaming);
				s.setStreamingText(event.streamingText);
				s.setError(event.error);
			});
			break;
		case "chunk":
			s.setStreamingText((prev) => prev + event.delta);
			break;
		case "error":
			batch(() => {
				s.setIsStreaming(false);
				s.setStreamingText("");
				s.setError(event.message);
			});
			break;
		case "cleared":
			batch(() => {
				s.setMessages([]);
				s.setIsStreaming(false);
				s.setStreamingText("");
				s.setError(null);
			});
			break;
	}
}

async function subscribeToRegistry(targetChatId: string): Promise<void> {
	if (!isTauri()) return;
	const s = activeConversation();

	if (s.registrySubscription) {
		await s.registrySubscription.cleanup();
		s.registrySubscription = null;
	}

	try {
		const { invoke, Channel } = await import("@tauri-apps/api/core");
		const channel = new Channel<RegistryChatEvent>();

		let buffered: RegistryChatEvent[] = [];
		let ready = false;

		channel.onmessage = (event) => {
			if (!ready) {
				buffered.push(event);
			} else {
				applyRegistryEvent(s, event);
			}
		};

		const result = await invoke<{ subscriptionId: number; snapshot: RegistryChatEvent & { kind: "snapshot" } }>(
			"chat_subscribe",
			{ chatId: targetChatId, onEvent: channel },
		);

		applyRegistryEvent(s, result.snapshot);
		ready = true;
		for (const event of buffered) applyRegistryEvent(s, event);
		buffered = [];

		const subId = result.subscriptionId;
		s.registrySubscription = {
			chatId: targetChatId,
			subscriptionId: subId,
			cleanup: async () => {
				ready = false;
				try {
					await invoke("chat_unsubscribe", { chatId: targetChatId, subscriptionId: subId });
				} catch (e) {
					appLogger.warn("conversation", "chat_unsubscribe failed", { error: String(e) });
				}
			},
		};
		appLogger.debug("conversation", `subscribed to registry: chatId=${targetChatId} subId=${subId}`);
	} catch (e) {
		appLogger.warn("conversation", "subscribeToRegistry failed", { error: String(e) });
	}
}

async function unsubscribeFromRegistry(): Promise<void> {
	const s = activeConversation();
	if (s.registrySubscription) {
		await s.registrySubscription.cleanup();
		s.registrySubscription = null;
	}
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

async function listAllConversations(): Promise<ConversationMeta[]> {
	if (!isTauri()) return [];
	try {
		const { invoke } = await import("@tauri-apps/api/core");
		return await invoke<BackendConversationMeta[]>("list_conversations");
	} catch (e) {
		appLogger.warn("conversation", "listAllConversations failed", { error: String(e) });
		return [];
	}
}

async function loadConversation(id: string): Promise<void> {
	if (!isTauri()) return;
	const s = activeConversation();
	try {
		const { invoke } = await import("@tauri-apps/api/core");
		const conv = await invoke<BackendConversation>("load_conversation", { id });
		batch(() => {
			s.setChatId(conv.meta.id);
			s.setMessages(
				conv.messages
					.filter((m) => m.role === "user" || m.role === "assistant" || m.role === "system")
					.map((m) => ({
						role: m.role as ConversationMessage["role"],
						content: m.content ?? "",
						timestamp: m.timestamp,
					}))
					.slice(-MAX_MESSAGES),
			);
			s.setStreamingText("");
			s.setIsStreaming(false);
			s.setError(null);
		});
	} catch (e) {
		appLogger.warn("conversation", "loadConversation failed", { id, error: String(e) });
	}
}

// ---------------------------------------------------------------------------
// Streaming helpers (backward compat shims for tests)
// ---------------------------------------------------------------------------

function setStreaming(v: boolean): void {
	activeConversation().setIsStreaming(v);
}
function appendStreamChunk(text: string): void {
	activeConversation().setStreamingText((prev) => prev + text);
}
function finalizeStream(fullText: string): void {
	const s = activeConversation();
	batch(() => {
		s.setIsStreaming(false);
		s.setStreamingText("");
		addAssistantMessage(fullText);
	});
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export const conversationStore = {
	// Per-terminal API
	activeConversation,
	getOrCreate,
	setActiveTerminal,
	onTerminalClose,

	// Reactive getters (proxy through activeConversation)
	messages,
	isStreaming,
	streamingText,
	error,
	chatId,
	sessionUsage,
	agentState,
	currentIteration,
	toolCalls,
	textChunks,
	reasoningChunks,
	pendingApproval,
	agentError,
	completionReason,
	unrestricted,
	setUnrestricted,
	isThinking,

	// Chat actions
	addUserMessage,
	addAssistantMessage,
	addSystemMessage,
	accumulateUsage,
	clearHistory,
	setStreaming,
	appendStreamChunk,
	finalizeStream,
	sendMessage,
	cancelStream,
	setError,
	resetChatId,
	setChatId,

	// Agent actions
	startAgent,
	cancelAgent,
	pauseAgent,
	resumeAgent,
	approveAction,
	reset: resetAgent,

	// Legacy event processing (removed in 1617)
	processEvent,

	// Persistence
	initFromDisk,
	persistNow,

	// Registry subscription
	subscribeToRegistry,
	unsubscribeFromRegistry,

	// History
	listAllConversations,
	loadConversation,
};
