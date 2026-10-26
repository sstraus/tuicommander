import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { rpc, subscribeEvents } from "../../transport";
import styles from "../screens/SessionDetailScreen.module.css";

export interface OpenMarkdownTab {
	id: string;
	title: string;
	relative_path: string;
	absolute_path: string;
	fs_root: string;
	opener_tuic_session?: string;
}

export function createOpenMarkdownTabs(getCwd: () => string | null) {
	const cwd = createMemo(getCwd);
	const [connected, setConnected] = createSignal(false);
	const [tabs, setTabs] = createSignal<OpenMarkdownTab[]>([]);
	const [error, setError] = createSignal("");
	const [loading, setLoading] = createSignal(true);
	let request = 0;
	let disposed = false;
	async function refresh(path = cwd()) {
		const current = ++request;
		if (!path) {
			setLoading(false);
			setError("Repository path is unavailable.");
			return;
		}
		try {
			const result = await rpc<OpenMarkdownTab[]>("list_open_markdown_tabs", { cwd: path });
			if (disposed || current !== request) return;
			if (!Array.isArray(result)) throw new Error("Invalid Markdown tab list");
			setTabs(result);
			setError("");
		} catch {
			if (disposed || current !== request) return;
			setTabs([]);
			setError("Could not load open Markdown tabs.");
		} finally {
			if (!disposed && current === request) setLoading(false);
		}
	}
	createEffect(() => {
		const path = cwd();
		if (connected()) {
			void refresh(path);
		}
	});
	onMount(() => {
		// Start listening before reading; onResync rereads when SSE first becomes live.
		const subscription = subscribeEvents(
			{ "open-markdown-tabs-changed": () => void refresh(), "repositories-changed": () => void refresh() },
			{ onResync: () => void refresh() },
		);
		subscription
			.then(() => {
				if (!disposed) setConnected(true);
			})
			.catch(() => {
				if (!disposed) {
					setLoading(false);
					setError("Could not follow open Markdown tabs.");
				}
			});
		onCleanup(() => {
			disposed = true;
			subscription.then((unsubscribe) => unsubscribe()).catch(() => {});
		});
	});
	return { tabs, error, loading };
}

export function OpenMarkdownMenu(props: {
	cwd: string | null;
	source?: ReturnType<typeof createOpenMarkdownTabs>;
	onOpen: (tab: OpenMarkdownTab) => void;
}) {
	const { tabs, error, loading } = props.source ?? createOpenMarkdownTabs(() => props.cwd);

	return (
		<section role="region" aria-label="Markdown" class={styles.markdownSection}>
			<div class={styles.markdownHeading}>Markdown</div>
			<Show when={!loading()} fallback={<div class={styles.markdownStatus}>Loading…</div>}>
				<Show
					when={!error()}
					fallback={
						<div class={styles.markdownStatus} role="alert">
							{error()}
						</div>
					}
				>
					<For each={tabs()} fallback={<div class={styles.markdownStatus}>No open Markdown tabs</div>}>
						{(tab) => (
							<button
								data-control-layout="row"
								type="button"
								class={styles.markdownEntry}
								title={tab.relative_path}
								aria-label={`${tab.title} ${tab.relative_path}`}
								onClick={() => props.onOpen(tab)}
							>
								<span>{tab.title}</span>
								<span class={styles.markdownPath}>{tab.relative_path}</span>
							</button>
						)}
					</For>
				</Show>
			</Show>
		</section>
	);
}
