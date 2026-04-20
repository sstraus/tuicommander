/**
 * AI Chat context menu actions — "Explain with AI" and "Fix this error"
 * registered on terminal right-click.
 */

import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import { aiChatStore } from "../../stores/aiChatStore";
import { uiStore } from "../../stores/ui";
import { terminalsStore } from "../../stores/terminals";
import { appLogger } from "../../stores/appLogger";

const PLUGIN_ID = "ai-chat";
const MAX_CHARS = 2000;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Switch the active chat context to the terminal matching sessionId. */
function switchToTerminalBySession(sessionId: string): void {
  const ids = terminalsStore.getIds();
  for (const id of ids) {
    const t = terminalsStore.get(id);
    if (t?.sessionId === sessionId) {
      const key = t.tuicSession ?? id;
      aiChatStore.setActiveTerminal(key);
      terminalsStore.setActive(id);
      return;
    }
  }
}

/** Truncate text to maxChars, appending a marker if truncated. */
export function truncateText(text: string, maxChars: number): string {
  if (text.length <= maxChars) return text;
  return text.slice(0, maxChars) + "\n[... truncated]";
}

/**
 * Get terminal text for the AI action.
 *
 * Prefers selected text (via window.getSelection). Falls back to the last 50
 * buffer lines from the active terminal ref.
 */
function getTerminalText(sessionId?: string): string {
  // 1. Try window selection (xterm selections propagate to window.getSelection)
  const sel = window.getSelection()?.toString().trim();
  if (sel) return sel;

  // 2. Fallback: last 50 lines from the terminal buffer
  if (!sessionId) return "";
  const terminals = terminalsStore.state.terminals;
  const entry = Object.values(terminals).find((t) => t.sessionId === sessionId);
  if (!entry?.ref) return "";

  try {
    // getBufferLines(start, end) — read the tail of the buffer.
    // xterm buffer length isn't directly exposed on TerminalRef, but
    // we can request a generous range and the impl clamps internally.
    // Use 0 → very large number to get all lines, then take last 50.
    const allLines = entry.ref.getBufferLines(0, 999_999);
    const tail = allLines.slice(-50);
    return tail.join("\n").trimEnd();
  } catch (e) {
    appLogger.warn("ai-chat", "Failed to read terminal buffer", { error: String(e) });
    return "";
  }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

export function registerAiChatContextActions(): Array<{ dispose(): void }> {
  const disposables: Array<{ dispose(): void }> = [];

  // ── Explain with AI ────────────────────────────────────────────────
  disposables.push(
    contextMenuActionsStore.registerContextAction(PLUGIN_ID, {
      id: "ai-chat:explain",
      label: "Explain with AI",
      target: "terminal",
      action: (ctx) => {
        const raw = getTerminalText(ctx.sessionId);
        if (!raw) {
          appLogger.info("ai-chat", "Explain: no terminal text available");
          return;
        }
        const text = truncateText(raw, MAX_CHARS);
        uiStore.setAiChatPanelVisible(true);
        if (ctx.sessionId) switchToTerminalBySession(ctx.sessionId);
        aiChatStore.sendMessage(
          `Explain this terminal output:\n\n\`\`\`\n${text}\n\`\`\``,
        );
      },
    }),
  );

  // ── Fix this error ─────────────────────────────────────────────────
  disposables.push(
    contextMenuActionsStore.registerContextAction(PLUGIN_ID, {
      id: "ai-chat:fix-error",
      label: "Fix this error",
      target: "terminal",
      action: (ctx) => {
        const raw = getTerminalText(ctx.sessionId);
        if (!raw) {
          appLogger.info("ai-chat", "Fix error: no terminal text available");
          return;
        }
        const text = truncateText(raw, MAX_CHARS);
        uiStore.setAiChatPanelVisible(true);
        if (ctx.sessionId) switchToTerminalBySession(ctx.sessionId);
        aiChatStore.sendMessage(
          `Analyze this terminal error and suggest a fix:\n\n\`\`\`\n${text}\n\`\`\`\n\nExplain: 1) What went wrong 2) The root cause 3) How to fix it`,
        );
      },
    }),
  );

  return disposables;
}
