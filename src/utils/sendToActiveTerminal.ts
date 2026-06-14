import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { terminalsStore } from "../stores/terminals";
import { getShellFamily, sendCommand } from "./sendCommand";

/** Send text to the currently-active terminal as a command, routed through
 *  the canonical `sendCommand` (agent-aware split Enter for Ink raw mode,
 *  bracketed-paste for multi-line, Windows-native Ctrl-U skip).
 *
 *  Shared by the Notes panel "Send to Terminal" action in both its attached
 *  (PanelOrchestrator) and detached (notes panel adapter) forms so the two
 *  paths can never drift. No-op when there is no active PTY session. */
export async function sendTextToActiveTerminal(text: string): Promise<void> {
	const active = terminalsStore.getActive();
	const sessionId = active?.sessionId;
	if (!sessionId) return;
	const agentType = terminalsStore.getAgentTypeForSession(sessionId);
	const shellFamily = await getShellFamily(sessionId);
	try {
		await sendCommand((data) => invoke("write_pty", { sessionId, data }), text, agentType, shellFamily);
	} catch (err) {
		appLogger.error("network", `Send to terminal failed: ${err instanceof Error ? err.message : String(err)}`);
	}
	requestAnimationFrame(() => active?.ref?.focus());
}
