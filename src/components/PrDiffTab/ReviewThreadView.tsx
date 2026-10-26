import { type Component, createSignal, For, Show } from "solid-js";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { hidesOriginal, OriginalComment, ReviewFindingView } from "../shared/ReviewFindingView";
import { TaskMarkdown } from "../Tasks/detail/TaskMarkdown";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./ReviewThreads.module.css";
import type { ReviewThread } from "./reviewThreads";
import type { ReviewSession } from "./useReviewSession";

/** One GitHub review thread: replies, resolve/unresolve, reply box. State changes only after the backend confirms. */
export const ReviewThreadView: Component<{ thread: ReviewThread; session: ReviewSession }> = (props) => {
	const [draft, setDraft] = createSignal("");
	const busy = () => props.session.busyThread() === props.thread.id;
	const line = () => props.thread.line ?? props.thread.originalLine;
	const reply = async () => {
		const body = draft().trim();
		if (!body) return;
		if (await props.session.reply(props.thread.id, body)) setDraft("");
	};
	return (
		<section
			class={cx(s.thread, props.thread.isResolved && s.resolved, props.thread.isOutdated && s.outdated)}
			aria-label={t("prDiff.thread", "Review thread")}
			data-thread-id={props.thread.id}
		>
			<header class={s.threadHeader}>
				<span class={s.threadPath}>
					{props.thread.path}
					{line() !== null ? `:${line()}` : ""}
				</span>
				<Show when={props.thread.isOutdated}>
					<span class={cx(s.badge, s.badgeOutdated)}>{t("prDiff.outdated", "Outdated")}</span>
				</Show>
				<span class={cx(s.badge, props.thread.isResolved ? s.badgeResolved : s.badgeOpen)}>
					{props.thread.isResolved ? t("prDiff.resolved", "Resolved") : t("prDiff.open", "Open")}
				</span>
				<Button
					type="button"
					disabled={busy()}
					onClick={() => void props.session.setResolved(props.thread.id, !props.thread.isResolved)}
				>
					{props.thread.isResolved ? t("prDiff.unresolve", "Unresolve") : t("prDiff.resolve", "Resolve")}
				</Button>
			</header>
			<Show when={props.thread.finding}>{(finding) => <ReviewFindingView finding={finding()} />}</Show>
			<For each={props.thread.comments.nodes}>
				{(comment, index) => (
					<article class={s.comment}>
						<header class={s.commentHeader}>
							<strong>{comment.author?.login ?? t("prDiff.ghost", "ghost")}</strong>
							<Show when={comment.author?.bot}>
								<span class={cx(s.badge, s.badgeBot)}>{t("prDiff.bot", "bot")}</span>
							</Show>
							<time dateTime={comment.createdAt}>{new Date(comment.createdAt).toLocaleString()}</time>
						</header>
						<OriginalComment collapsed={index() === 0 && hidesOriginal(props.thread.finding)}>
							<TaskMarkdown content={comment.body} />
						</OriginalComment>
					</article>
				)}
			</For>
			<Show when={props.thread.comments.pageInfo.hasNextPage}>
				<Button type="button" disabled={busy()} onClick={() => void props.session.loadMoreComments(props.thread.id)}>
					{t("prDiff.moreReplies", "Load more replies")}
				</Button>
			</Show>
			<div class={s.composer}>
				<Textarea
					aria-label={t("prDiff.replyLabel", "Reply")}
					placeholder={t("prDiff.replyPlaceholder", "Reply…")}
					value={draft()}
					onInput={(e) => setDraft(e.currentTarget.value)}
				/>
				<Button type="button" variant="primary" disabled={busy() || !draft().trim()} onClick={() => void reply()}>
					{t("prDiff.reply", "Reply")}
				</Button>
			</div>
		</section>
	);
};
