import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { terminalsStore } from "../../stores/terminals";
import { HttpRpcError } from "../../transport";
import { Button } from "../ui/Button";
import s from "./StoriesDialog.module.css";
import type { EditableStory } from "./StoryEditForm";

interface Transition {
	storyId: string;
	revision: number;
	command: string | Record<string, unknown>;
	actor: { type: "human" | "local_api" } | { type: "managed_session"; session_id: string };
}

/** The stable identity remains visible even after the claiming terminal closes. */
export function StoryMetadata(props: { project: string; story: EditableStory & { claimSession: string | null } }) {
	const [history, setHistory] = createSignal<Transition[]>([]);
	const [error, setError] = createSignal("");
	const [loading, setLoading] = createSignal(false);
	let request = 0;
	onCleanup(() => ++request);
	const sessionName = (id: string) =>
		Object.values(terminalsStore.state.terminals).find(
			(terminal) => terminal.tuicSession === id || terminal.sessionId === id,
		)?.name;
	async function refresh() {
		const current = ++request;
		const storyId = props.story.id;
		setHistory([]);
		setLoading(true);
		setError("");
		try {
			const reply = await invoke<{ type: string; value: Transition[] }>("story_action_command", {
				project: props.project,
				action: { action: "transition_history", story_id: storyId },
			});
			if (reply.type !== "transitions") throw new Error("Invalid transition history response");
			if (current === request) setHistory(reply.value);
		} catch (cause) {
			if (current === request)
				setError(cause instanceof HttpRpcError ? cause.detail : cause instanceof Error ? cause.message : String(cause));
		} finally {
			if (current === request) setLoading(false);
		}
	}
	createEffect(
		on(
			() => [props.project, props.story.id, props.story.revision],
			() => void refresh(),
		),
	);
	const originLabel = () => {
		switch (props.story.origin.type) {
			case "native":
				return t("stories.metadata.native", "Native");
			case "plan_step":
				return t("stories.metadata.planStep", "Plan step");
			case "git_hub_issue":
				return t("stories.metadata.issue", "GitHub issue #{number}", { number: String(props.story.origin.number) });
		}
	};
	return (
		<>
			<section aria-label={t("stories.metadata.title", "Story metadata")}>
				<dl class={s.metadata}>
					<dt>{t("stories.metadata.origin", "Origin")}</dt>
					<dd>{originLabel()}</dd>
					<Show when={props.story.origin.type === "plan_step" ? props.story.origin.step : null}>
						{(step) => (
							<>
								<dt>{t("stories.metadata.planStep", "Plan step")}</dt>
								<dd>{step()}</dd>
							</>
						)}
					</Show>
					<dt>{t("stories.metadata.revision", "Revision")}</dt>
					<dd>
						{t("stories.metadata.revision", "Revision")} {props.story.revision}
					</dd>
					<dt>{t("stories.metadata.claim", "Claiming session")}</dt>
					<dd>
						<Show when={props.story.claimSession} fallback={t("stories.metadata.unclaimed", "Unclaimed")}>
							{(id) => (
								<>
									<Show when={sessionName(id())}>{(name) => <span>{name()}</span>}</Show>
									<code class={s.sessionId}>{id()}</code>
								</>
							)}
						</Show>
					</dd>
				</dl>
			</section>
			<section aria-label={t("stories.metadata.history", "Transition history")} aria-busy={loading()}>
				<div class={s.detailHeader}>
					<h4>{t("stories.metadata.history", "Transition history")}</h4>
					<Button type="button" class={s.loadMore} disabled={loading()} onClick={() => void refresh()}>
						{t("stories.metadata.refresh", "Refresh history")}
					</Button>
				</div>
				<Show when={loading()}>
					<p class={s.muted} role="status">
						{t("stories.metadata.loading", "Loading history…")}
					</p>
				</Show>
				<Show when={error()}>
					<p class={s.editError} role="alert">
						{error()}
					</p>
				</Show>
				<Show when={!loading() && !error() && history().length === 0}>
					<p class={s.muted}>{t("stories.metadata.empty", "No transitions recorded.")}</p>
				</Show>
				<ol class={s.transitionHistory}>
					<For each={history()}>
						{(entry) => (
							<li>
								<span>
									{t("stories.metadata.revision", "Revision")} {entry.revision} ·{" "}
								</span>
								<code>{typeof entry.command === "string" ? entry.command : JSON.stringify(entry.command)}</code>
								<span>
									{" "}
									·{" "}
									{entry.actor.type === "managed_session"
										? `${sessionName(entry.actor.session_id) ?? ""} ${entry.actor.session_id}`.trim()
										: entry.actor.type === "human"
											? t("stories.metadata.human", "Human")
											: t("stories.metadata.localApi", "Local API")}
								</span>
							</li>
						)}
					</For>
				</ol>
			</section>
		</>
	);
}
