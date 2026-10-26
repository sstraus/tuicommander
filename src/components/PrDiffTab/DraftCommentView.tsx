import { type Component, createSignal } from "solid-js";
import { t } from "../../i18n";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./ReviewThreads.module.css";
import type { DraftComment } from "./reviewThreads";

/** A pending comment of the draft review. Not on GitHub until the review is submitted. */
export const DraftCommentView: Component<{ draft: DraftComment; onRemove: () => void }> = (props) => (
	<section class={s.draft} aria-label={t("prDiff.pending", "Pending comment")}>
		<header class={s.threadHeader}>
			<span class={s.threadPath}>
				{props.draft.path}:{props.draft.line}
			</span>
			<span class={s.badge}>{t("prDiff.pending", "Pending comment")}</span>
			<Button type="button" onClick={props.onRemove}>
				{t("prDiff.discard", "Discard")}
			</Button>
		</header>
		<p class={s.draftBody}>{props.draft.body}</p>
	</section>
);

/** Opened by the diff's "+" gutter button: collects the text of a new pending comment. */
export const NewDraftForm: Component<{ disabled: boolean; onAdd: (body: string) => void; onClose: () => void }> = (
	props,
) => {
	const [body, setBody] = createSignal("");
	return (
		<div class={s.composer}>
			<Textarea
				aria-label={t("prDiff.commentLabel", "Line comment")}
				placeholder={t("prDiff.commentPlaceholder", "Comment on this line…")}
				value={body()}
				onInput={(e) => setBody(e.currentTarget.value)}
			/>
			<div class={s.row}>
				<Button type="button" onClick={props.onClose}>
					{t("prDiff.cancel", "Cancel")}
				</Button>
				<Button
					type="button"
					variant="primary"
					disabled={props.disabled || !body().trim()}
					onClick={() => {
						props.onAdd(body().trim());
						props.onClose();
					}}
				>
					{t("prDiff.addToReview", "Add to review")}
				</Button>
			</div>
		</div>
	);
};
