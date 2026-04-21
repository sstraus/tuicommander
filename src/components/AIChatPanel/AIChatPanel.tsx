import { Component, For, Show, createSignal, createEffect, createMemo, onCleanup } from "solid-js";
import { aiChatStore, type ConversationMeta } from "../../stores/aiChatStore";
import { aiAgentStore, type ToolCallEntry } from "../../stores/aiAgentStore";
import { terminalsStore } from "../../stores/terminals";
import { ContentRenderer } from "../ui/ContentRenderer";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { sendCommand, getShellFamily } from "../../utils/sendCommand";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import p from "../shared/panel.module.css";
import s from "./AIChatPanel.module.css";
import { SessionKnowledgeBar } from "./SessionKnowledgeBar";

/** Session ID of the currently active terminal tab (null when no terminal is focused). */
function useActiveSessionId() {
  return createMemo(() => {
    const id = terminalsStore.state.activeId;
    return id ? (terminalsStore.get(id)?.sessionId ?? null) : null;
  });
}

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

// SVG strings for imperative DOM injection (codeBlock Copy/Run buttons live
// inside markdown-parsed HTML, so they're constructed via createElement rather
// than JSX). Content is fully static — no interpolation, safe via innerHTML.
const SVG_COPY =
  '<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2"><rect x="4" y="4" width="7" height="7" rx="1"/><path d="M3 10V3h7"/></svg>';
const SVG_COPIED =
  '<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 7l3 3 5-5"/></svg>';
const SVG_RUN =
  '<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor"><path d="M4 2.5l8 4.5-8 4.5z"/></svg>';

const IconHistory = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
    <circle cx="7" cy="7" r="5.5" />
    <path d="M7 4v3.5l2 1.5" stroke-linecap="round" />
  </svg>
);

const IconRobot = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
    <path d="M7 1a.75.75 0 01.75.75V3h1.5A2.25 2.25 0 0111.5 5.25v4.5A2.25 2.25 0 019.25 12h-4.5A2.25 2.25 0 012.5 9.75v-4.5A2.25 2.25 0 014.75 3h1.5V1.75A.75.75 0 017 1zM5 6.5a.75.75 0 100 1.5.75.75 0 000-1.5zm4 0a.75.75 0 100 1.5.75.75 0 000-1.5zM5.5 9a.5.5 0 000 1h3a.5.5 0 000-1h-3z" />
  </svg>
);

const IconPause = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
    <rect x="3" y="2" width="3" height="10" rx="0.5" />
    <rect x="8" y="2" width="3" height="10" rx="0.5" />
  </svg>
);

const IconPlay = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
    <path d="M4 2.5l8 4.5-8 4.5z" />
  </svg>
);

const IconUnlock = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
    <rect x="2.5" y="6" width="9" height="6.5" rx="1" />
    <path d="M5 6V4a2 2 0 014 0" stroke-linecap="round" />
  </svg>
);

/** Collapsible tool call card */
const ToolCallCard: Component<{ entry: ToolCallEntry }> = (props) => {
  const [expanded, setExpanded] = createSignal(false);
  const statusClass = () => {
    if (props.entry.status === "pending") return s.toolCallPending;
    return props.entry.result.success ? s.toolCallSuccess : s.toolCallFailure;
  };

  return (
    <div class={s.toolCallCard}>
      <div class={s.toolCallHeader} onClick={() => setExpanded(!expanded())}>
        <span class={cx(s.toolCallStatusDot, statusClass())} />
        <span class={s.toolCallName}>{props.entry.toolName}</span>
        <Show when={props.entry.status === "done"}>
          <span class={s.toolCallDuration}>{(props.entry as ToolCallEntry & { status: "done" }).duration}ms</span>
        </Show>
      </div>
      <Show when={expanded()}>
        <div class={s.toolCallBody}>
          <div>Args: {JSON.stringify(props.entry.args, null, 2)}</div>
          <Show when={props.entry.status === "done"}>
            <div>
              Result ({(props.entry as ToolCallEntry & { status: "done" }).result.success ? "ok" : "error"}): {(props.entry as ToolCallEntry & { status: "done" }).result.output}
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export const AIChatPanel: Component<AIChatPanelProps> = (props) => {
  const [inputText, setInputText] = createSignal("");
  let messageListRef: HTMLDivElement | undefined;
  let textareaRef: HTMLTextAreaElement | undefined;

  const [agentMode, setAgentMode] = createSignal(false);
  const [showHistory, setShowHistory] = createSignal(false);
  const [showUnrestrictedConfirm, setShowUnrestrictedConfirm] = createSignal(false);
  const [historyList, setHistoryList] = createSignal<ConversationMeta[]>([]);

  const openHistory = () => {
    void aiChatStore.listAllConversations().then(setHistoryList);
    setShowHistory(true);
  };

  const resolveSessionName = (sessionId?: string | null): string => {
    if (!sessionId) return "";
    const ids = terminalsStore.getIds();
    for (const id of ids) {
      const t = terminalsStore.get(id);
      if (t?.tuicSession === sessionId || t?.sessionId === sessionId) return t.name ?? sessionId;
    }
    return sessionId.slice(0, 8);
  };

  const handleLoadConversation = async (id: string) => {
    await aiChatStore.loadConversation(id);
    setShowHistory(false);
  };

  // Active terminal derived from terminalsStore (null when non-terminal tab focused)
  const activeSessionId = useActiveSessionId();
  const isFrozen = createMemo(() => !terminalsStore.state.activeId);
  const activeTerminalName = createMemo(() => {
    const id = terminalsStore.state.activeId;
    return id ? (terminalsStore.get(id)?.name ?? null) : null;
  });

  // ── Registry subscription lifecycle ─────────────────────────────────────
  createEffect(() => {
    const id = aiChatStore.chatId();
    void aiChatStore.subscribeToRegistry(id);
  });
  onCleanup(() => {
    void aiChatStore.unsubscribeFromRegistry();
  });

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
    if (!text || isFrozen()) return;
    const sid = activeSessionId();

    if (agentMode()) {
      const st = aiAgentStore.agentState();
      if (st === "running" || st === "paused") return;
      if (sid) aiAgentStore.startAgent(sid, text, aiAgentStore.unrestricted());
    } else {
      if (aiChatStore.isStreaming()) return;
      aiChatStore.sendMessage(text, sid);
    }

    setInputText("");
    if (textareaRef) {
      textareaRef.style.height = "auto";
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      if (e.metaKey || e.ctrlKey || e.shiftKey) {
        // Cmd/Ctrl/Shift+Enter = newline (default behavior)
        return;
      }
      e.preventDefault();
      handleSend();
    }
  };

  // ── Run code in active terminal ────────────────────────────────────────
  const runCodeInTerminal = async (code: string) => {
    const sessionId = activeSessionId();
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
    const resolvedRef = termRef;
    const shellFamily = await getShellFamily(sessionId);
    const lines = code.trim().split("\n");
    for (const line of lines) {
      await sendCommand(
        (data: string) => { resolvedRef.write(data); return Promise.resolve(); },
        line,
        agentType,
        shellFamily,
      );
    }
  };

  // ── Code block enhancement: inject Copy + Run buttons ──────────────────
  const enhanceCodeBlocks = (container: HTMLDivElement, signal: AbortSignal) => {
    const pres = container.querySelectorAll("pre");
    for (const pre of pres) {
      if (pre.parentElement?.classList.contains(s.codeBlockWrapper)) continue;

      const parent = pre.parentElement;
      if (!parent) continue;

      const wrapper = document.createElement("div");
      wrapper.className = s.codeBlockWrapper;
      parent.insertBefore(wrapper, pre);
      wrapper.appendChild(pre);

      const actions = document.createElement("div");
      actions.className = s.codeBlockActions;

      // Copy button
      const copyBtn = document.createElement("button");
      copyBtn.className = s.codeActionBtn;
      copyBtn.title = "Copy code";
      copyBtn.innerHTML = SVG_COPY;
      copyBtn.addEventListener("click", async () => {
        const text = extractCodeText(pre);
        const ok = await copyToClipboard(text);
        if (ok) {
          copyBtn.innerHTML = SVG_COPIED;
          copyBtn.classList.add(s.codeActionBtnCopied);
          setTimeout(() => {
            copyBtn.innerHTML = SVG_COPY;
            copyBtn.classList.remove(s.codeActionBtnCopied);
          }, 1500);
        }
      }, { signal });
      actions.appendChild(copyBtn);

      // Run button
      const runBtn = document.createElement("button");
      runBtn.className = s.codeActionBtn;
      runBtn.title = "Run in terminal";
      runBtn.innerHTML = SVG_RUN;
      runBtn.addEventListener("click", () => {
        const text = extractCodeText(pre);
        void runCodeInTerminal(text).catch((e) =>
          appLogger.warn("ai-chat", "Run code failed", { error: String(e) }),
        );
      }, { signal });
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
      aiChatStore.sendMessage(lastUser.content, activeSessionId());
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
          <Show when={activeTerminalName()}>
            {(name) => <span class={s.terminalName}>{name()}</span>}
          </Show>
        </div>
        <div class={s.headerActions}>
          <Show when={agentMode()}>
            <button
              class={cx(s.headerBtn, aiAgentStore.unrestricted() && s.headerBtnDanger)}
              onClick={() => {
                if (aiAgentStore.unrestricted()) {
                  aiAgentStore.setUnrestricted(false);
                } else {
                  setShowUnrestrictedConfirm(true);
                }
              }}
              title={aiAgentStore.unrestricted() ? "Disable unrestricted mode" : "Enable unrestricted mode (no approval prompts)"}
            >
              <IconUnlock />
            </button>
          </Show>
          <button
            class={cx(s.headerBtn, agentMode() && s.headerBtnActive)}
            onClick={() => setAgentMode((v) => !v)}
            title={agentMode() ? "Switch to chat mode" : "Switch to agent mode"}
          >
            <IconRobot />
          </button>
          <button
            class={cx(s.headerBtn, showHistory() && s.headerBtnActive)}
            onClick={() => (showHistory() ? setShowHistory(false) : openHistory())}
            title="Conversation history"
          >
            <IconHistory />
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

      {/* ── Unrestricted confirmation dialog ─────────────── */}
      <Show when={showUnrestrictedConfirm()}>
        <div class={s.approvalCard}>
          <div class={s.approvalText}>
            <strong>Enable unrestricted mode?</strong>
            <br />
            <span style={{ "font-size": "var(--font-xs)", color: "var(--fg-secondary)" }}>
              The agent will skip all approval prompts and operate without sandbox restrictions.
              Only use on repos you fully trust.
            </span>
          </div>
          <div class={s.approvalActions}>
            <button
              class={cx(s.approvalBtn, s.denyBtn)}
              onClick={() => {
                aiAgentStore.setUnrestricted(true);
                setShowUnrestrictedConfirm(false);
              }}
            >
              Enable
            </button>
            <button
              class={cx(s.approvalBtn, s.approveBtn)}
              onClick={() => setShowUnrestrictedConfirm(false)}
            >
              Cancel
            </button>
          </div>
        </div>
      </Show>

      {/* ── Unrestricted banner ───────────────────────────── */}
      <Show when={aiAgentStore.unrestricted()}>
        <div class={s.unrestrictedBanner}>UNRESTRICTED</div>
      </Show>

      {/* ── Agent banner ──────────────────────────────────── */}
      <Show when={aiAgentStore.agentState() === "running" || aiAgentStore.agentState() === "paused"}>
        <div class={s.agentBanner}>
          <IconRobot />
          <span class={s.agentBannerText}>
            Agent {aiAgentStore.agentState() === "paused" ? "paused" : "running"}
          </span>
          <span class={s.agentBannerIteration}>
            iter {aiAgentStore.currentIteration() + 1}
          </span>
          <Show when={aiAgentStore.agentState() === "running"}>
            <button
              class={s.agentBannerBtn}
              onClick={() => {
                const sid = activeSessionId();
                if (sid) aiAgentStore.pauseAgent(sid);
              }}
              title="Pause agent"
            >
              <IconPause />
            </button>
          </Show>
          <Show when={aiAgentStore.agentState() === "paused"}>
            <button
              class={s.agentBannerBtn}
              onClick={() => {
                const sid = activeSessionId();
                if (sid) aiAgentStore.resumeAgent(sid);
              }}
              title="Resume agent"
            >
              <IconPlay />
            </button>
          </Show>
          <button
            class={cx(s.agentBannerBtn, s.agentBannerBtnDanger)}
            onClick={() => {
              const sid = activeSessionId();
              if (sid) aiAgentStore.cancelAgent(sid);
            }}
            title="Stop agent"
          >
            <IconStop />
          </button>
        </div>
      </Show>

      {/* ── Approval prompt ────────────────────────────────── */}
      <Show when={aiAgentStore.pendingApproval()}>
        {(approval) => (
          <div class={s.approvalCard}>
            <div class={s.approvalText}>
              Agent wants to run: <strong>{approval().command}</strong>
              <br />
              <span style={{ "font-size": "var(--font-xs)", color: "var(--fg-secondary)" }}>
                {approval().reason}
              </span>
            </div>
            <div class={s.approvalActions}>
              <button
                class={cx(s.approvalBtn, s.approveBtn)}
                onClick={() => aiAgentStore.approveAction(approval().sessionId, true)}
              >
                Approve
              </button>
              <button
                class={cx(s.approvalBtn, s.denyBtn)}
                onClick={() => aiAgentStore.approveAction(approval().sessionId, false)}
              >
                Deny
              </button>
            </div>
          </div>
        )}
      </Show>

      {/* ── History panel ───────────────────────────────────── */}
      <Show when={showHistory()}>
        <div class={s.historyPanel}>
          <div class={s.historyHeader}>All conversations</div>
          <Show when={historyList().length === 0}>
            <div class={s.historyEmpty}>No conversations saved yet</div>
          </Show>
          <For each={historyList()}>
            {(conv) => (
              <button class={s.historyItem} onClick={() => void handleLoadConversation(conv.id)}>
                <span class={s.historyTitle}>{conv.title || "Untitled"}</span>
                <span class={s.historyMeta}>
                  <Show when={resolveSessionName(conv.session_id)}>
                    <span class={s.historySession}>{resolveSessionName(conv.session_id)}</span>
                  </Show>
                  <span class={s.historyCount}>{conv.message_count} msgs</span>
                  <span class={s.historyDate}>{new Date(conv.updated * 1000).toLocaleDateString()}</span>
                </span>
              </button>
            )}
          </For>
        </div>
      </Show>

      {/* ── Message list ────────────────────────────────────── */}
      <div class={cx(s.messageList, showHistory() && s.hidden)} ref={messageListRef}>
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
                      const ac = new AbortController();
                      onCleanup(() => ac.abort());
                      requestAnimationFrame(() => enhanceCodeBlocks(el, ac.signal));
                    }}
                  >
                    <ContentRenderer content={msg.content} />
                  </div>
                }
              >
                <div class={s.userMsg}>{msg.content}</div>
              </Show>
            )}
          </For>

          {/* Streaming text: render as markdown so formatting is progressive */}
          <Show when={aiChatStore.isStreaming() && aiChatStore.streamingText()}>
            <div class={s.assistantMsg}>
              <ContentRenderer content={aiChatStore.streamingText()!} />
            </div>
          </Show>

          {/* Agent tool call cards */}
          <Show when={aiAgentStore.toolCalls().length > 0}>
            <For each={aiAgentStore.toolCalls()}>
              {(entry) => <ToolCallCard entry={entry} />}
            </For>
          </Show>

          {/* Agent text output */}
          <Show when={aiAgentStore.textChunks()}>
            <div class={s.assistantMsg}>
              <ContentRenderer content={aiAgentStore.textChunks()!} />
            </div>
          </Show>
        </Show>
      </div>

      {/* ── Session knowledge footer ────────────────────────── */}
      <SessionKnowledgeBar sessionId={activeSessionId()} />

      {/* ── Frozen overlay ──────────────────────────────────── */}
      <Show when={isFrozen()}>
        <div class={s.frozenBanner}>No terminal focused — chat is read-only</div>
      </Show>

      {/* ── Input area ──────────────────────────────────────── */}
      <div class={s.inputArea}>
        <textarea
          ref={textareaRef}
          data-focus-target="ai-chat"
          class={s.textarea}
          rows={1}
          placeholder={isFrozen() ? "Focus a terminal first..." : agentMode() ? "Describe a goal for the agent..." : "Ask about your terminal... (Enter to send)"}
          value={inputText()}
          onInput={(e) => {
            setInputText(e.currentTarget.value);
            autoResize();
          }}
          onKeyDown={handleKeyDown}
          disabled={isFrozen()}
        />
        <Show
          when={aiChatStore.isStreaming()}
          fallback={
            <button
              class={s.sendBtn}
              onClick={handleSend}
              disabled={!inputText().trim() || aiChatStore.isStreaming() || isFrozen() || (agentMode() && (aiAgentStore.agentState() === "running" || aiAgentStore.agentState() === "paused"))}
              title="Send (Enter)"
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

      {/* ── Usage footer ────────────────────────────────────── */}
      <Show when={aiChatStore.sessionUsage()}>
        {(usage) => {
          const prompt = () => usage().promptTokens ?? 0;
          const completion = () => usage().completionTokens ?? 0;
          const cached = () => usage().cachedTokens ?? 0;
          const total = () => prompt() + completion();
          const cachedPct = () => (total() > 0 ? Math.round((cached() / total()) * 100) : 0);
          const cost = () => usage().costUsd;
          return (
            <div class={s.usageFooter}>
              <span title="Prompt tokens">↑{prompt().toLocaleString()}</span>
              <span title="Completion tokens">↓{completion().toLocaleString()}</span>
              <span>tok</span>
              <Show when={cost() != null}>
                <span>·</span>
                <span title="Estimated cost">${cost()!.toFixed(4)}</span>
              </Show>
              <Show when={cached() > 0}>
                <span>·</span>
                <span title="Cache hit rate">{cachedPct()}% cached</span>
              </Show>
            </div>
          );
        }}
      </Show>
    </div>
  );
};

export default AIChatPanel;
