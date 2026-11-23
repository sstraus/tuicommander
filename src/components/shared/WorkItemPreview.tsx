import { createResource, createSignal, For, type JSX, Show } from "solid-js";
import { t } from "../../i18n";
import type { WorkItemProvider } from "../../plugins/workItemProvider";
import { resolveWorkItem } from "../../services/taskNavigation";
import type { WorkItemAction, WorkItemCommentEntry, WorkItemDetail, WorkItemRef } from "../../types/work-items.gen";
import { TaskComment } from "../Tasks/detail/TaskComment";
import { TaskMarkdown } from "../Tasks/detail/TaskMarkdown";
import { ReplyBox, ThreadActions } from "./PreviewReply";
import s from "./WorkItemPreview.module.css";
import { isNewSince } from "./workItemVisit";

type PreviewResult = { detail: WorkItemDetail } | { unavailable: true } | { error: string };

export interface WorkItemPreview {
	result: () => PreviewResult | undefined;
	loading: () => boolean;
	/** Starts the first fetch; call it when a section that needs the data opens. */
	load: () => void;
	/** Fetches the item again, after a change made outside the provider (a thread reply). */
	reload: () => void;
	/** True when the provider offers `kind` on this item (comment, edit_field). */
	offers: (kind: WorkItemAction["kind"]) => boolean;
	/** Runs a provider action and shows the item it returns; rejects with the provider's error. */
	act: (action: WorkItemAction) => Promise<void>;
}

/** Description and comments of a PR or issue, read through the registered work-item provider
 *  that the Tasks detail uses. Nothing is fetched until a section asks for it. */
export function createWorkItemPreview(url: () => string | undefined): WorkItemPreview {
	const [wanted, setWanted] = createSignal(false);
	let resolved: { reference: WorkItemRef; provider: WorkItemProvider } | null = null;
	const [result, { mutate, refetch }] = createResource(
		() => (wanted() && url()) || null,
		async (target): Promise<PreviewResult> => {
			try {
				const signal = new AbortController().signal;
				resolved = await resolveWorkItem(target, signal);
				if (!resolved) return { unavailable: true };
				return { detail: await resolved.provider.detail(resolved.reference, signal) };
			} catch (cause) {
				return { error: cause instanceof Error ? cause.message : String(cause) };
			}
		},
	);
	const detail = () => {
		const value = result();
		return value && "detail" in value ? value.detail : undefined;
	};
	return {
		result: () => result(),
		loading: () => result.loading,
		load: () => setWanted(true),
		reload: () => void refetch(),
		offers: (kind) => detail()?.actions.some((action) => action.kind === kind && action.enabled) ?? false,
		act: async (action) => {
			if (!resolved) throw new Error(t("preview.unavailable", "Connect GitHub in Tasks to read it here."));
			const reply = await resolved.provider.act(resolved.reference, action, new AbortController().signal);
			if (reply.kind === "item") mutate({ detail: reply.value });
		},
	};
}

function PreviewState(props: { preview: WorkItemPreview; children: (detail: WorkItemDetail) => JSX.Element }) {
	const detail = () => {
		const value = props.preview.result();
		return value && "detail" in value ? value.detail : undefined;
	};
	return (
		<Show
			when={detail()}
			fallback={
				<div class={s.muted}>
					<Show when={props.preview.loading()}>{t("preview.loading", "Loading…")}</Show>
					<Show
						when={(() => {
							const value = props.preview.result();
							return value && "error" in value ? value.error : undefined;
						})()}
					>
						{(error) => <span class={s.error}>{error()}</span>}
					</Show>
					<Show
						when={(() => {
							const value = props.preview.result();
							return value && "unavailable" in value;
						})()}
					>
						{t("preview.unavailable", "Connect GitHub in Tasks to read it here.")}
					</Show>
				</div>
			}
		>
			{(value) => <>{props.children(value())}</>}
		</Show>
	);
}

/** The item's Markdown description. */
export function PreviewDescription(props: { preview: WorkItemPreview }) {
	return (
		<PreviewState preview={props.preview}>
			{(detail) => (
				<Show
					when={detail.body.trim()}
					fallback={<div class={s.muted}>{t("preview.noDescription", "No description.")}</div>}
				>
					<div class={s.markdown}>
						<TaskMarkdown content={detail.body} />
					</div>
				</Show>
			)}
		</PreviewState>
	);
}

/** The description's length for a section summary ("9 lines"), once the item is loaded. */
export function descriptionSummary(preview: WorkItemPreview): string | undefined {
	const value = preview.result();
	if (!value || !("detail" in value)) return undefined;
	const lines = value.detail.body.trim() ? value.detail.body.trim().split("\n").length : 0;
	return t("preview.lines", "{count} lines", { count: String(lines) });
}

/** Comments of the item that pass `filter`, newest last, with a link to the full conversation.
 *  `reply` adds a composer for a new comment; `threadRepo` adds Reply and Resolve to review threads. */
export function PreviewComments(props: {
	preview: WorkItemPreview;
	filter: (entry: WorkItemCommentEntry) => boolean;
	empty: string;
	onOpenAll: () => void;
	reply?: boolean;
	threadRepo?: string;
	/** Previous visit time: comments created after it carry a "new" mark. */
	newSince?: number | null;
}) {
	return (
		<PreviewState preview={props.preview}>
			{(detail) => {
				const entries = () => detail.comments.entries.filter(props.filter);
				return (
					<div class={s.comments} data-compact>
						<Show when={entries().length > 0} fallback={<div class={s.muted}>{props.empty}</div>}>
							<For each={entries()}>
								{(entry) => (
									<div class={s.entry}>
										<Show when={entry.kind === "normal" && isNewSince(entry.value.createdAt, props.newSince ?? null)}>
											<span class={s.newMark}>{t("preview.new", "New")}</span>
										</Show>
										<TaskComment entry={entry} />
										<Show when={props.threadRepo && entry.kind === "review_thread" ? entry.value : undefined}>
											{(thread) => (
												<ThreadActions
													repoPath={props.threadRepo ?? ""}
													threadId={thread().id}
													preview={props.preview}
												/>
											)}
										</Show>
									</div>
								)}
							</For>
						</Show>
						<Show when={props.reply && props.preview.offers("comment")}>
							<ReplyBox
								label={t("preview.reply", "Reply")}
								onSend={(body) => props.preview.act({ kind: "comment", value: body })}
							/>
						</Show>
						<button type="button" class={s.openAll} data-control-layout="row" onClick={props.onOpenAll}>
							{t("preview.openAll", "Open the conversation in Tasks ↗")}
						</button>
					</div>
				);
			}}
		</PreviewState>
	);
}
