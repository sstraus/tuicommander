import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { rpc } from "../../transport";
import styles from "../screens/SessionDetailScreen.module.css";

/** Rust owns selection, units, cost semantics and display formatting. */
interface SessionUsage {
	availability: "available" | "unavailable";
	model: string | null;
	reason: string | null;
	stale: boolean;
	lines: { label: string; value: string }[];
}

/** Mounted only while the overflow is open; no inventory polling or sticky ticker state. */
export function SessionUsageBlock(props: { sessionId: string }) {
	const [usage, setUsage] = createSignal<SessionUsage | null>(null);
	const [loading, setLoading] = createSignal(true);
	createEffect(() => {
		const sessionId = props.sessionId;
		let active = true;
		onCleanup(() => {
			active = false;
		});
		setUsage(null);
		setLoading(true);
		void rpc<SessionUsage>("get_session_usage", { sessionId })
			.then((snapshot) => {
				if (active) setUsage(snapshot);
			})
			.catch(() => {
				if (active) setUsage(null);
			})
			.finally(() => {
				if (active) setLoading(false);
			});
	});
	return (
		<section class={styles.usageBlock} aria-label="Session usage" aria-live="polite">
			<Show when={!loading()} fallback={<div>Usage: loading…</div>}>
				<div>{usage()?.model ? `Model: ${usage()!.model}` : "Model unavailable"}</div>
				<Show
					when={usage()?.availability === "available"}
					fallback={
						<>
							<div>Usage not available</div>
							<Show when={usage()?.reason}>
								<div>{usage()!.reason}</div>
							</Show>
						</>
					}
				>
					<Show when={usage()?.stale}>
						<div>Last known usage (stale)</div>
					</Show>
					<For each={usage()?.lines}>
						{(line) => (
							<div>
								{line.label}: {line.value}
							</div>
						)}
					</For>
				</Show>
			</Show>
		</section>
	);
}
