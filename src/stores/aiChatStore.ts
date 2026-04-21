/**
 * AI Chat store — manages chat messages, streaming state, and terminal attachment.
 *
 * Per-terminal architecture: each terminal tab gets its own PerTerminalChatState
 * keyed by tuicSession. The top-level exports (messages, isStreaming, etc.) proxy
 * through activeChat() so all existing callers remain unchanged.
 *
 * Drives the AI Chat panel UI. Invokes `stream_ai_chat` / `cancel_ai_chat`
 * Tauri commands and accumulates chunks via `Channel` IPC.
 */

import { createSignal, batch } from "solid-js";
import type { Accessor, Setter } from "solid-js";
import { isTauri } from "../transport";
import { appLogger } from "./appLogger";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface AiChatMessage {
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
}

interface ChatUsage {
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
  cachedTokens?: number;
  cacheCreationTokens?: number;
  costUsd?: number;
}

type ChatStreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "end"; data: { fullText: string; usage?: ChatUsage } }
  | { event: "error"; data: { message: string } };

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

/** Events from the Rust ChatRegistry, received via Channel subscription. */
type RegistryChatEvent =
  | { kind: "snapshot"; messages: RegistryMessage[]; isStreaming: boolean; streamingText: string; error: string | null; attachedSessionId: string | null; pinned: boolean }
  | { kind: "chunk"; delta: string }
  | { kind: "error"; message: string }
  | { kind: "cleared" };

interface RegistryMessage {
  role: string;
  content: string;
  timestamp: number;
}

/** Tracks an active registry subscription for cross-window sync. */
interface RegistrySubscription {
  chatId: string;
  subscriptionId: number;
  cleanup: () => Promise<void>;
}

/** Reactive state for a single terminal's chat session. */
export interface PerTerminalChatState {
  messages: Accessor<AiChatMessage[]>;
  setMessages: Setter<AiChatMessage[]>;
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
  persistTimer: ReturnType<typeof setTimeout> | null;
  initialized: boolean;
  registrySubscription: RegistrySubscription | null;
}

const MAX_MESSAGES = 100;
const PERSIST_DEBOUNCE_MS = 500;
const DEFAULT_KEY = "__default__";

// ---------------------------------------------------------------------------
// Per-terminal state map
// ---------------------------------------------------------------------------

const chatStateMap = new Map<string, PerTerminalChatState>();
const [activeChatKey, setActiveChatKey] = createSignal<string>(DEFAULT_KEY);

function generateChatId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
}

function createChatState(): PerTerminalChatState {
  const [messages, setMessages] = createSignal<AiChatMessage[]>([]);
  const [isStreaming, setIsStreaming] = createSignal(false);
  const [streamingText, setStreamingText] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [chatId, setChatId] = createSignal(generateChatId());
  const [sessionUsage, setSessionUsage] = createSignal<ChatUsage | null>(null);
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
    persistTimer: null,
    initialized: false,
    registrySubscription: null,
  };
}

function getOrCreate(key: string): PerTerminalChatState {
  let state = chatStateMap.get(key);
  if (!state) {
    state = createChatState();
    chatStateMap.set(key, state);
  }
  return state;
}

function activeChat(): PerTerminalChatState {
  return getOrCreate(activeChatKey());
}

/** Switch the active terminal. Steps 5+ will resolve terminalId → tuicSession. */
function setActiveTerminal(key: string): void {
  setActiveChatKey(key);
}

// ---------------------------------------------------------------------------
// Convenience accessors — proxy through activeChat() for reactivity
// ---------------------------------------------------------------------------

function messages(): AiChatMessage[] {
  return activeChat().messages();
}
function isStreaming(): boolean {
  return activeChat().isStreaming();
}
function streamingText(): string {
  return activeChat().streamingText();
}
function error(): string | null {
  return activeChat().error();
}
function chatId(): string {
  return activeChat().chatId();
}
function sessionUsage(): ChatUsage | null {
  return activeChat().sessionUsage();
}

// ---------------------------------------------------------------------------
// Message management
// ---------------------------------------------------------------------------

function addMessage(role: AiChatMessage["role"], content: string): void {
  const key = activeChatKey();
  const s = getOrCreate(key);
  s.setMessages((prev) => {
    const msg: AiChatMessage = { role, content, timestamp: Date.now() };
    const next = [...prev, msg];
    if (next.length > MAX_MESSAGES) {
      return next.slice(next.length - MAX_MESSAGES);
    }
    return next;
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
  const s = activeChat();
  s.setSessionUsage((prev) => ({
    promptTokens: (prev?.promptTokens ?? 0) + (usage.promptTokens ?? 0),
    completionTokens: (prev?.completionTokens ?? 0) + (usage.completionTokens ?? 0),
    totalTokens: (prev?.totalTokens ?? 0) + (usage.totalTokens ?? 0),
    cachedTokens: (prev?.cachedTokens ?? 0) + (usage.cachedTokens ?? 0),
    cacheCreationTokens: (prev?.cacheCreationTokens ?? 0) + (usage.cacheCreationTokens ?? 0),
    costUsd:
      usage.costUsd != null
        ? (prev?.costUsd ?? 0) + usage.costUsd
        : prev?.costUsd,
  }));
}

function clearHistory(): void {
  const s = activeChat();
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
      appLogger.warn("ai-chat", "clearHistory: backend wipe failed", { error: String(e) });
    }
  })();
}

// ---------------------------------------------------------------------------
// Persistence (debounced autosave + init load)
// ---------------------------------------------------------------------------

function schedulePersist(key?: string): void {
  if (!isTauri()) return;
  const resolvedKey = key ?? activeChatKey();
  const s = getOrCreate(resolvedKey);
  if (s.persistTimer) clearTimeout(s.persistTimer);
  s.persistTimer = setTimeout(() => {
    s.persistTimer = null;
    void persistNow(resolvedKey);
  }, PERSIST_DEBOUNCE_MS);
}

async function persistNow(key?: string): Promise<void> {
  if (!isTauri()) return;
  const resolvedKey = key ?? activeChatKey();
  const s = getOrCreate(resolvedKey);
  const msgs = s.messages();
  if (msgs.length === 0) return;
  try {
    const id = s.chatId();
    const now = Date.now();
    const firstUser = msgs.find((m) => m.role === "user");
    const title = firstUser
      ? firstUser.content.slice(0, 60).replace(/\s+/g, " ").trim()
      : "New chat";
    const conv: BackendConversation = {
      meta: {
        id,
        title: title || "New chat",
        session_id: resolvedKey === DEFAULT_KEY ? null : resolvedKey,
        created: msgs[0]?.timestamp ?? now,
        updated: now,
        message_count: msgs.length,
      },
      messages: msgs.map((m) => ({
        role: m.role,
        content: m.content,
        timestamp: m.timestamp,
      })),
      schema_version: 1,
    };
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("save_conversation", { conversation: conv });
  } catch (e) {
    appLogger.warn("ai-chat", "persistNow failed", { error: String(e) });
  }
}

/** Load active conversation from disk or create a fresh id. Idempotent per terminal.
 *  With tuicSession: filters list_conversations by session_id, loads most recent match.
 *  Without tuicSession: legacy path — uses ACTIVE_ID_KEY from localStorage. */
async function initFromDisk(tuicSession?: string): Promise<void> {
  const s = activeChat();
  if (s.initialized) return;
  s.initialized = true;
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");

    if (tuicSession) {
      // Per-terminal path: find most recent conversation for this session
      try {
        const metas = await invoke<BackendConversationMeta[]>("list_conversations");
        const matches = metas.filter((m) => m.session_id === tuicSession);
        const match = matches.reduce<BackendConversationMeta | undefined>(
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
                  role: m.role as AiChatMessage["role"],
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
        appLogger.info("ai-chat", "no saved conversation for session, starting new", { tuicSession, error: String(e) });
      }
      const newId = await invoke<string>("new_conversation_id");
      s.setChatId(newId);
      return;
    }

    const newId = await invoke<string>("new_conversation_id");
    s.setChatId(newId);
  } catch (e) {
    appLogger.warn("ai-chat", "initFromDisk failed", { error: String(e) });
  }
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

function setStreaming(v: boolean): void {
  activeChat().setIsStreaming(v);
}

function appendStreamChunk(text: string): void {
  activeChat().setStreamingText((prev) => prev + text);
}

function finalizeStream(fullText: string): void {
  const s = activeChat();
  batch(() => {
    s.setIsStreaming(false);
    s.setStreamingText("");
    addAssistantMessage(fullText);
  });
}

/** Send a message and start streaming the AI response.
 *  `sessionId` is the PTY session id (from `terminalsStore.get(activeId)?.sessionId`),
 *  which the Rust backend uses to key into `AppState.sessions`. Distinct from the
 *  chat key (tuicSession) — that's stable across PTY respawns and keys conversation
 *  history. Caller supplies sessionId reactively so we don't duplicate state here. */
async function sendMessage(text: string, sessionId: string | null): Promise<void> {
  if (!isTauri()) return;
  const s = activeChat();
  if (s.isStreaming()) return;

  if (!sessionId) {
    s.setError("No terminal attached — focus a terminal first");
    return;
  }

  addUserMessage(text);
  s.setError(null);
  s.setIsStreaming(true);
  s.setStreamingText("");

  const history = s.messages().map((m) => ({
    role: m.role,
    content: m.content,
  }));

  const currentChatId = s.chatId();

  try {
    const { invoke, Channel } = await import("@tauri-apps/api/core");
    const onEvent = new Channel<ChatStreamEvent>();
    onEvent.onmessage = (msg: ChatStreamEvent) => {
      // Use captured `s` — not activeChat() — so callbacks always route to
      // the terminal that initiated this stream, regardless of tab switches.
      switch (msg.event) {
        case "chunk":
          s.setStreamingText((prev) => prev + msg.data.text);
          break;
        case "end": {
          const usage = msg.data.usage;
          if (usage) {
            const parts: string[] = [];
            if (usage.promptTokens != null) parts.push(`prompt=${usage.promptTokens}`);
            if (usage.completionTokens != null) parts.push(`completion=${usage.completionTokens}`);
            if (usage.cachedTokens != null) parts.push(`cached=${usage.cachedTokens}`);
            if (usage.cacheCreationTokens != null) parts.push(`cache_created=${usage.cacheCreationTokens}`);
            if (usage.costUsd != null) parts.push(`cost=$${usage.costUsd.toFixed(4)}`);
            appLogger.info("ai-chat", `usage: ${parts.join(", ")}`);
            accumulateUsage(usage);
          }
          batch(() => {
            s.setIsStreaming(false);
            s.setStreamingText("");
            s.setMessages((prev) => {
              const msg2: AiChatMessage = { role: "assistant", content: msg.data.fullText, timestamp: Date.now() };
              const next = [...prev, msg2];
              return next.length > MAX_MESSAGES ? next.slice(next.length - MAX_MESSAGES) : next;
            });
          });
          schedulePersist(activeChatKey());
          break;
        }
        case "error":
          batch(() => {
            s.setIsStreaming(false);
            s.setStreamingText("");
            s.setError(msg.data.message);
          });
          break;
      }
    };

    // Push user message to registry so other windows see it
    invoke("chat_push_message", { chatId: currentChatId, role: "user", content: text }).catch((e: unknown) =>
      appLogger.warn("ai-chat", "chat_push_message failed", { error: String(e) }),
    );

    await invoke("stream_ai_chat", {
      sessionId,
      messages: history,
      chatId: currentChatId,
      onEvent,
    });
  } catch (e) {
    batch(() => {
      s.setIsStreaming(false);
      s.setStreamingText("");
      s.setError(String(e));
    });
    appLogger.warn("ai-chat", "stream_ai_chat failed", { error: String(e) });
  }
}

/** Called when a terminal tab is closed. Cancels stream, persists partial, frees memory. */
async function onTerminalClose(key: string): Promise<void> {
  const s = chatStateMap.get(key);
  if (!s) return;

  // Cancel persist debounce timer — we'll persist synchronously below if needed
  if (s.persistTimer) {
    clearTimeout(s.persistTimer);
    s.persistTimer = null;
  }

  // Unsubscribe from registry
  if (s.registrySubscription) {
    await s.registrySubscription.cleanup();
    s.registrySubscription = null;
  }

  // Cancel in-flight stream
  if (s.isStreaming() && isTauri()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("cancel_ai_chat", { chatId: s.chatId() });
    } catch (e) {
      appLogger.warn("ai-chat", "onTerminalClose: cancel_ai_chat failed", { error: String(e) });
    }
  }

  // Persist partial state before freeing (persistNow while key is still in map)
  if (s.messages().length > 0) {
    await persistNow(key);
  }

  // Free in-memory state
  chatStateMap.delete(key);
  // DEFERRED (2026-04-20) — tuicSession null window: if a message was queued before
  // tuicSession was assigned, it may be lost here. Needs a reliable repro; wire in Step 5.
}

/** Cancel the in-flight stream. */
async function cancelStream(): Promise<void> {
  const s = activeChat();
  if (!s.isStreaming()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("cancel_ai_chat", { chatId: s.chatId() });
  } catch (e) {
    appLogger.warn("ai-chat", "cancel_ai_chat failed", { error: String(e) });
  }
}

// ---------------------------------------------------------------------------
// Chat ID
// ---------------------------------------------------------------------------

function resetChatId(): void {
  const newId = generateChatId();
  activeChat().setChatId(newId);
}

function setChatId(id: string): void {
  activeChat().setChatId(id);
}

function setError(e: string | null): void {
  activeChat().setError(e);
}

// ---------------------------------------------------------------------------
// Registry subscription (cross-window sync)
// ---------------------------------------------------------------------------

function applyRegistryEvent(s: PerTerminalChatState, event: RegistryChatEvent): void {
  switch (event.kind) {
    case "snapshot":
      batch(() => {
        s.setMessages(
          event.messages
            .filter((m) => m.role === "user" || m.role === "assistant" || m.role === "system")
            .map((m) => ({
              role: m.role as AiChatMessage["role"],
              content: m.content,
              timestamp: m.timestamp,
            }))
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

/** Subscribe to the Rust ChatRegistry for a given chatId.
 *  Uses the buffering pattern from the plan to avoid snapshot-vs-chunk race. */
async function subscribeToRegistry(targetChatId: string): Promise<void> {
  if (!isTauri()) return;
  const s = activeChat();

  // Unsubscribe existing if any
  if (s.registrySubscription) {
    await s.registrySubscription.cleanup();
    s.registrySubscription = null;
  }

  try {
    const { invoke, Channel } = await import("@tauri-apps/api/core");
    const channel = new Channel<RegistryChatEvent>();

    // Buffering: events arriving before snapshot is applied are queued
    let buffered: RegistryChatEvent[] = [];
    let ready = false;

    channel.onmessage = (event: RegistryChatEvent) => {
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

    // Apply snapshot synchronously
    applyRegistryEvent(s, { kind: "snapshot", ...result.snapshot });

    // Flush buffered events in order
    ready = true;
    for (const event of buffered) {
      applyRegistryEvent(s, event);
    }
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
          appLogger.warn("ai-chat", "chat_unsubscribe failed", { error: String(e) });
        }
      },
    };
    appLogger.info("ai-chat", `subscribed to registry: chatId=${targetChatId} subId=${subId}`);
  } catch (e) {
    appLogger.warn("ai-chat", "subscribeToRegistry failed", { error: String(e) });
  }
}

/** Unsubscribe the active chat from the registry. */
async function unsubscribeFromRegistry(): Promise<void> {
  const s = activeChat();
  if (s.registrySubscription) {
    await s.registrySubscription.cleanup();
    s.registrySubscription = null;
  }
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

export type ConversationMeta = BackendConversationMeta;

async function listAllConversations(): Promise<ConversationMeta[]> {
  if (!isTauri()) return [];
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<BackendConversationMeta[]>("list_conversations");
  } catch (e) {
    appLogger.warn("ai-chat", "listAllConversations failed", { error: String(e) });
    return [];
  }
}

async function loadConversation(id: string): Promise<void> {
  if (!isTauri()) return;
  const s = activeChat();
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const conv = await invoke<BackendConversation>("load_conversation", { id });
    batch(() => {
      s.setChatId(conv.meta.id);
      s.setMessages(
        conv.messages
          .filter((m) => m.role === "user" || m.role === "assistant" || m.role === "system")
          .map((m) => ({
            role: m.role as AiChatMessage["role"],
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
    appLogger.warn("ai-chat", "loadConversation failed", { id, error: String(e) });
  }
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export const aiChatStore = {
  // Per-terminal API (new in 1406-c679)
  activeChat,
  getOrCreate,
  setActiveTerminal,
  onTerminalClose,

  // Reactive getters (proxy through activeChat)
  messages,
  isStreaming,
  streamingText,
  error,
  chatId,
  sessionUsage,

  // Actions
  addUserMessage,
  accumulateUsage,
  addAssistantMessage,
  addSystemMessage,
  clearHistory,
  setStreaming,
  appendStreamChunk,
  finalizeStream,
  sendMessage,
  cancelStream,
  setError,
  resetChatId,
  setChatId,

  // Persistence
  initFromDisk,
  persistNow,

  // Registry subscription (cross-window sync)
  subscribeToRegistry,
  unsubscribeFromRegistry,

  // History
  listAllConversations,
  loadConversation,
};
