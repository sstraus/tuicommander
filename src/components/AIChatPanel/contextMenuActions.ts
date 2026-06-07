/**
 * AI Chat context menu actions — "Explain with AI" and "Fix this error"
 * registered on terminal right-click.
 */

import { appLogger } from "../../stores/appLogger";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import { conversationStore } from "../../stores/conversationStore";
import { terminalsStore } from "../../stores/terminals";
import { uiStore } from "../../stores/ui";
import { switchToTerminalBySession } from "../../utils/switchToTerminalBySession";

const PLUGIN_ID = "ai-chat";
const MAX_CHARS = 2000;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
async function getTerminalText(sessionId?: string): Promise<string> {
	const sel = window.getSelection()?.toString().trim();
	if (sel) return sel;

	if (!sessionId) return "";
	const terminals = terminalsStore.state.terminals;
	const entry = Object.values(terminals).find((t) => t.sessionId === sessionId);
	if (!entry?.ref) return "";

	try {
		const allLines = await entry.ref.getBufferLines(0, 999_999);
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
			action: async (ctx) => {
				const raw = await getTerminalText(ctx.sessionId);
				if (!raw) {
					appLogger.info("ai-chat", "Explain: no terminal text available");
					return;
				}
				const text = truncateText(raw, MAX_CHARS);
				uiStore.setAiChatPanelVisible(true);
				if (ctx.sessionId) switchToTerminalBySession(ctx.sessionId);
				conversationStore.sendMessage(
					`Explain this terminal output:\n\n\`\`\`\n${text}\n\`\`\``,
					ctx.sessionId ?? null,
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
			action: async (ctx) => {
				const raw = await getTerminalText(ctx.sessionId);
				if (!raw) {
					appLogger.info("ai-chat", "Fix error: no terminal text available");
					return;
				}
				const text = truncateText(raw, MAX_CHARS);
				uiStore.setAiChatPanelVisible(true);
				if (ctx.sessionId) switchToTerminalBySession(ctx.sessionId);
				conversationStore.sendMessage(
					`Analyze this terminal error and suggest a fix:\n\n\`\`\`\n${text}\n\`\`\`\n\nExplain: 1) What went wrong 2) The root cause 3) How to fix it`,
					ctx.sessionId ?? null,
				);
			},
		}),
	);

	return disposables;
}
