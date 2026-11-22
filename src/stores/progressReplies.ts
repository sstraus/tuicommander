import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import { randomUuid } from "../utils/randomId";
import type { ProgressEntry } from "./progress";

export interface ProgressReplyReceipt {
	entryId: number;
	requestId: string;
	delivery: "delivered" | "queued";
}

interface ReplyState {
	draft: string;
	expanded: boolean;
	pending: boolean;
	error: string | null;
	receipt?: ProgressReplyReceipt;
	requestId?: string;
	payload?: string;
}

/** Session-local drafts shared by the toast and journal, never by selected repository. */
export function createProgressRepliesStore() {
	const [state, setState] = createStore<Record<string, Record<string, ReplyState>>>({});
	const empty = (): ReplyState => ({ draft: "", expanded: false, pending: false, error: null });
	const scope = (entry: ProgressEntry) => JSON.stringify([entry.originConnectionId ?? null, entry.project]);
	function get(entry: ProgressEntry): ReplyState {
		return state[scope(entry)]?.[entry.id] ?? empty();
	}
	function update(project: string, id: number, patch: Partial<ReplyState>): void {
		if (!state[project]) setState(project, {});
		if (!state[project][id]) setState(project, String(id), empty());
		setState(project, String(id), patch);
	}
	return {
		get,
		expand(entry: ProgressEntry, expanded: boolean) {
			if (!get(entry).pending) update(scope(entry), entry.id, { expanded });
		},
		setDraft(entry: ProgressEntry, draft: string) {
			if (!get(entry).pending) update(scope(entry), entry.id, { draft, error: null });
		},
		async send(entry: ProgressEntry, answer?: boolean): Promise<void> {
			const { project, id } = entry;
			const key = scope(entry);
			const current = get(entry);
			if (current.pending || (entry.question?.kind === "yes_no" && (entry.answered || current.receipt))) return;
			const text = current.draft.trim();
			if (answer === undefined && !text) return;
			const payload = answer === undefined ? `text:${text}` : `answer:${answer}`;
			// A transport retry reuses its identity; an edited/new comment gets a new identity.
			const requestId = current.payload === payload && current.requestId ? current.requestId : randomUuid();
			update(key, id, { pending: true, error: null, requestId, payload });
			try {
				const args = { project, input: { entryId: id, requestId, ...(answer === undefined ? { text } : { answer }) } };
				const receipt = await (entry.originConnectionId === undefined
					? invoke<ProgressReplyReceipt>("progress_reply", args)
					: invoke<ProgressReplyReceipt>("progress_reply", args, entry.originConnectionId));
				update(key, id, {
					pending: false,
					draft: "",
					expanded: false,
					receipt,
					requestId: undefined,
					payload: undefined,
				});
			} catch (error) {
				update(key, id, { pending: false, error: error instanceof Error ? error.message : String(error) });
			}
		},
		resetForTests() {
			setState(reconcile({}));
		},
	};
}

export const progressRepliesStore = createProgressRepliesStore();
