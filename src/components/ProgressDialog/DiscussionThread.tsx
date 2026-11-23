import { type Component, createResource, createSignal, For, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { discussionStore } from "../../stores/discussion";
import { toastsStore } from "../../stores/toasts";
import { Button } from "../ui/Button";
import { ContentRenderer } from "../ui/ContentRenderer";
import { Textarea } from "../ui/Textarea";
import s from "./Discussion.module.css";

export interface DiscussionThreadProps {
	/** Any question or answer entry of the thread. */
	entryId: number;
	/** Changes whenever the journal list changes, so a landed answer is re-read. */
	revision: unknown;
}

function report(title: string, error: unknown): void {
	const message = error instanceof Error ? error.message : String(error);
	appLogger.warn("discussion", title, { error: message });
	toastsStore.add(title, message, "error");
}

/** A Discussion thread opened inline under its journal entry. */
export const DiscussionThread: Component<DiscussionThreadProps> = (props) => {
	const [resource] = createResource(
		() => ({ id: props.entryId, revision: props.revision }),
		({ id }) => discussionStore.get(id),
	);
	/** Reading an errored resource rethrows, so every read goes through this guard. */
	const thread = () => (resource.error ? undefined : resource());
	const [followUp, setFollowUp] = createSignal("");
	const lastAnswer = () => {
		const turns = thread()?.turns ?? [];
		return [...turns].reverse().find((turn) => turn.answer)?.answer ?? null;
	};
	const lastCategory = () => thread()?.turns.at(-1)?.category ?? null;

	const send = async () => {
		const text = followUp().trim();
		if (!text || thread()?.running) return;
		try {
			await discussionStore.followUp(props.entryId, text);
			setFollowUp("");
		} catch (error) {
			report(t("discussion.followUpFailed", "Cannot send the follow-up"), error);
		}
	};

	/** Queue the answer for the main agent's next idle window: it is usually busy. */
	const sendToAgent = async () => {
		const answer = lastAnswer();
		const ptyId = thread()?.ptyId;
		if (!answer || !ptyId) return;
		try {
			await invoke("enqueue_agent_command", { sessionId: ptyId, text: answer });
			toastsStore.add(t("discussion.queued", "Queued for the agent"), "", "info");
		} catch (error) {
			report(t("discussion.queueFailed", "Cannot queue for the agent"), error);
		}
	};

	return (
		<div class={s.thread}>
			<Show when={resource.error}>
				<p class={s.error}>{String(resource.error)}</p>
			</Show>
			<For each={thread()?.turns ?? []}>
				{(turn) => (
					<div class={s.turn}>
						<div class={s.question}>{turn.question}</div>
						<Show when={turn.answer}>
							<ContentRenderer content={turn.answer!} untrustedContent />
						</Show>
						<Show when={turn.error}>
							<div class={s.error}>{turn.error}</div>
						</Show>
						<Show when={!turn.finishedMs}>
							<div class={s.hint}>{t("discussion.running", "answering…")}</div>
						</Show>
					</div>
				)}
			</For>
			<div class={s.actions}>
				<Show when={thread()?.running}>
					<Button
						onClick={() =>
							void discussionStore
								.cancel(props.entryId)
								.catch((e) => report(t("discussion.cancelFailed", "Cannot cancel"), e))
						}
					>
						{t("discussion.cancel", "Cancel")}
					</Button>
				</Show>
				<Show when={lastAnswer()}>
					<Button onClick={() => void navigator.clipboard.writeText(lastAnswer()!)}>
						{t("discussion.copy", "Copy")}
					</Button>
				</Show>
				<Show when={lastCategory() === "action" && lastAnswer()}>
					<Button onClick={() => void sendToAgent()}>{t("discussion.sendToAgent", "Send to agent")}</Button>
				</Show>
				<Show when={thread()?.forkSessionId}>
					<span class={s.hint}>
						{thread() && discussionStore.isWarm(thread()!)
							? t("discussion.warm", "warm")
							: t("discussion.cold", "cold")}
					</span>
				</Show>
			</div>
			<Show when={thread()?.forkSessionId}>
				<div class={s.ask}>
					<Textarea
						autoGrow
						value={followUp()}
						disabled={thread()?.running}
						placeholder={t("discussion.followUpPlaceholder", "Follow-up question…")}
						onInput={(event) => setFollowUp(event.currentTarget.value)}
						onKeyDown={(event) => {
							if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
								event.preventDefault();
								void send();
							}
						}}
					/>
					<Button variant="primary" disabled={!followUp().trim() || thread()?.running} onClick={() => void send()}>
						{t("discussion.send", "Send")}
					</Button>
				</div>
			</Show>
		</div>
	);
};
