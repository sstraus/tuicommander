import { type Component, createEffect, createSignal, on, Show } from "solid-js";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { discussionStore } from "../../stores/discussion";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./Discussion.module.css";

/** Agents with a fork adapter in the backend (`discussion::resolve_target`). */
const FORKABLE_AGENTS = new Set(["claude", "codex", "ego"]);

/** Why a terminal cannot take a side question, or null when it can. */
function unavailableReason(sessionId: string): string | null {
	const terminal = Object.values(terminalsStore.state.terminals).find((term) => term.sessionId === sessionId);
	if (!terminal) return t("discussion.noTerminal", "The terminal is closed.");
	if (!terminal.agentType || !FORKABLE_AGENTS.has(terminal.agentType))
		return t("discussion.unsupportedAgent", "Side questions support Claude, Codex and ego terminals for now.");
	if (terminal.isRemote) return t("discussion.remote", "Side questions are not available for remote terminals.");
	return null;
}

/** Ask the agent in the selected terminal a side question, answered in a fork. */
export const DiscussionAsk: Component<{ sessionId: string }> = (props) => {
	const [question, setQuestion] = createSignal("");
	const [sending, setSending] = createSignal(false);
	let input: HTMLTextAreaElement | undefined;
	createEffect(on(discussionStore.askFocus, () => input?.focus(), { defer: true }));

	const ask = async () => {
		const text = question().trim();
		if (!text || sending()) return;
		setSending(true);
		try {
			await discussionStore.ask(props.sessionId, text);
			setQuestion("");
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			appLogger.warn("discussion", "Side question refused", { error: message });
			toastsStore.add(t("discussion.askFailed", "Cannot ask the side question"), message, "error");
		} finally {
			setSending(false);
		}
	};

	return (
		<Show
			when={!unavailableReason(props.sessionId)}
			fallback={<p class={s.hint}>{unavailableReason(props.sessionId)}</p>}
		>
			<div class={s.ask}>
				<Textarea
					ref={input}
					autoGrow
					value={question()}
					placeholder={t("discussion.placeholder", "Ask about what the agent is doing…")}
					onInput={(event) => setQuestion(event.currentTarget.value)}
					onKeyDown={(event) => {
						if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
							event.preventDefault();
							void ask();
						}
					}}
				/>
				<Button variant="primary" disabled={!question().trim() || sending()} onClick={() => void ask()}>
					{t("discussion.ask", "Ask")}
				</Button>
			</div>
		</Show>
	);
};
