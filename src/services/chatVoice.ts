import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { dictationStore, type HandsFreeStatus } from "../stores/dictation";

export interface ChatVoiceTarget {
	id: string;
	name: string;
	chatSessionId: string;
}

/** The host owns attachments, including those shown in a detached window. */
export async function chatVoiceTargets(): Promise<ChatVoiceTarget[]> {
	const targets = await invoke<ChatVoiceTarget[]>("get_voice_targets");
	return Array.isArray(targets) ? targets : [];
}

/** Closing or leaving a chat must not leave its microphone or speaker bound. */
export async function stopChatVoice(sessionId?: string | null): Promise<void> {
	if (dictationStore.state.unavailable) return;
	try {
		const status = await invoke<HandsFreeStatus>("get_hands_free_status");
		if (!status?.armed || !status.sessionId?.startsWith("acp:")) return;
		if (sessionId && status.sessionId.slice(status.sessionId.indexOf(":", 4) + 1) !== sessionId) return;
		await dictationStore.disarmHandsFree();
	} catch (error) {
		appLogger.warn("dictation", "Could not release the AI Chat voice binding", error);
	}
}
