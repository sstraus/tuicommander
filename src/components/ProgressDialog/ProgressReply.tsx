import { type Component, createEffect, type JSX, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import type { ProgressEntry } from "../../stores/progress";
import { progressRepliesStore } from "../../stores/progressReplies";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./ProgressReply.module.css";

/** One origin-bound reply UI shared by transient Progress cards and the journal. */
export const ProgressReply: Component<{
	entry: ProgressEntry;
	children?: JSX.Element;
	part?: "controls" | "composer";
	onActiveChange?: (active: boolean) => void;
}> = (props) => {
	const reply = () => progressRepliesStore.get(props.entry);
	const binary = () => props.entry.question?.kind === "yes_no";
	const answered = () => binary() && (props.entry.answered || !!reply().receipt);
	const available = () =>
		(props.entry.type === "done" || props.entry.type === "blocked") && props.entry.replyTargetAvailable === true;
	createEffect(() => props.onActiveChange?.(reply().expanded || reply().pending));
	onCleanup(() => props.onActiveChange?.(false));
	const send = (answer?: boolean) => void progressRepliesStore.send(props.entry, answer);
	return (
		<Show
			when={
				available() &&
				!(props.part === "controls" && answered()) &&
				(props.part !== "composer" ||
					reply().expanded ||
					reply().pending ||
					reply().error ||
					reply().receipt ||
					answered())
			}
			fallback={props.children}
		>
			<div
				class={s.reply}
				data-progress-reply={props.part ?? "all"}
				onClick={(event) => {
					if (reply().expanded || reply().pending || (event.target as HTMLElement).closest("button, textarea"))
						event.stopPropagation();
				}}
				onKeyDown={(event) => {
					if (reply().expanded || reply().pending || (event.target as HTMLElement).closest("button, textarea"))
						event.stopPropagation();
				}}
			>
				{props.children}
				<Show when={!answered() && props.part !== "composer"}>
					<Show
						when={binary()}
						fallback={
							<Button
								type="button"
								size="compact"
								aria-expanded={reply().expanded}
								disabled={reply().pending}
								onClick={() => progressRepliesStore.expand(props.entry, !reply().expanded)}
							>
								{t("progress.reply.open", "Reply")}
							</Button>
						}
					>
						<div class={s.actions}>
							<Button type="button" size="compact" disabled={reply().pending} onClick={() => send(true)}>
								{t("progress.reply.yes", "Yes")}
							</Button>
							<Button type="button" size="compact" disabled={reply().pending} onClick={() => send(false)}>
								{t("progress.reply.no", "No")}
							</Button>
						</div>
					</Show>
				</Show>
				<Show when={!answered() && props.part !== "controls" && !binary() && reply().expanded}>
					<div class={s.composer}>
						<Textarea
							autoGrow
							ref={(node) =>
								queueMicrotask(() => {
									if (node.isConnected) node.focus();
								})
							}
							aria-label={t("progress.reply.comment", "Reply to this progress")}
							value={reply().draft}
							disabled={reply().pending}
							onInput={(event) => progressRepliesStore.setDraft(props.entry, event.currentTarget.value)}
							onKeyDown={(event) => {
								if (event.key === "Enter" && !event.shiftKey && !event.isComposing && event.keyCode !== 229) {
									event.preventDefault();
									send();
								}
							}}
						/>
						<div class={s.actions}>
							<Button
								type="button"
								size="compact"
								disabled={reply().pending || !reply().draft.trim()}
								onClick={() => send()}
							>
								{t("progress.reply.send", "Send")}
							</Button>
							<span class={s.hint}>{t("progress.reply.hint", "Enter to send · Shift+Enter for a new line")}</span>
						</div>
					</div>
				</Show>
				<Show when={props.part !== "controls"}>
					<Show when={reply().pending}>
						<span role="status">{t("progress.reply.sending", "Sending…")}</span>
					</Show>
					<Show when={reply().error}>
						{(error) => (
							<p class={s.error} role="alert">
								{error()}
							</p>
						)}
					</Show>
					<Show when={!reply().pending && (reply().receipt || answered())}>
						<span role="status">
							{reply().receipt?.delivery === "queued"
								? t("progress.reply.queued", "Reply queued for the agent")
								: reply().receipt
									? t("progress.reply.sent", "Reply sent")
									: t("progress.reply.answered", "Answered")}
						</span>
					</Show>
				</Show>
			</div>
		</Show>
	);
};
