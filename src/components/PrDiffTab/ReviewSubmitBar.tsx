import { type Component, createSignal, Show } from "solid-js";
import { t } from "../../i18n";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import s from "./ReviewThreads.module.css";
import type { ReviewEvent, ReviewSession } from "./useReviewSession";

/** Submits the draft review through the head-pinned backend command. */
export const ReviewSubmitBar: Component<{ session: ReviewSession }> = (props) => {
	const [body, setBody] = createSignal("");
	const [event, setEvent] = createSignal<ReviewEvent>("COMMENT");
	const count = () => props.session.drafts().length;
	// GitHub rejects REQUEST_CHANGES without a body, and an empty COMMENT review carries nothing.
	const blocked = () =>
		props.session.headMoved() ||
		props.session.submitting() ||
		(event() === "REQUEST_CHANGES" && !body().trim()) ||
		(event() === "COMMENT" && count() === 0 && !body().trim());
	return (
		<div class={s.submitBar}>
			<Show when={props.session.headMoved()}>
				<p class={s.alert} role="alert">
					{t("prDiff.headMoved", "The PR has new commits. Reopen the diff to review the current head.")}
				</p>
			</Show>
			<Show when={props.session.error() && !props.session.headMoved()}>
				<p class={s.alert} role="alert">
					{props.session.error()}
				</p>
			</Show>
			<div class={s.row}>
				<span class={s.pendingCount}>
					{count()} {t("prDiff.pendingComments", "pending comments")}
				</span>
				<Select
					aria-label={t("prDiff.reviewEvent", "Review type")}
					value={event()}
					onChange={(e) => setEvent(e.currentTarget.value as ReviewEvent)}
				>
					<option value="COMMENT">{t("prDiff.eventComment", "Comment")}</option>
					<option value="APPROVE">{t("prDiff.eventApprove", "Approve")}</option>
					<option value="REQUEST_CHANGES">{t("prDiff.eventRequest", "Request changes")}</option>
				</Select>
				<Input
					type="text"
					aria-label={t("prDiff.reviewBody", "Review summary")}
					placeholder={t("prDiff.reviewBody", "Review summary")}
					value={body()}
					onInput={(e) => setBody(e.currentTarget.value)}
				/>
				<Button
					type="button"
					variant="primary"
					disabled={blocked()}
					onClick={async () => {
						if (await props.session.submit(body().trim(), event())) setBody("");
					}}
				>
					{t("prDiff.submitReview", "Submit review")}
				</Button>
			</div>
		</div>
	);
};
