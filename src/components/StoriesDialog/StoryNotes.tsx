import { createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { HttpRpcError } from "../../transport";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./StoryNotes.module.css";

interface StoryNote {
	id: string;
	storyId: string;
	storyRevision: number;
	text: string;
	actor: { type: "human" | "local_api" } | { type: "managed_session"; session_id: string };
	createdAt: number;
	receipts: { runId: string; commandId: string }[];
}

const errorMessage = (cause: unknown) =>
	cause instanceof HttpRpcError ? cause.detail : cause instanceof Error ? cause.message : String(cause);

/** Mounted with a story key so drafts and pending replies never cross subjects. */
export function StoryNotes(props: { project: string; storyId: string; revision: number; busy: boolean }) {
	const [draft, setDraft] = createSignal("");
	const [saving, setSaving] = createSignal(false);
	const [saveError, setSaveError] = createSignal("");
	const [saved, setSaved] = createSignal(false);
	let disposed = false;
	onCleanup(() => {
		disposed = true;
	});
	const call = (action: Record<string, unknown>) =>
		invoke<{ type: "notes"; value: StoryNote[] } | { type: "note"; value: StoryNote }>("story_action_command", {
			project: props.project,
			action,
		});
	const [result, { refetch }] = createResource(async () => {
		try {
			const reply = await call({ action: "list_notes", story_id: props.storyId });
			if (reply.type !== "notes") throw new Error("Invalid story notes response");
			return { notes: reply.value, error: "" };
		} catch (cause) {
			appLogger.warn("store", "Stories: notes load failed", { error: String(cause) });
			return { notes: [] as StoryNote[], error: errorMessage(cause) };
		}
	});

	async function addNote(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		if (saving() || props.busy || !draft().trim()) return;
		setSaving(true);
		setSaveError("");
		setSaved(false);
		try {
			const reply = await call({
				action: "add_note",
				story_id: props.storyId,
				expected_revision: props.revision,
				text: draft(),
			});
			if (reply.type !== "note") throw new Error("Invalid story note response");
			if (disposed) return;
			setDraft("");
			setSaved(true);
			await refetch();
		} catch (cause) {
			if (disposed) return;
			appLogger.warn("store", "Stories: note append failed", { error: String(cause) });
			setSaveError(errorMessage(cause));
		} finally {
			if (!disposed) setSaving(false);
		}
	}

	const actorLabel = (note: StoryNote) =>
		note.actor.type === "managed_session"
			? t("stories.notes.managed", "Managed session {id}", { id: note.actor.session_id })
			: note.actor.type === "local_api"
				? t("stories.notes.localApi", "Local API")
				: t("stories.notes.human", "Human");

	return (
		<section class={s.notes} aria-label={t("stories.notes.heading", "Notes and evidence")}>
			<div class={s.header}>
				<h4>{t("stories.notes.heading", "Notes and evidence")}</h4>
				<Button
					type="button"
					onClick={() => void refetch()}
					disabled={result.loading || saving()}
					aria-label={t("stories.notes.refresh", "Refresh notes")}
				>
					{t("stories.notes.refresh", "Refresh notes")}
				</Button>
			</div>
			<Show when={result.loading}>
				<p class={s.muted} role="status">
					{t("stories.notes.loading", "Loading notes…")}
				</p>
			</Show>
			<Show when={result()?.error}>
				<p class={s.error} role="alert">
					{result()?.error}
				</p>
			</Show>
			<Show when={!result.loading && !result()?.error && result()?.notes.length === 0}>
				<p class={s.muted}>{t("stories.notes.empty", "No notes yet.")}</p>
			</Show>
			<ol class={s.list}>
				<For each={result()?.notes}>
					{(note) => (
						<li class={s.note}>
							<div class={s.metadata}>
								<span>{actorLabel(note)}</span>
								<time dateTime={new Date(note.createdAt).toISOString()}>
									{new Date(note.createdAt).toLocaleString()}
								</time>
								<span>
									{t("stories.notes.revision", "Revision {revision}", { revision: String(note.storyRevision) })}
								</span>
							</div>
							<p class={s.text}>{note.text}</p>
							<Show when={note.receipts.length}>
								<ul class={s.receipts} aria-label={t("stories.notes.receipts", "Workflow receipt references")}>
									<For each={note.receipts}>
										{(receipt) => (
											<li>
												<code>
													{receipt.runId} / {receipt.commandId}
												</code>
											</li>
										)}
									</For>
								</ul>
							</Show>
						</li>
					)}
				</For>
			</ol>
			<form class={s.form} onSubmit={(event) => void addNote(event)}>
				<label>
					{t("stories.notes.add", "Add note")}
					<Textarea
						value={draft()}
						rows={3}
						disabled={saving() || props.busy}
						onInput={(event) => {
							setDraft(event.currentTarget.value);
							setSaved(false);
						}}
					/>
				</label>
				<p class={s.muted}>
					{t("stories.notes.immutable", "Notes cannot be edited or removed. Up to 16,000 characters.")}
				</p>
				<Show when={saveError()}>
					<p class={s.error} role="alert">
						{saveError()}
					</p>
				</Show>
				<Show when={saved()}>
					<p class={s.muted} role="status">
						{t("stories.notes.saved", "Note added.")}
					</p>
				</Show>
				<Button variant="primary" type="submit" disabled={saving() || props.busy || !draft().trim()}>
					{t("stories.notes.submit", "Append note")}
				</Button>
			</form>
		</section>
	);
}
