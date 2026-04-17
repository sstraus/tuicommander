/**
 * AI Chat store — manages chat messages, streaming state, and terminal attachment.
 *
 * Drives the AI Chat panel UI. Invokes `stream_ai_chat` / `cancel_ai_chat`
 * Tauri commands and accumulates chunks via `Channel` IPC.
 */

import { createSignal, batch } from "solid-js";
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

const MAX_MESSAGES = 100;
const ACTIVE_ID_KEY = "ai-chat-active-id";
const PERSIST_DEBOUNCE_MS = 500;

// ---------------------------------------------------------------------------
// Signals
// ---------------------------------------------------------------------------

const [messages, setMessages] = createSignal<AiChatMessage[]>([]);
const [isStreaming, setIsStreaming] = createSignal(false);
const [streamingText, setStreamingText] = createSignal("");
const [error, setError] = createSignal<string | null>(null);
const [attachedSessionId, setAttachedSessionId] = createSignal<string | null>(null);
const [pinned, setPinned] = createSignal(false);
const [chatId, setChatId] = createSignal(generateChatId());

function generateChatId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
}

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
  setMessages((prev) => {
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
  // Cancel any pending debounced persist so it cannot race the wipe below
  // and rewrite the old file under oldId after delete_conversation succeeds.
  if (persistTimer) {
    clearTimeout(persistTimer);
    persistTimer = null;
  }
  batch(() => {
    setMessages([]);
    setStreamingText("");
    setIsStreaming(false);
    setError(null);
  });
  // Wipe disk copy + rotate to a fresh chatId so next message starts a new file.
  const oldId = chatId();
  void (async () => {
    if (!isTauri()) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("delete_conversation", { id: oldId });
      const newId = await invoke<string>("new_conversation_id");
      setChatId(newId);
      writeActiveId(newId);
    } catch (e) {
      appLogger.warn("ai-chat", "clearHistory: backend wipe failed", { error: String(e) });
    }
  })();
}

// ---------------------------------------------------------------------------
// Persistence (debounced autosave + init load)
// ---------------------------------------------------------------------------

let persistTimer: ReturnType<typeof setTimeout> | null = null;
let initialized = false;

function schedulePersist(): void {
  if (!isTauri()) return;
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    persistTimer = null;
    void persistNow();
  }, PERSIST_DEBOUNCE_MS);
}

async function persistNow(): Promise<void> {
  if (!isTauri()) return;
  const msgs = messages();
  if (msgs.length === 0) return; // nothing to persist yet — avoid empty files
  try {
    const id = chatId();
    const now = Date.now();
    // Title = first user message, trimmed. Falls back to "New chat".
    const firstUser = msgs.find((m) => m.role === "user");
    const title = firstUser
      ? firstUser.content.slice(0, 60).replace(/\s+/g, " ").trim()
      : "New chat";
    const conv: BackendConversation = {
      meta: {
        id,
        title: title || "New chat",
        session_id: attachedSessionId(),
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

/** Load active conversation from disk or create a fresh id. Idempotent. */
async function initFromDisk(): Promise<void> {
  if (initialized) return;
  initialized = true;
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const savedId = readActiveId();
    if (savedId) {
      try {
        const conv = await invoke<BackendConversation>("load_conversation", { id: savedId });
        batch(() => {
          setChatId(conv.meta.id);
          setMessages(
            conv.messages
              .filter((m) => m.role === "user" || m.role === "assistant" || m.role === "system")
              .map((m) => ({
                role: m.role as AiChatMessage["role"],
                content: m.content,
                timestamp: m.timestamp,
              }))
              .slice(-MAX_MESSAGES),
          );
          setStreamingText("");
          setIsStreaming(false);
          setError(null);
        });
        return;
      } catch (e) {
        appLogger.info("ai-chat", "saved conversation not found, starting new", { id: savedId, error: String(e) });
      }
    }
    const newId = await invoke<string>("new_conversation_id");
    setChatId(newId);
    writeActiveId(newId);
  } catch (e) {
    appLogger.warn("ai-chat", "initFromDisk failed", { error: String(e) });
  }
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

function setStreaming(v: boolean): void {
  setIsStreaming(v);
}

function appendStreamChunk(text: string): void {
  setStreamingText((prev) => prev + text);
}

function finalizeStream(fullText: string): void {
  batch(() => {
    setIsStreaming(false);
    setStreamingText("");
    addAssistantMessage(fullText);
  });
}

/** Send a message and start streaming the AI response. */
async function sendMessage(text: string): Promise<void> {
  if (!isTauri()) return;
  if (isStreaming()) return;

  const sessionId = attachedSessionId();
  if (!sessionId) {
    setError("No terminal attached — focus a terminal first");
    return;
  }

  addUserMessage(text);
  setError(null);
  setIsStreaming(true);
  setStreamingText("");

  // Build message history for the backend
  const history = messages().map((m) => ({
    role: m.role,
    content: m.content,
  }));

  const currentChatId = chatId();

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
            setIsStreaming(false);
            setStreamingText("");
            setError(msg.data.message);
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
      setIsStreaming(false);
      setStreamingText("");
      setError(String(e));
    });
    appLogger.warn("ai-chat", "stream_ai_chat failed", { error: String(e) });
  }
}

/** Cancel the in-flight stream. */
async function cancelStream(): Promise<void> {
  if (!isStreaming()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("cancel_ai_chat", { chatId: chatId() });
  } catch (e) {
    appLogger.warn("ai-chat", "cancel_ai_chat failed", { error: String(e) });
  }
}

// ---------------------------------------------------------------------------
// Terminal attachment
// ---------------------------------------------------------------------------

function attachTerminal(sessionId: string): void {
  setAttachedSessionId(sessionId);
}

function detachTerminal(): void {
  setAttachedSessionId(null);
}

function autoAttach(sessionId: string): void {
  if (pinned()) return;
  setAttachedSessionId(sessionId);
}

// ---------------------------------------------------------------------------
// Chat ID
// ---------------------------------------------------------------------------

function resetChatId(): void {
  const newId = generateChatId();
  setChatId(newId);
  writeActiveId(newId);
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export const aiChatStore = {
  // Reactive getters
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
