import { createEffect, createSignal, onCleanup, untrack } from "solid-js";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import { createTasksQuery } from "../../../stores/tasksQuery";
import { uiStore } from "../../../stores/ui";
import type { WorkItemQuery } from "../../../types/work-items.gen";

export const initialTaskQuery = (): WorkItemQuery => ({
	filters: [],
	savedFilterId: null,
	search: "",
	sort: null,
	groupBy: null,
	cursor: null,
	limit: 100,
});

/** Persist only UI intent; provider results and page cursors remain transient. */
export function useTasksListQuery(
	props: { repoPath?: string; visible?: boolean },
	provider: () => WorkItemProvider | undefined,
) {
	const model = createTasksQuery();
	const [query, setQuery] = createSignal(initialTaskQuery());
	const [search, setSearch] = createSignal("");
	const [linkedOnly, setLinkedOnly] = createSignal(false);
	let debounce: ReturnType<typeof setTimeout> | undefined;
	let generation = 0;
	let lastValidSort: WorkItemQuery["sort"] = null;
	let restoredQuery: WorkItemQuery | undefined;
	let queryScope = "";
	const same = (left: unknown, right: unknown) => JSON.stringify(left) === JSON.stringify(right);
	/** Save the query only when it differs from the stored one, so a preferences reload cannot re-trigger a save. */
	const saveQuery = (key: string, request: WorkItemQuery) => {
		if (!props.repoPath || same(uiStore.state.tasksViewPreferences[key]?.query, request)) return;
		uiStore.setTaskViewPreference(key, { query: request });
	};
	const scope = () => JSON.stringify([props.repoPath, provider()?.providerId, provider()?.accountId]);
	createEffect(() => {
		scope();
		uiStore.state.tasksPreferencesRevision;
		untrack(() => {
			const sameScope = queryScope === scope();
			queryScope = scope();
			restoredQuery = undefined;
			clearTimeout(debounce);
			const saved = props.repoPath ? uiStore.state.tasksViewPreferences[scope()] : undefined;
			const next = saved?.query
				? (JSON.parse(JSON.stringify({ ...initialTaskQuery(), ...saved.query, cursor: null })) as WorkItemQuery)
				: initialTaskQuery();
			lastValidSort = saved?.lastValidSort ? { ...saved.lastValidSort } : null;
			setLinkedOnly(saved?.linkedOnly ?? false);
			// A reload that restores the query already shown must not start another request.
			if (sameScope && same(next, query())) return;
			generation++;
			setSearch(next.search);
			setQuery(next);
		});
	});
	createEffect(() => {
		const current = provider();
		const request = query();
		const key = scope();
		if (props.visible === false || key !== queryScope) return;
		untrack(() => {
			if (request === restoredQuery) {
				restoredQuery = undefined;
				saveQuery(key, request);
				return;
			}
			const token = ++generation;
			if (!current) {
				model.clear();
				return;
			}
			saveQuery(key, request);
			void model.select(current, request).then(() => {
				if (token !== generation) return;
				const error = model.state().error;
				if (error) {
					const accepted = model.acceptedQuery();
					if (accepted) {
						restoredQuery = structuredClone({
							...request,
							sort: accepted.sort,
							groupBy: accepted.groupBy,
							filters: accepted.filters,
						});
						setQuery(restoredQuery);
					} else if (request.sort && /sort|order.?by/i.test(error)) {
						const fallback = JSON.stringify(lastValidSort) === JSON.stringify(request.sort) ? null : lastValidSort;
						lastValidSort = null;
						setQuery({ ...request, sort: fallback, savedFilterId: null });
					}
				} else {
					lastValidSort = request.sort;
					if (props.repoPath && !same(uiStore.state.tasksViewPreferences[key]?.lastValidSort ?? null, lastValidSort))
						uiStore.setTaskViewPreference(key, { lastValidSort });
				}
			});
		});
	});
	onCleanup(() => {
		generation++;
		clearTimeout(debounce);
		model.dispose();
	});
	return {
		model,
		query,
		setQuery,
		search,
		linkedOnly,
		setLinkedOnly(value: boolean) {
			setLinkedOnly(value);
			if (props.repoPath) uiStore.setTaskViewPreference(scope(), { linkedOnly: value });
		},
		cancelSearch() {
			clearTimeout(debounce);
		},
		setSearch,
		updateSearch(value: string) {
			setSearch(value);
			clearTimeout(debounce);
			debounce = setTimeout(() => setQuery((previous) => ({ ...previous, savedFilterId: null, search: value })), 250);
		},
	};
}
