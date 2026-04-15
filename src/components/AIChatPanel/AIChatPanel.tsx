import { Component, For, Show, createSignal, createEffect } from "solid-js";
import { aiChatStore } from "../../stores/aiChatStore";
import { terminalsStore } from "../../stores/terminals";
import { MarkdownRenderer } from "../ui/MarkdownRenderer";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { sendCommand, getShellFamily } from "../../utils/sendCommand";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import p from "../shared/panel.module.css";
import s from "./AIChatPanel.module.css";

export interface AIChatPanelProps {
  visible: boolean;
  onClose: () => void;
}

/** Copy text to clipboard, return true on success */
async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** Extract code text from a <pre><code> element */
function extractCodeText(pre: HTMLPreElement): string {
  const code = pre.querySelector("code");
  return (code ?? pre).textContent ?? "";
}

// ── Inline SVG icons (monochrome, fill=currentColor) ─────────────────────

const IconSend = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 2.5l-4.5 4.5h3v5h3v-5h3z" />
  </svg>
);

const IconStop = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
    <rect x="2" y="2" width="10" height="10" rx="1" />
  </svg>
);

const IconTrash = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
    <path d="M2.5 4h9M5 4V2.5h4V4M3.5 4v7.5a1 1 0 001 1h5a1 1 0 001-1V4" />
    <path d="M5.5 6.5v3M8.5 6.5v3" />
  </svg>
);

const IconPin = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
    <path d="M7 1.5v5M4.5 6.5h5l-1 4h-3z" />
    <path d="M7 10.5v2" />
  </svg>
);

export const AIChatPanel: Component<AIChatPanelProps> = (props) => {
  const [inputText, setInputText] = createSignal("");
  let messageListRef: HTMLDivElement | undefined;
  let textareaRef: HTMLTextAreaElement | undefined;

  // ── Auto-scroll on new messages / streaming chunks ──────────────────────
  createEffect(() => {
    // Subscribe to reactive dependencies
    aiChatStore.streamingText();
    aiChatStore.messages().length;
    if (messageListRef) {
      messageListRef.scrollTop = messageListRef.scrollHeight;
    }
  });

  // ── Auto-resize textarea ───────────────────────────────────────────────
  const autoResize = () => {
    if (!textareaRef) return;
    textareaRef.style.height = "auto";
    textareaRef.style.height = `${Math.min(textareaRef.scrollHeight, 150)}px`;
  };

  // ── Send message ───────────────────────────────────────────────────────
  const handleSend = () => {
    const text = inputText().trim();
    if (!text || aiChatStore.isStreaming()) return;
    aiChatStore.sendMessage(text);
    setInputText("");
    if (textareaRef) {
      textareaRef.style.height = "auto";
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    // Cmd+Enter (Mac) or Ctrl+Enter sends
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSend();
      return;
    }
    // Shift+Enter = newline (default behavior)
    // Plain Enter = newline (default behavior)
  };

  // ── Terminal list for dropdown ──────────────────────────────────────────
  const terminalList = () => {
    const ids = terminalsStore.getIds();
    return ids.map((id) => {
      const t = terminalsStore.get(id);
      return {
        id,
        sessionId: t?.sessionId ?? null,
        name: t?.name ?? id,
      };
    });
  };

  const handleTerminalChange = (e: Event) => {
    const value = (e.target as HTMLSelectElement).value;
    if (value === "__none__") {
      aiChatStore.detachTerminal();
    } else {
      aiChatStore.attachTerminal(value);
    }
  };

  // ── Run code in attached terminal ──────────────────────────────────────
  const runCodeInTerminal = async (code: string) => {
    const sessionId = aiChatStore.attachedSessionId();
    if (!sessionId) {
      appLogger.warn("ai-chat", "Cannot run code: no terminal attached");
      return;
    }
    // Find the terminal ref for this session
    const ids = terminalsStore.getIds();
    let termRef: { write: (data: string) => void } | undefined;
    let agentType: string | null = null;
    for (const id of ids) {
      const t = terminalsStore.get(id);
      if (t?.sessionId === sessionId && t.ref) {
        termRef = t.ref;
        agentType = t.agentType ?? null;
        break;
      }
    }
    if (!termRef) {
      appLogger.warn("ai-chat", "Cannot run code: terminal ref not found", { sessionId });
      return;
    }
    const shellFamily = await getShellFamily(sessionId);
    // Send each line as a separate command for multi-line code blocks
    const lines = code.trim().split("\n");
    for (const line of lines) {
      await sendCommand(
        (data: string) => { termRef!.write(data); return Promise.resolve(); },
        line,
        agentType,
        shellFamily,
      );
    }
  };

  // ── Code block enhancement: inject Copy + Run buttons ──────────────────
  const enhanceCodeBlocks = (container: HTMLDivElement) => {
    const pres = container.querySelectorAll("pre");
    for (const pre of pres) {
      // Skip if already enhanced
      if (pre.parentElement?.classList.contains(s.codeBlockWrapper)) continue;

      const wrapper = document.createElement("div");
      wrapper.className = s.codeBlockWrapper;
      pre.parentElement!.insertBefore(wrapper, pre);
      wrapper.appendChild(pre);

      const actions = document.createElement("div");
      actions.className = s.codeBlockActions;

      // Copy button
      const copyBtn = document.createElement("button");
      copyBtn.className = s.codeActionBtn;
      copyBtn.title = "Copy code";
      copyBtn.innerHTML = `<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2"><rect x="4" y="4" width="7" height="7" rx="1"/><path d="M3 10V3h7"/></svg>`;
      copyBtn.addEventListener("click", async () => {
        const text = extractCodeText(pre);
        const ok = await copyToClipboard(text);
        if (ok) {
          copyBtn.innerHTML = `<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 7l3 3 5-5"/></svg>`;
          copyBtn.classList.add(s.codeActionBtnCopied);
          setTimeout(() => {
            copyBtn.innerHTML = `<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2"><rect x="4" y="4" width="7" height="7" rx="1"/><path d="M3 10V3h7"/></svg>`;
            copyBtn.classList.remove(s.codeActionBtnCopied);
          }, 1500);
        }
      });
      actions.appendChild(copyBtn);

      // Run button
      const runBtn = document.createElement("button");
      runBtn.className = s.codeActionBtn;
      runBtn.title = "Run in terminal";
      runBtn.innerHTML = `<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor"><path d="M4 2.5l8 4.5-8 4.5z"/></svg>`;
      runBtn.addEventListener("click", () => {
        const text = extractCodeText(pre);
        runCodeInTerminal(text);
      });
      actions.appendChild(runBtn);

      wrapper.appendChild(actions);
    }
  };

  // ── Retry last message on error ────────────────────────────────────────
  const handleRetry = () => {
    const msgs = aiChatStore.messages();
    const lastUser = [...msgs].reverse().find((m) => m.role === "user");
    if (lastUser) {
      aiChatStore.setError(null);
      aiChatStore.sendMessage(lastUser.content);
    }
  };

  return (
    <div id="ai-chat-panel" class={cx(s.panel, !props.visible && s.hidden)}>
      <PanelResizeHandle panelId="ai-chat-panel" minWidth={300} maxWidth={700} />

      {/* ── Header ──────────────────────────────────────────── */}
      <div class={p.header}>
        <div class={p.headerLeft}>
          <span class={p.title}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" style={{ "vertical-align": "-2px", "margin-right": "4px" }}>
              <path d="M2 2.5A1.5 1.5 0 013.5 1h7A1.5 1.5 0 0112 2.5v6A1.5 1.5 0 0110.5 10H5l-3 2.5V10A1.5 1.5 0 010.5 8.5v-6z" transform="translate(1 0.5)" />
            </svg>
            AI Chat
          </span>
          <div class={s.attachIndicator}>
            <span class={cx(s.statusDot, aiChatStore.attachedSessionId() ? s.statusDotAttached : s.statusDotDetached)} />
            <select
              class={s.terminalSelect}
              value={aiChatStore.attachedSessionId() ?? "__none__"}
              onChange={handleTerminalChange}
            >
              <option value="__none__">No terminal</option>
              <For each={terminalList()}>
                {(term) => (
                  <Show when={term.sessionId}>
                    <option value={term.sessionId!}>{term.name}</option>
                  </Show>
                )}
              </For>
            </select>
          </div>
        </div>
        <div class={s.headerActions}>
          <button
            class={cx(s.headerBtn, aiChatStore.pinned() && s.headerBtnActive)}
            onClick={() => aiChatStore.setPinned(!aiChatStore.pinned())}
            title={aiChatStore.pinned() ? "Unpin (allow auto-attach)" : "Pin terminal (prevent auto-attach)"}
          >
            <IconPin />
          </button>
          <button
            class={s.headerBtn}
            onClick={() => aiChatStore.clearHistory()}
            title="Clear conversation"
          >
            <IconTrash />
          </button>
          <button class={p.close} onClick={props.onClose} title="Close">
            &times;
          </button>
        </div>
      </div>

      {/* ── Error banner ────────────────────────────────────── */}
      <Show when={aiChatStore.error()}>
        <div class={s.errorBanner}>
          <span class={s.errorText}>{aiChatStore.error()}</span>
          <button class={s.retryBtn} onClick={handleRetry}>Retry</button>
        </div>
      </Show>

      {/* ── Message list ────────────────────────────────────── */}
      <div class={s.messageList} ref={messageListRef}>
        <Show
          when={aiChatStore.messages().length > 0 || aiChatStore.isStreaming()}
          fallback={
            <div class={s.emptyState}>Ask me about your terminal output</div>
          }
        >
          <For each={aiChatStore.messages()}>
            {(msg) => (
              <Show
                when={msg.role === "user"}
                fallback={
                  <div
                    class={s.assistantMsg}
                    ref={(el) => {
                      // Enhance code blocks after the markdown renders
                      requestAnimationFrame(() => enhanceCodeBlocks(el));
                    }}
                  >
                    <MarkdownRenderer content={msg.content} />
                  </div>
                }
              >
                <div class={s.userMsg}>{msg.content}</div>
              </Show>
            )}
          </For>

          {/* Streaming text: raw <pre>, NOT markdown */}
          <Show when={aiChatStore.isStreaming() && aiChatStore.streamingText()}>
            <pre class={s.streamingMsg}>{aiChatStore.streamingText()}</pre>
          </Show>
        </Show>
      </div>

      {/* ── Input area ──────────────────────────────────────── */}
      <div class={s.inputArea}>
        <textarea
          ref={textareaRef}
          class={s.textarea}
          rows={1}
          placeholder={aiChatStore.attachedSessionId()
            ? "Ask about your terminal... (Cmd+Enter to send)"
            : "Focus a terminal first..."}
          value={inputText()}
          onInput={(e) => {
            setInputText(e.currentTarget.value);
            autoResize();
          }}
          onKeyDown={handleKeyDown}
          disabled={!aiChatStore.attachedSessionId()}
        />
        <Show
          when={aiChatStore.isStreaming()}
          fallback={
            <button
              class={s.sendBtn}
              onClick={handleSend}
              disabled={!inputText().trim() || aiChatStore.isStreaming() || !aiChatStore.attachedSessionId()}
              title="Send (Cmd+Enter)"
            >
              <IconSend />
            </button>
          }
        >
          <button
            class={s.stopBtn}
            onClick={() => aiChatStore.cancelStream()}
            title="Stop generating"
          >
            <IconStop />
          </button>
        </Show>
      </div>
    </div>
  );
};

export default AIChatPanel;
