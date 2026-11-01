import { createEffect, createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { Button } from "../../components/ui";
import type { QueuedCommand } from "../../hooks/usePty";
import { HttpRpcError, rpc } from "../../transport";
import styles from "./QueuedPrompts.module.css";

/** A view of the existing PTY FIFO, not a second client-side queue. */
export function QueuedPrompts(props: { sessionId: string; count: number; revision: number }) {
	let read: AbortController | undefined;
	onCleanup(() => read?.abort());
	const [pending, setPending] = createSignal(false);
	const [hidden, setHidden] = createSignal(false);
	const [actionError, setActionError] = createSignal<string | null>(null);
	const [snapshot, { refetch }] = createResource(
		() => ({ sessionId: props.sessionId, count: props.count, revision: props.revision }),
		async ({ sessionId, count, revision }) => {
			read?.abort();
			read = new AbortController();
			try {
				// A fresh read must not share an older pre-mutation HTTP GET.
				return {
					items: await rpc<QueuedCommand[]>("list_queued_agent_commands", { sessionId }, undefined, read.signal),
					error: null,
				};
			} catch (error) {
				// Older hosts may not expose queue reads. With no pending count or
				// accepted local enqueue, that optional capability is not a session error.
				if (error instanceof HttpRpcError && error.status === 404 && count === 0 && revision === 0) {
					return { items: [], error: null };
				}
				return { items: [], error: "Could not load queued prompts." };
			}
		},
	);
	// Origin is supplied by the host; never infer ownership from prompt text.
	const items = () => snapshot()?.items.filter((item) => item.kind === "user_command") ?? [];
	createEffect(() => {
		const current = snapshot();
		// Reset only from a successful empty snapshot: the lifecycle count can
		// still be zero just after this client has accepted a queued prompt.
		if (current && !current.error && items().length === 0) setHidden(false);
	});
	async function remove(commandId?: number) {
		if (pending()) return;
		const sessionId = props.sessionId;
		setPending(true);
		setActionError(null);
		try {
			// Clear only visible stable IDs; the full FIFO also holds hidden mail.
			const commandIds = commandId === undefined ? items().map((item) => item.id) : [commandId];
			for (const id of commandIds) {
				await rpc("remove_queued_agent_command", { sessionId, commandId: id });
			}
		} catch {
			setActionError("Could not remove queued prompts. Check the queue before retrying.");
		} finally {
			await refetch();
			setPending(false);
		}
	}
	return (
		<Show when={items().length > 0 || snapshot()?.error || actionError()}>
			<section class={styles.queue} aria-label="Queued prompts">
				<div class={styles.header}>
					<Button
						variant="hint"
						aria-label={hidden() ? "Show queued prompts" : "Hide queued prompts"}
						aria-expanded={!hidden()}
						onClick={() => setHidden(!hidden())}
					>
						<span role="status">
							Queued<Show when={!snapshot()?.error}> · {items().length}</Show>
						</span>
						{hidden() ? "Show" : "Hide"}
					</Button>
					<Show when={!hidden() && items().length > 0}>
						<Button variant="ghost" disabled={pending()} onClick={() => void remove()}>
							Clear all
						</Button>
					</Show>
				</div>
				<Show when={snapshot()?.error || actionError()}>
					<div role="alert">
						{actionError() || snapshot()?.error}{" "}
						<Button variant="ghost" onClick={() => void refetch()}>
							Retry
						</Button>
					</div>
				</Show>
				<Show when={!hidden()}>
					<div class={styles.items}>
						<For each={items()}>
							{(item) => (
								<div class={styles.row}>
									<div class={styles.text}>{item.text}</div>
									<Button
										variant="ghost"
										disabled={pending()}
										aria-label={`Cancel queued prompt ${item.text}`}
										onClick={() => void remove(item.id)}
									>
										Cancel
									</Button>
								</div>
							)}
						</For>
					</div>
				</Show>
			</section>
		</Show>
	);
}
