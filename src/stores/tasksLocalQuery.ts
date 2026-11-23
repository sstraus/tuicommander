import { invoke } from "../invoke";
import type { WorkItemProvider } from "../plugins/workItemProvider";
import type { WorkItem, WorkItemGroupedPage, WorkItemQuery } from "../types/work-items.gen";

export const LOCAL_TASK_LIMIT = 1000;
export type LocalSource = Pick<WorkItemProvider, "list" | "fields"> & Partial<Pick<WorkItemProvider, "providerId">>;

/** The provider query that bounds one collection; sort and grouping are projected locally. */
export function localScope(provider: Pick<LocalSource, "providerId">, query: WorkItemQuery): WorkItemQuery {
	// GitLab type/state select the endpoint and its scope, not just a local predicate.
	const filters =
		provider.providerId === "gitlab"
			? query.filters.filter((filter) => ["type", "state"].includes(filter.fieldId))
			: [];
	return { ...query, filters, sort: null, groupBy: null, cursor: null, limit: 100 };
}

/** Collection is transport orchestration only. Rust owns every query projection. */
export async function collectLocalTasks(
	provider: LocalSource,
	query: WorkItemQuery,
	signal: AbortSignal,
): Promise<WorkItem[] | null> {
	const scope = localScope(provider, query);
	let page = await provider.list(scope, signal);
	const total = page.total;
	if (total == null || !Number.isInteger(total) || total < 0 || total > LOCAL_TASK_LIMIT) return null;
	const rows = new Map<string, WorkItem>();
	const cursors = new Set<string>();
	for (let pages = 0; pages <= LOCAL_TASK_LIMIT; pages++) {
		signal.throwIfAborted();
		if (page.total !== total) return null;
		const previousSize = rows.size;
		for (const item of page.items)
			rows.set(JSON.stringify([item.reference.providerId, item.reference.accountId, item.reference.id]), item);
		if (rows.size > total) return null;
		if (!page.nextCursor) return rows.size === total ? [...rows.values()] : null;
		if (rows.size === previousSize || cursors.has(page.nextCursor) || pages === LOCAL_TASK_LIMIT) return null;
		cursors.add(page.nextCursor);
		page = await provider.list({ ...scope, cursor: page.nextCursor }, signal);
	}
	return null;
}

export async function projectLocalTasks(
	provider: LocalSource,
	items: WorkItem[],
	query: WorkItemQuery,
): Promise<WorkItemGroupedPage> {
	const result = await invoke<WorkItemGroupedPage>("project_work_items", {
		input: {
			items,
			fields: provider.fields,
			query:
				provider.providerId === "gitlab"
					? { ...query, filters: query.filters.filter((filter) => !["type", "state"].includes(filter.fieldId)) }
					: query,
		},
	});
	if (!result || !Array.isArray(result.groups)) throw new Error("Invalid local Tasks projection response");
	return result;
}
