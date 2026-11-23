import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../../../__tests__/mocks/tauri";
import { createTasksQuery } from "../../../stores/tasksQuery";
import type { WorkItem, WorkItemPage, WorkItemQuery } from "../../../types/work-items.gen";

const query: WorkItemQuery = {
	filters: [],
	savedFilterId: null,
	search: "",
	sort: null,
	groupBy: null,
	cursor: null,
	limit: 100,
};
function item(id: string, accountId = "a"): WorkItem {
	return {
		reference: { providerId: "demo", accountId, id },
		key: id,
		title: id,
		itemType: "issue",
		state: { id: "open", name: "Open", tone: "info" },
		assignees: [],
		labels: [],
		updatedAt: "2026-10-10",
		url: "https://example.test",
		fields: {},
	};
}
function deferred() {
	let resolve: (page: WorkItemPage) => void = () => {
		throw new Error("not initialized");
	};
	const promise = new Promise<WorkItemPage>((done) => {
		resolve = done;
	});
	return { promise, resolve };
}
function source(accountId: string, list: (q: WorkItemQuery) => Promise<WorkItemPage>) {
	return {
		providerId: "demo",
		accountId,
		list,
		grouped: async () => {
			throw new Error("Grouping not used by this source");
		},
	};
}

describe("Tasks query consumer", () => {
	// Catches: an older search response replaces the latest search rows.
	it("stale_response_never_overwrites_newer_query", async () => {
		const old = deferred();
		const recent = deferred();
		const provider = source("a", (q) => (q.search === "old" ? old.promise : recent.promise));
		const model = createTasksQuery();
		const first = model.select(provider, { ...query, search: "old" });
		const second = model.select(provider, { ...query, search: "new" });
		recent.resolve({ items: [item("new")], nextCursor: null });
		await second;
		old.resolve({ items: [item("old")], nextCursor: null });
		await first;
		expect(model.state().items.map((row) => row.key)).toEqual(["new"]);
	});

	// Catches: cache keys omit the account or old-account rows survive a failed switch.
	it("cache_is_partitioned_by_account", async () => {
		const model = createTasksQuery();
		await model.select(
			source("a", async () => ({ items: [item("A")], nextCursor: null })),
			query,
		);
		await model.select(
			source("b", async () => {
				throw new Error("offline");
			}),
			query,
		);
		expect(model.state().items).toEqual([]);
		await model.select(
			source("b", async () => ({ items: [item("B", "b")], nextCursor: null })),
			query,
		);
		expect(model.state().items.map((row) => row.key)).toEqual(["B"]);
	});

	// Catches: a repeated provider cursor appends duplicate rows or loops forever.
	it("next_page_appends_without_duplicates", async () => {
		const model = createTasksQuery();
		await model.select(
			source("a", async (q) =>
				q.cursor === null
					? { items: [item("one")], nextCursor: "page2" }
					: { items: [item("one"), item("two")], nextCursor: "page2" },
			),
			query,
		);
		await model.loadMore();
		expect(model.state().items.map((row) => row.key)).toEqual(["one", "two"]);
		expect(model.state().nextCursor).toBeNull();
	});

	// Catches: refreshing page one reuses obsolete cached rows from later pages.
	it("refresh_invalidates_cached_later_pages", async () => {
		const model = createTasksQuery();
		let title = "Old second page";
		const provider = source("a", async (request) =>
			request.cursor
				? { items: [{ ...item("two"), title }], nextCursor: null }
				: { items: [item("one")], nextCursor: "page2" },
		);
		await model.select(provider, query);
		await model.loadMore();
		title = "Updated second page";
		await model.refresh();
		await model.loadMore();
		expect(model.state().items.map((row) => row.title)).toEqual(["one", "Updated second page"]);
	});

	// Catches: refresh failure silently discards rows or presents cached data as fresh.
	it("failed_refresh_retains_rows_marked_stale", async () => {
		const model = createTasksQuery();
		let fail = false;
		const provider = source("a", async () => {
			if (fail) throw new Error("offline");
			return { items: [item("one")], nextCursor: null };
		});
		await model.select(provider, query);
		fail = true;
		await model.refresh();
		expect(model.state().items.map((row) => row.key)).toEqual(["one"]);
		expect(model.state().stale).toBe(true);
		expect(model.state().error).toBe("offline");
	});

	// Catches: the total of a previous source or page survives into a page that reports none.
	it("total_follows_the_latest_page_and_resets_when_unreported", async () => {
		const model = createTasksQuery();
		await model.select(
			source("a", async (q) =>
				q.cursor === null
					? { items: [item("one")], nextCursor: "page2", total: 2 }
					: { items: [item("two")], nextCursor: null, total: 2 },
			),
			query,
		);
		expect(model.state().total).toBe(2);
		await model.loadMore();
		expect(model.state().total).toBe(2);
		await model.select(
			source("b", async () => ({ items: [item("B", "b")], nextCursor: null })),
			query,
		);
		expect(model.state().total).toBeNull();
	});

	// Catches: the page cache grows without evicting the least recently used query.
	it("least_recent_query_is_evicted_at_capacity", async () => {
		const model = createTasksQuery({ maxCachedPages: 2 });
		let calls = 0;
		const provider = source("a", async (q) => {
			calls++;
			return { items: [item(q.search)], nextCursor: null };
		});
		for (const search of ["one", "two", "one", "three", "two"]) await model.select(provider, { ...query, search });
		expect(calls).toBe(4);
	});

	// Catches: grouped cursor pages replace earlier group members or fall back to locally inferred groups.
	it("grouped_pages_preserve_provider_labels_and_append_members", async () => {
		const model = createTasksQuery();
		const provider = {
			...source("a", async () => {
				throw new Error("List must not be used");
			}),
			grouped: async (q: WorkItemQuery) => ({
				groups: [{ id: "custom", name: "Provider group", items: q.cursor ? [item("two")] : [item("one")] }],
				nextCursor: q.cursor ? null : "second",
			}),
		};
		await model.select(provider, { ...query, groupBy: "state" });
		await model.loadMore();
		expect(model.state().groups).toHaveLength(1);
		expect(model.state().groups[0].name).toBe("Provider group");
		expect(model.state().groups[0].items.map((row) => row.key)).toEqual(["one", "two"]);
	});
});

// Catches: GitLab username controls are compared against unrelated numeric REST identities locally.
it("gitlab_username_filters_remain_in_server_mode", async () => {
	// @ts-expect-error The GitLab plugin is plain JavaScript without declarations.
	const { createGitLabProvider } = await import("../../../../plugins/gitlab/provider.js");
	const { default: recorded } = await import("../../../../plugins/gitlab/fixtures/recorded.json");
	for (const fieldId of ["assignee", "author"]) {
		const provider = createGitLabProvider({
			account: { id: "local", host: "https://gitlab.example.invalid:8443", login: "root" },
			project: "review/demo",
			fetch: async (request: { url: string }) => {
				const params = new URL(request.url).searchParams;
				expect(params.get(`${fieldId}_username`)).toBe("dev");
				return { status: 200, headers: { "x-total": "1" }, json: [recorded.mr] };
			},
		});
		const model = createTasksQuery();
		await model.select(provider, {
			...query,
			filters: [
				{ fieldId: "type", values: [{ kind: "enum", value: "merge_request" }] },
				{ fieldId, values: [{ kind: "user", value: [{ id: "dev", name: "dev", avatarUrl: null, bot: false }] }] },
			],
		});
		expect(model.state().error).toBeNull();
		expect(model.state().local).toBe(false);
		expect(model.state().items.map((row) => row.itemType)).toEqual(["merge_request"]);
		expect(model.state().items[0].assignees[0].id).toBe("2");
		model.dispose();
	}
});

// Catches: the previous error clears the moment a retry starts, so the view flashes red off and on.
it("error_stays_visible_while_no_new_request_runs", async () => {
	const retry = deferred();
	let fail = true;
	const provider = source("a", () => {
		if (fail) return Promise.reject(new Error("offline"));
		return retry.promise;
	});
	const model = createTasksQuery();
	await model.select(provider, query);
	expect(model.state().error).toBe("offline");
	fail = false;
	const pending = model.refresh();
	expect(model.state().loading).toBe(true);
	expect(model.state().error).toBe("offline");
	retry.resolve({ items: [item("ok")], nextCursor: null });
	await pending;
	expect(model.state().error).toBeNull();
});

describe("Tasks local projection transport", () => {
	const localSource = (list: (q: WorkItemQuery) => Promise<WorkItemPage>) => ({
		...source("a", list),
		fields: [],
	});
	const rateLimited = (status: number, extra: Record<string, unknown>) =>
		Object.assign(new Error("GitHub request failed"), { status, ...extra });
	beforeEach(() => {
		mockInvoke.mockReset().mockImplementation(async (command: string, args?: unknown) =>
			command === "project_work_items"
				? {
						groups: [
							{
								id: "all",
								name: "All",
								items: (args as { input: { items: WorkItem[] } }).input.items,
							},
						],
						nextCursor: null,
					}
				: undefined,
		);
	});
	afterEach(() => mockInvoke.mockReset().mockResolvedValue(undefined));

	// Catches: collection runs before the page cache check, so a repeated query spends the scope again.
	it("same_query_twice_collects_the_scope_once", async () => {
		const list = vi.fn(async () => ({ items: [item("one")], nextCursor: null, total: 1 }));
		const model = createTasksQuery();
		await model.select(localSource(list), query);
		await model.select(localSource(list), query);
		expect(model.state().local).toBe(true);
		expect(model.state().items.map((row) => row.key)).toEqual(["one"]);
		expect(list).toHaveBeenCalledTimes(1);
	});

	// Catches: collection keeps paging after a 403/429, or the error changes on every retry, or refresh stays blocked.
	it.each([
		[429, { retryAfter: "90" }],
		[403, { resetAt: String(Math.floor(Date.now() / 1000) + 600) }],
	])("status_%s_stops_collection_with_a_stable_rate_limit_error", async (status, extra) => {
		const list = vi.fn(async (q: WorkItemQuery) => {
			if (q.cursor) throw rateLimited(status, extra);
			return { items: [item("one")], nextCursor: "page2", total: 2 };
		});
		const model = createTasksQuery();
		await model.select(localSource(list), query);
		expect(list).toHaveBeenCalledTimes(2);
		const message = model.state().error;
		expect(message).toMatch(/^GitHub rate limit — retry after \d{2}:\d{2}$/);
		await model.select(localSource(list), query);
		expect(list).toHaveBeenCalledTimes(2);
		expect(model.state().error).toBe(message);
		await model.refresh();
		expect(list).toHaveBeenCalledTimes(4);
	});

	// Catches: every 403 is reported as a rate limit although it is a permission error.
	it("plain_403_is_not_a_rate_limit", async () => {
		const model = createTasksQuery();
		await model.select(
			localSource(async () => {
				throw rateLimited(403, {});
			}),
			query,
		);
		expect(model.state().error).toBe("GitHub request failed");
	});
});

describe("typed gateway limit", () => {
	// Catches: the typed rate_limited error is read as a generic failure and the default 60 s guess replaces its retry_at.
	it("honours retry_at of a typed rate_limited error and exposes it", async () => {
		const retryAt = Date.now() + 600_000;
		const list = vi.fn(async () => {
			throw { status: 429, kind: "rate_limited", bucket: "core", retry_at: retryAt };
		});
		const model = createTasksQuery();
		await model.select(source("a", list), query);
		expect(model.state().limit).toEqual({ kind: "rate_limited", bucket: "core", until: retryAt });
		await model.select(source("a", list), query);
		expect(list).toHaveBeenCalledTimes(1);
	});
});
