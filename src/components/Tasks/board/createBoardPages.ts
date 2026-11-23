import { createSignal } from "solid-js";
import { forceFreshness } from "../../../plugins/freshness";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import { parseLimitError } from "../../../transport/limitError";
import type { WorkItemBoardPage, WorkItemQuery } from "../../../types/work-items.gen";
import { cardKey } from "./types";

const query: WorkItemQuery = {
	filters: [],
	savedFilterId: null,
	search: "",
	sort: null,
	groupBy: null,
	cursor: null,
	limit: 50,
};
/** View pagination only; columns, placement and move legality come from the provider. */
export function createBoardPages() {
	const [page, setPage] = createSignal<WorkItemBoardPage>();
	const [loading, setLoading] = createSignal(false);
	const [error, setError] = createSignal("");
	const [limit, setLimit] = createSignal<{ until: number } | null>(null);
	const [cachedAt, setCachedAt] = createSignal<number | null>(null);
	let source: WorkItemProvider | undefined;
	let boardId = "";
	let generation = 0;
	let controller: AbortController | undefined;
	function cancel() {
		generation++;
		controller?.abort();
		setLoading(false);
	}
	function merge(previous: WorkItemBoardPage | undefined, next: WorkItemBoardPage): WorkItemBoardPage {
		const cards = new Map<string, { columnId: string; card: WorkItemBoardPage["lanes"][number]["cards"][number] }>();
		for (const lane of [...(previous?.lanes ?? []), ...next.lanes])
			for (const card of lane.cards) cards.set(cardKey(card.item.reference), { columnId: lane.columnId, card });
		const lanes = new Map(
			next.board.columns.map((column) => [
				column.id,
				{ columnId: column.id, cards: [] as WorkItemBoardPage["lanes"][number]["cards"] },
			]),
		);
		for (const { columnId, card } of cards.values())
			lanes.get(columnId)?.cards.push(next.board.writeDisabledReason ? { ...card, moves: [] } : card);
		return {
			...next,
			lanes: [...lanes.values()],
		};
	}
	async function request(append: boolean, force = false) {
		const provider = source;
		const id = boardId;
		const cursor = append ? (page()?.nextCursor ?? null) : null;
		if (!provider || !id || (append && (!cursor || loading()))) return;
		cancel();
		const attempt = generation;
		const abort = new AbortController();
		if (force) forceFreshness(abort.signal);
		controller = abort;
		setLoading(true);
		setError("");
		setLimit(null);
		try {
			const next = await provider.board(id, { ...query, cursor }, abort.signal);
			if (attempt !== generation || abort.signal.aborted) return;
			setPage(merge(page(), next));
			setCachedAt(Date.now());
		} catch (cause) {
			if (attempt !== generation || abort.signal.aborted) return;
			const typed = parseLimitError(cause);
			if (typed) setLimit({ until: typed.retry_at });
			setError(cause instanceof Error ? cause.message : String(cause));
		} finally {
			if (attempt === generation) setLoading(false);
		}
	}
	return {
		page,
		loading,
		error,
		limit,
		cachedAt,
		cancel,
		select(provider: WorkItemProvider, id: string) {
			cancel();
			source = provider;
			boardId = id;
			setPage(undefined);
			setError("");
			if (id) void request(false);
		},
		more: () => request(true),
		refresh: () => request(false, true),
	};
}
