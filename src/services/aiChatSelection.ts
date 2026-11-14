import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { subscribeEvents } from "../transport";

export interface AiChatSelection {
	workspace: string;
	sessionId: string | null;
	/** The tab `sessionId` continues, set while the selection is a compaction successor. */
	replacing?: string | null;
}

/** Invalidations and reconnects both reread the owner. Adoption never writes. */
export function createAiChatSelectionSync(adopt: (selection: AiChatSelection) => void) {
	let disposed = false;
	let revision = 0;
	let writes: Promise<unknown> = Promise.resolve();
	async function refresh(): Promise<void> {
		const request = ++revision;
		try {
			const selection = await invoke<AiChatSelection>("ai_chat_selection");
			if (!disposed && request === revision && selection) adopt(selection);
		} catch (failure) {
			appLogger.warn("ai-chat", "reading shared conversation failed", failure);
		}
	}
	const subscription = subscribeEvents(
		{ "ai-chat-selection-changed": () => void writes.then(refresh) },
		{ onResync: () => void writes.then(refresh) },
	);
	const ready = subscription.then(refresh).catch((failure: unknown) => {
		appLogger.warn("ai-chat", "following shared conversation failed", failure);
	});
	return {
		ready,
		/** A click supersedes any older read, even while attaching the conversation. */
		invalidate(): void {
			++revision;
		},
		select(sessionId: string | null, replacing?: string): Promise<void> {
			++revision;
			const write = writes.then(() => invoke<void>("select_ai_chat_conversation", { sessionId, replacing }));
			writes = write.catch(() => {});
			return write;
		},
		dispose(): void {
			disposed = true;
			subscription.then((unsubscribe) => unsubscribe()).catch(() => {});
		},
	};
}
