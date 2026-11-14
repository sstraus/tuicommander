import { createPromptDraft } from "./promptDraft";

const drafts = new Map<string, ReturnType<typeof createPromptDraft>>();

/** Desktop Compose and mobile share the parked draft for the same PTY session. */
export function terminalPromptDraft(session: string) {
	let draft = drafts.get(session);
	if (!draft) {
		draft = createPromptDraft();
		draft.activate(`terminal:${session}`);
		drafts.set(session, draft);
	}
	return draft;
}
