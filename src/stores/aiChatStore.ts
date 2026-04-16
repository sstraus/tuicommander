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

const MAX_MESSAGES = 100;

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
  batch(() => {
    setMessages([]);
    setStreamingText("");
    setIsStreaming(false);
    setError(null);
  });
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
  setChatId(generateChatId());
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
};
