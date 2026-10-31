import { createEffect, createMemo, createSignal, onCleanup, untrack } from "solid-js";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import { tasksNotifications } from "../../../services/tasksNotifications";
import type { WorkItemAction, WorkItemCommentEntry, WorkItemDetail, WorkItemRef } from "../../../types/work-items.gen";

export function commentKey(entry: WorkItemCommentEntry): string {
	return `${entry.kind}:${entry.kind === "review_summary" ? entry.value.comment.id : entry.value.id}`;
}

export function useTasksDetail(props: {
	provider: WorkItemProvider;
	reference: WorkItemRef | null;
	visible?: boolean;
	focusComments?: object;
	onItemChange?: (detail: WorkItemDetail) => void;
}) {
	const [detail, setDetail] = createSignal<WorkItemDetail>();
	const [loading, setLoading] = createSignal(false);
	const [paging, setPaging] = createSignal(false);
	const [pending, setPending] = createSignal<string>();
	const [pendingKind, setPendingKind] = createSignal<WorkItemAction["kind"]>();
	const [error, setError] = createSignal<string>();
	const [documentVisible, setDocumentVisible] = createSignal(document.visibilityState !== "hidden");
	let epoch = 0;
	let request = 0;
	let pageCount = 1;
	let controller = new AbortController();
	let source = props.provider;
	let reference: WorkItemRef | null = null;
	const identity = createMemo(() =>
		props.reference
			? JSON.stringify([props.reference.providerId, props.reference.accountId, props.reference.id])
			: null,
	);
	const active = () => props.visible !== false && documentVisible() && identity() !== null;
	const report = (cause: unknown) => setError(cause instanceof Error ? cause.message : String(cause));
	const accept = (value: WorkItemDetail, silent = false) => {
		void tasksNotifications.observe(value, silent);
		setDetail(value);
		props.onItemChange?.(value);
	};

	// Re-read pages already opened rather than dropping their comments after a refresh or an action.
	async function withOpenPages(
		first: WorkItemDetail,
		provider: WorkItemProvider,
		target: WorkItemRef,
		signal: AbortSignal,
	): Promise<WorkItemDetail> {
		let value = first;
		for (let page = 1; page < pageCount && value.comments.nextCursor; page++) {
			const next = await provider.comments(target, value.comments.nextCursor, 50, signal);
			value = {
				...value,
				comments: { entries: mergeEntries(value.comments.entries, next.entries), nextCursor: next.nextCursor },
			};
		}
		return value;
	}

	async function refresh() {
		if (!reference || loading() || pending() || paging() || !active()) return;
		const currentEpoch = epoch;
		const currentRequest = ++request;
		const target = reference;
		const provider = source;
		const signal = controller.signal;
		setLoading(true);
		setError(undefined);
		try {
			const value = await withOpenPages(await provider.detail(target, signal), provider, target, signal);
			if (currentEpoch === epoch && currentRequest === request) accept(value);
		} catch (cause) {
			if (currentEpoch === epoch && currentRequest === request) report(cause);
		} finally {
			if (currentEpoch === epoch && currentRequest === request) setLoading(false);
		}
	}

	async function loadMore() {
		const value = detail();
		if (!reference || !value?.comments.nextCursor || loading() || paging() || pending()) return;
		const currentEpoch = epoch;
		setPaging(true);
		setError(undefined);
		try {
			const next = await source.comments(reference, value.comments.nextCursor, 50, controller.signal);
			if (currentEpoch !== epoch) return;
			setDetail({
				...value,
				comments: { entries: mergeEntries(value.comments.entries, next.entries), nextCursor: next.nextCursor },
			});
			pageCount++;
		} catch (cause) {
			if (currentEpoch === epoch) report(cause);
		} finally {
			if (currentEpoch === epoch) setPaging(false);
		}
	}

	async function act(action: WorkItemAction, label: string): Promise<boolean> {
		if (!reference || !detail() || pending() || paging()) return false;
		const currentEpoch = epoch;
		const target = reference;
		// Invalidate an in-flight refresh before applying any optimistic presentation.
		++request;
		setLoading(false);
		setPending(label);
		setPendingKind(action.kind);
		setError(undefined);
		try {
			const result = await source.act(target, action, controller.signal);
			if (currentEpoch !== epoch) return false;
			if (result.kind !== "item") throw new Error("The provider did not return an updated work item.");
			let value = result.value;
			try {
				value = await withOpenPages(value, source, target, controller.signal);
			} catch (cause) {
				// The mutation is committed even when its comment history cannot refresh.
				if (currentEpoch === epoch) {
					const message = cause instanceof Error ? cause.message : String(cause);
					setError(`The change was saved, but comment history could not refresh: ${message}`);
				}
			}
			if (currentEpoch !== epoch) return false;
			accept(value, action.kind === "comment");
			return true;
		} catch (cause) {
			if (currentEpoch === epoch) report(cause);
			return false;
		} finally {
			// Authoritative state was never guessed: removing the pending label rolls it back.
			if (currentEpoch === epoch) {
				setPending(undefined);
				setPendingKind(undefined);
			}
		}
	}

	createEffect(() => {
		const nextProvider = props.provider;
		identity();
		untrack(() => {
			controller.abort();
			controller = new AbortController();
			++epoch;
			++request;
			source = nextProvider;
			reference = props.reference;
			pageCount = 1;
			setDetail(undefined);
			setLoading(false);
			setPaging(false);
			setPending(undefined);
			setPendingKind(undefined);
			setError(undefined);
			void refresh();
		});
	});
	// The provider pages ordinary comments, reviews, then threads. Deep links advance
	// to the first thread page; later pages stay behind the existing Load more action.
	createEffect(() => {
		const value = detail();
		if (
			props.focusComments &&
			active() &&
			value?.item.itemType === "pull" &&
			value.comments.nextCursor &&
			!value.comments.entries.some((entry) => entry.kind === "review_thread") &&
			!loading() &&
			!paging() &&
			!pending() &&
			!error()
		) {
			void loadMore();
		}
	});
	createEffect(() => {
		if (!active()) return;
		untrack(() => {
			void refresh();
		});
		const timer = setInterval(() => {
			void refresh();
		}, 60_000);
		onCleanup(() => clearInterval(timer));
	});
	const visibilityChanged = () => setDocumentVisible(document.visibilityState !== "hidden");
	document.addEventListener("visibilitychange", visibilityChanged);
	onCleanup(() => {
		++epoch;
		controller.abort();
		document.removeEventListener("visibilitychange", visibilityChanged);
	});
	return { detail, identity, loading, paging, pending, pendingKind, error, refresh, loadMore, act };
}

function mergeEntries(previous: WorkItemCommentEntry[], next: WorkItemCommentEntry[]): WorkItemCommentEntry[] {
	const entries = new Map(previous.map((entry) => [commentKey(entry), entry]));
	for (const entry of next) entries.set(commentKey(entry), entry);
	return [...entries.values()];
}
