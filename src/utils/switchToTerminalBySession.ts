import { conversationStore } from "../stores/conversationStore";
import { terminalsStore } from "../stores/terminals";

/**
 * Switch both the active chat context and the active terminal to the one whose
 * PTY matches `sessionId`. No-op if no terminal owns that session.
 *
 * The conversation key is `tuicSession ?? id` — the same key the AI Chat panel
 * and conversationStore use per terminal.
 */
export function switchToTerminalBySession(sessionId: string): void {
	for (const id of terminalsStore.getIds()) {
		const t = terminalsStore.get(id);
		if (t?.sessionId === sessionId) {
			conversationStore.setActiveTerminal(t.tuicSession ?? id);
			terminalsStore.setActive(id);
			return;
		}
	}
}
