import { createSignal } from "solid-js";
import { forceFreshness } from "../plugins/freshness";
import type { WorkItemProvider } from "../plugins/workItemProvider";
import { parseLimitError } from "../transport/limitError";
import type {
	WorkItem,
	WorkItemGroup,
	WorkItemGroupedPage,
	WorkItemPage,
	WorkItemQuery,
} from "../types/work-items.gen";
import { clockTime } from "../utils/clockTime";
import { collectLocalTasks, localScope, projectLocalTasks } from "./tasksLocalQuery";

type Source = Pick<WorkItemProvider, "providerId" | "accountId" | "list" | "grouped"> &
	Partial<Pick<WorkItemProvider, "fields">>;
interface QueryState {
	items: WorkItem[];
	groups: WorkItemGroup[];
	nextCursor: string | null;
	/** Count of every match across all pages, when the provider reports it. */
	total: number | null;
	local: boolean;
	loading: boolean;
	stale: boolean;
	/** When the rows on screen were last loaded, epoch milliseconds. */
	cachedAt: number | null;
	/** The gateway's typed refusal, when it is why the load failed. */
	limit: { kind: "rate_limited" | "shed"; bucket: string | null; until: number } | null;
	error: string | null;
}
const empty = (): QueryState => ({
	items: [],
	groups: [],
	nextCursor: null,
	total: null,
	local: false,
	loading: false,
	stale: false,
	cachedAt: null,
	limit: null,
	error: null,
});

const DEFAULT_RETRY_SECONDS = 60;
const clock = clockTime;

/**
 * GitHub answers an exhausted quota with 429, or with 403 plus a retry hint; a bare 403 is a permission error.
 * The retry time comes from retry-after (seconds or date) or from x-ratelimit-reset (epoch seconds).
 */
function rateLimitOf(
	error: unknown,
): { until: number; message: string; limit: NonNullable<QueryState["limit"]> } | null {
	const typed = parseLimitError(error);
	if (typed)
		return {
			until: typed.retry_at,
			message: `Rate limited — retry after ${clock(typed.retry_at)}`,
			limit: { kind: typed.kind, bucket: typed.bucket, until: typed.retry_at },
		};
	const { status, retryAfter, resetAt } = (error ?? {}) as {
		status?: unknown;
		retryAfter?: unknown;
		resetAt?: unknown;
	};
	const hint = retryAfter ?? resetAt;
	if (status !== 429 && !(status === 403 && hint != null)) return null;
	const now = Date.now();
	let until = now + DEFAULT_RETRY_SECONDS * 1000;
	if (retryAfter != null) {
		const seconds = Number(retryAfter);
		const date = Date.parse(String(retryAfter));
		if (Number.isFinite(seconds)) until = now + seconds * 1000;
		else if (Number.isFinite(date)) until = date;
	} else if (resetAt != null && Number.isFinite(Number(resetAt))) until = Number(resetAt) * 1000;
	return {
		until,
		message: `GitHub rate limit — retry after ${clock(until)}`,
		limit: { kind: "rate_limited", bucket: null, until },
	};
}

/** View state and transport: providers own server queries; Rust projects certified complete scopes. */
export function createTasksQuery(options: { maxCachedPages?: number; local?: boolean } = {}) {
	const requestedCapacity = options.maxCachedPages ?? 32;
	const capacity = Number.isFinite(requestedCapacity) ? Math.max(1, Math.floor(requestedCapacity)) : 32;
	const cache = new Map<string, WorkItemPage | WorkItemGroupedPage>();
	const [state, setState] = createSignal<QueryState>(empty());
	let generation = 0;
	let source: Source | undefined;
	let query: WorkItemQuery | undefined;
	let acceptedQuery: WorkItemQuery | undefined;
	let controller: AbortController | undefined;
	const consumedCursors = new Set<string>();
	/** Collected local scopes; null records a scope that must stay in server mode. */
	const collected = new Map<string, WorkItem[] | null>();
	/** Accounts whose quota is exhausted, until the time they named. */
	const limits = new Map<string, NonNullable<ReturnType<typeof rateLimitOf>>>();
	const key = (provider: Source, request: WorkItemQuery) =>
		JSON.stringify([provider.providerId, provider.accountId, request]);
	const identity = (item: WorkItem) =>
		JSON.stringify([item.reference.providerId, item.reference.accountId, item.reference.id]);

	async function request(append: boolean, force: boolean) {
		if (!source || !query) return;
		const provider = source;
		const token = generation;
		const cursor = append ? state().nextCursor : null;
		if (append && (!cursor || state().loading || consumedCursors.has(cursor))) return;
		const pageQuery = { ...query, cursor };
		const pageKey = key(provider, pageQuery);
		const accountKey = JSON.stringify([provider.providerId, provider.accountId]);
		const limit = force ? undefined : limits.get(accountKey);
		if (limit && limit.until > Date.now()) {
			setState((previous) => ({
				...previous,
				loading: false,
				stale: previous.items.length > 0,
				limit: limit.limit,
				error: limit.message,
			}));
			return;
		}
		limits.delete(accountKey);
		controller?.abort();
		const abort = new AbortController();
		if (force) forceFreshness(abort.signal);
		controller = abort;
		setState((previous) => ({ ...previous, loading: true }));
		try {
			let local = false;
			let page: WorkItemPage | WorkItemGroupedPage | undefined;
			// GitLab controls submit usernames, while REST rows carry numeric user IDs.
			const usernameFilter =
				provider.providerId === "gitlab" &&
				pageQuery.filters.some((filter) => ["assignee", "author"].includes(filter.fieldId));
			if (!append && options.local !== false && provider.fields && !usernameFilter) {
				const localSource = {
					providerId: provider.providerId,
					list: provider.list.bind(provider),
					fields: provider.fields,
				};
				const scopeKey = key(provider, localScope(localSource, pageQuery));
				let items = force ? undefined : collected.get(scopeKey);
				if (items === undefined) {
					items = await collectLocalTasks(localSource, pageQuery, abort.signal);
					if (token !== generation || abort.signal.aborted) return;
					collected.delete(scopeKey);
					collected.set(scopeKey, items);
					while (collected.size > capacity) {
						const oldest = collected.keys().next().value;
						if (oldest === undefined) break;
						collected.delete(oldest);
					}
				}
				if (items) {
					page = await projectLocalTasks(localSource, items, pageQuery);
					local = true;
				}
			}
			page ??= force ? undefined : cache.get(pageKey);
			if (!page)
				page = pageQuery.groupBy
					? await provider.grouped(pageQuery, abort.signal)
					: await provider.list(pageQuery, abort.signal);
			if (token !== generation || abort.signal.aborted) return;
			cache.delete(pageKey);
			if (!local) cache.set(pageKey, page);
			while (cache.size > capacity) {
				const oldest = cache.keys().next().value;
				if (oldest === undefined) break;
				cache.delete(oldest);
			}
			acceptedQuery = structuredClone({ ...pageQuery, cursor: null });
			if (cursor) consumedCursors.add(cursor);
			const rows = new Map<string, WorkItem>();
			if (append) for (const item of state().items) rows.set(identity(item), item);
			const groups = new Map<string, WorkItemGroup>();
			if ("groups" in page) {
				if (append) for (const group of state().groups) groups.set(group.id, group);
				for (const group of page.groups) {
					const groupedRows = new Map((groups.get(group.id)?.items ?? []).map((item) => [identity(item), item]));
					for (const item of group.items) {
						groupedRows.set(identity(item), item);
						rows.set(identity(item), item);
					}
					groups.set(group.id, { ...group, items: [...groupedRows.values()] });
				}
			} else for (const item of page.items) rows.set(identity(item), item);
			setState({
				items: [...rows.values()],
				groups: pageQuery.groupBy ? [...groups.values()] : [],
				local,
				nextCursor: page.nextCursor && !consumedCursors.has(page.nextCursor) ? page.nextCursor : null,
				total: page.total ?? null,
				loading: false,
				stale: false,
				cachedAt: Date.now(),
				limit: null,
				error: null,
			});
		} catch (error) {
			if (token !== generation || abort.signal.aborted) return;
			// Restore rejected controls, not the attempted search or its retry target.
			if (acceptedQuery)
				query = { ...query, sort: acceptedQuery.sort, groupBy: acceptedQuery.groupBy, filters: acceptedQuery.filters };
			const limited = rateLimitOf(error);
			if (limited) limits.set(accountKey, limited);
			setState((previous) => ({
				...previous,
				loading: false,
				stale: previous.items.length > 0,
				limit: limited?.limit ?? null,
				error: limited?.message ?? (error instanceof Error ? error.message : String(error)),
			}));
		}
	}

	return {
		state,
		acceptedQuery: () => acceptedQuery,
		async select(provider: Source, nextQuery: WorkItemQuery) {
			const sameSource = source?.providerId === provider.providerId && source?.accountId === provider.accountId;
			generation++;
			controller?.abort();
			if (!sameSource) acceptedQuery = undefined;
			source = provider;
			query = structuredClone({ ...nextQuery, cursor: null });
			consumedCursors.clear();
			setState((previous) =>
				sameSource ? { ...previous, nextCursor: null, stale: previous.items.length > 0 } : empty(),
			);
			await request(false, false);
		},
		loadMore: () => request(true, false),
		async refresh() {
			generation++;
			cache.clear();
			collected.clear();
			limits.clear();
			consumedCursors.clear();
			await request(false, true);
		},
		clear() {
			generation++;
			controller?.abort();
			source = undefined;
			query = undefined;
			acceptedQuery = undefined;
			consumedCursors.clear();
			setState(empty());
		},
		dispose() {
			generation++;
			controller?.abort();
			cache.clear();
			collected.clear();
			limits.clear();
		},
	};
}
