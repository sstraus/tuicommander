import { createSignal, type JSX, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import type { WorkItemPreview } from "./WorkItemPreview";
import s from "./WorkItemPreview.module.css";

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

/** A `label` button that opens a small composer; Send runs `onSend` and closes it on success. */
export function ReplyBox(props: {
	label: string;
	onSend: (body: string) => Promise<void>;
	variant?: "primary" | "secondary";
	icon?: JSX.Element;
}) {
	const [open, setOpen] = createSignal(false);
	const [body, setBody] = createSignal("");
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const send = async () => {
		if (!body().trim() || busy()) return;
		setBusy(true);
		setError("");
		try {
			await props.onSend(body());
			setBody("");
			setOpen(false);
		} catch (cause) {
			setError(message(cause));
		} finally {
			setBusy(false);
		}
	};
	return (
		<Show
			when={open()}
			fallback={
				<Button size="compact" variant={props.variant ?? "secondary"} onClick={() => setOpen(true)}>
					{props.icon}
					{props.label}
				</Button>
			}
		>
			<div class={s.reply}>
				<Textarea
					value={body()}
					rows={3}
					placeholder={t("preview.replyPlaceholder", "Write a reply")}
					onInput={(event) => setBody(event.currentTarget.value)}
				/>
				<Show when={error()}>
					<span class={s.error}>{error()}</span>
				</Show>
				<div class={s.replyActions}>
					<Button size="compact" variant="primary" disabled={busy() || !body().trim()} onClick={() => void send()}>
						{busy() ? "..." : t("preview.send", "Send")}
					</Button>
					<Button size="compact" variant="ghost" disabled={busy()} onClick={() => setOpen(false)}>
						{t("preview.cancel", "Cancel")}
					</Button>
				</div>
			</div>
		</Show>
	);
}

/** Reply and Resolve under an open review thread; both reload the item afterwards. */
export function ThreadActions(props: { repoPath: string; threadId: string; preview: WorkItemPreview }) {
	const [resolving, setResolving] = createSignal(false);
	const [error, setError] = createSignal("");
	const resolve = async () => {
		setResolving(true);
		setError("");
		try {
			await invoke("set_pr_thread_resolved", { repoPath: props.repoPath, threadId: props.threadId, resolved: true });
			props.preview.reload();
		} catch (cause) {
			setError(message(cause));
		} finally {
			setResolving(false);
		}
	};
	return (
		<div class={s.threadActions}>
			<ReplyBox
				label={t("preview.reply", "Reply")}
				onSend={async (body) => {
					await invoke("reply_pr_thread", { repoPath: props.repoPath, threadId: props.threadId, body });
					props.preview.reload();
				}}
			/>
			<Button size="compact" variant="secondary" disabled={resolving()} onClick={() => void resolve()}>
				{resolving() ? "..." : t("preview.resolve", "Resolve")}
			</Button>
			<Show when={error()}>
				<span class={s.error}>{error()}</span>
			</Show>
		</div>
	);
}
