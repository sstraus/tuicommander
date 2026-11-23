import { createResource, untrack } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";

/** Comments added since the previous visit of the panel (backend `work_item_visits`). */
export interface NewComments {
	count: number;
	/** Time of the previous visit; a comment created after it is new. Null on the first visit. */
	sinceMs: number | null;
}

const NONE: NewComments = { count: 0, sinceMs: null };

/** Records one visit when the panel opens and answers what was new at that moment. A later
 *  count change (a poll) does not record again, so the marks stay while the panel is open. */
export function createNewComments(url: () => string | undefined, commentCount: () => number): () => NewComments {
	const [visit] = createResource(url, (itemUrl) =>
		invoke<NewComments>("visit_work_item", { itemUrl, commentCount: untrack(commentCount) }).catch((cause) => {
			appLogger.warn("github", "Failed to record the work item visit", cause);
			return NONE;
		}),
	);
	return () => visit() ?? NONE;
}

/** True when the comment was created after the previous visit. */
export function isNewSince(createdAt: string, sinceMs: number | null): boolean {
	return sinceMs !== null && Date.parse(createdAt) > sinceMs;
}
