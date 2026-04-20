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

type ChatStreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "end"; data: { fullText: string } }
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
  attachedSessionId: Accessor<string | null>;
  setAttachedSessionId: Setter<string | null>;
  persistTimer: ReturnType<typeof setTimeout> | null;
  initialized: boolean;
}

const MAX_MESSAGES = 100;
const ACTIVE_ID_KEY = "ai-chat-active-id";
const PERSIST_DEBOUNCE_MS = 500;
const DEFAULT_KEY = "__default__";

// ---------------------------------------------------------------------------
// Per-terminal state map
// ---------------------------------------------------------------------------

const chatStateMap = new Map<string, PerTerminalChatState>();
const [activeChatKey, setActiveChatKey] = createSignal<string>(DEFAULT_KEY);

// pinned is global (applies to the attachment behavior, not per-terminal)
const [pinned, setPinned] = createSignal(false);

function generateChatId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
}

function createChatState(): PerTerminalChatState {
  const [messages, setMessages] = createSignal<AiChatMessage[]>([]);
  const [isStreaming, setIsStreaming] = createSignal(false);
  const [streamingText, setStreamingText] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [chatId, setChatId] = createSignal(generateChatId());
  const [attachedSessionId, setAttachedSessionId] = createSignal<string | null>(null);
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
    attachedSessionId,
    setAttachedSessionId,
    persistTimer: null,
    initialized: false,
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
function attachedSessionId(): string | null {
  return activeChat().attachedSessionId();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function readActiveId(): string | null {
  try {
    return globalThis.localStorage?.getItem(ACTIVE_ID_KEY) ?? null;
  } catch {
    return null;
  }
}

function writeActiveId(id: string | null): void {
  try {
    if (id) globalThis.localStorage?.setItem(ACTIVE_ID_KEY, id);
    else globalThis.localStorage?.removeItem(ACTIVE_ID_KEY);
  } catch {
    // localStorage unavailable (SSR/tests) — best effort only
  }
}

// ---------------------------------------------------------------------------
// Message management
// ---------------------------------------------------------------------------

function addMessage(role: AiChatMessage["role"], content: string): void {
  const s = activeChat();
  s.setMessages((prev) => {
    const msg: AiChatMessage = { role, content, timestamp: Date.now() };
    const next = [...prev, msg];
    if (next.length > MAX_MESSAGES) {
      return next.slice(next.length - MAX_MESSAGES);
    }
    return next;
  });
  schedulePersist();
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
  });
  const oldId = s.chatId();
  void (async () => {
    if (!isTauri()) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("delete_conversation", { id: oldId });
      const newId = await invoke<string>("new_conversation_id");
      s.setChatId(newId);
      writeActiveId(newId);
    } catch (e) {
      appLogger.warn("ai-chat", "clearHistory: backend wipe failed", { error: String(e) });
    }
  })();
}

// ---------------------------------------------------------------------------
// Persistence (debounced autosave + init load)
// ---------------------------------------------------------------------------

function schedulePersist(): void {
  if (!isTauri()) return;
  const s = activeChat();
  if (s.persistTimer) clearTimeout(s.persistTimer);
  s.persistTimer = setTimeout(() => {
    s.persistTimer = null;
    void persistNow();
  }, PERSIST_DEBOUNCE_MS);
}

async function persistNow(): Promise<void> {
  if (!isTauri()) return;
  const s = activeChat();
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
        session_id: s.attachedSessionId(),
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

/** Load active conversation from disk or create a fresh id. Idempotent per terminal. */
async function initFromDisk(): Promise<void> {
  const s = activeChat();
  if (s.initialized) return;
  s.initialized = true;
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const savedId = readActiveId();
    if (savedId) {
      try {
        const conv = await invoke<BackendConversation>("load_conversation", { id: savedId });
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
      } catch (e) {
        appLogger.info("ai-chat", "saved conversation not found, starting new", { id: savedId, error: String(e) });
      }
    }
    const newId = await invoke<string>("new_conversation_id");
    s.setChatId(newId);
    writeActiveId(newId);
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

/** Send a message and start streaming the AI response. */
async function sendMessage(text: string): Promise<void> {
  if (!isTauri()) return;
  const s = activeChat();
  if (s.isStreaming()) return;

  const sessionId = s.attachedSessionId();
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
      switch (msg.event) {
        case "chunk":
          appendStreamChunk(msg.data.text);
          break;
        case "end":
          finalizeStream(msg.data.fullText);
          break;
        case "error":
          batch(() => {
            s.setIsStreaming(false);
            s.setStreamingText("");
            s.setError(msg.data.message);
          });
          break;
      }
    };

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
// Terminal attachment
// ---------------------------------------------------------------------------

function attachTerminal(sessionId: string): void {
  activeChat().setAttachedSessionId(sessionId);
}

function detachTerminal(): void {
  activeChat().setAttachedSessionId(null);
}

function autoAttach(sessionId: string): void {
  if (pinned()) return;
  activeChat().setAttachedSessionId(sessionId);
}

// ---------------------------------------------------------------------------
// Chat ID
// ---------------------------------------------------------------------------

function resetChatId(): void {
  const newId = generateChatId();
  activeChat().setChatId(newId);
  writeActiveId(newId);
}

function setError(e: string | null): void {
  activeChat().setError(e);
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export const aiChatStore = {
  // Per-terminal API (new in 1406-c679)
  activeChat,
  getOrCreate,
  setActiveTerminal,

  // Reactive getters (proxy through activeChat)
  messages,
  isStreaming,
  streamingText,
  error,
  attachedSessionId,
  pinned,
  chatId,

  // Actions
  addUserMessage,
  addAssistantMessage,
  addSystemMessage,
  clearHistory,
  setStreaming,
  appendStreamChunk,
  finalizeStream,
  sendMessage,
  cancelStream,
  attachTerminal,
  detachTerminal,
  autoAttach,
  setPinned,
  setError,
  resetChatId,

  // Persistence
  initFromDisk,
  persistNow,
};
