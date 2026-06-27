import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { terminalsStore } from "../stores/terminals";
import { getShellFamily, sendCommand } from "./sendCommand";

/** Send text to a specific PTY session as a command, routed through the
 *  canonical `sendCommand` (agent-aware split Enter for Ink raw mode,
 *  bracketed-paste for multi-line, Windows-native Ctrl-U skip).
 *
 *  Uses the smart `invoke` wrapper so it works in both Tauri and browser modes.
 *  Bypassing `sendCommand` (raw `write_pty` text + "\r") submits in browser
 *  thanks to the per-session HTTP write-queue gap, but NOT in Tauri — the back-
 *  to-back IPC writes land in one PTY read chunk and Ink swallows the Enter. */
export async function sendTextToSession(sessionId: string, text: string): Promise<void> {
	const agentType = terminalsStore.getAgentTypeForSession(sessionId);
	const shellFamily = await getShellFamily(sessionId);
	await sendCommand((data) => invoke("write_pty", { sessionId, data }), text, agentType, shellFamily);
}

/** Send text to the currently-active terminal as a command.
 *
 *  Shared by the Notes panel "Send to Terminal" action in both its attached
 *  (PanelOrchestrator) and detached (notes panel adapter) forms so the two
 *  paths can never drift. No-op when there is no active PTY session. */
export async function sendTextToActiveTerminal(text: string): Promise<void> {
	const active = terminalsStore.getActive();
	const sessionId = active?.sessionId;
	if (!sessionId) return;
	try {
		await sendTextToSession(sessionId, text);
	} catch (err) {
		appLogger.error("network", `Send to terminal failed: ${err instanceof Error ? err.message : String(err)}`);
	}
	requestAnimationFrame(() => active?.ref?.focus());
}
