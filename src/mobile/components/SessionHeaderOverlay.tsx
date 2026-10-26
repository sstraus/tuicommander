import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Button } from "../../components/ui";
import { appLogger } from "../../stores/appLogger";
import type { ProgressEntry, ProgressList } from "../../stores/progress";
import { rpc } from "../../transport";
import { displayTask } from "../../utils/displayTask";
import type { SessionInfo, SessionState } from "../useSessions";
import styles from "./SessionHeaderOverlay.module.css";

export type SessionHeaderPanel = "details" | "tasks" | "progress";

interface SessionHeaderOverlayProps {
	mode: SessionHeaderPanel;
	session: SessionInfo;
	state?: SessionState;
	onClose: () => void;
}

function withinRoot(path: string, root: string): boolean {
	const file = path.replaceAll("\\", "/").replace(/\/+$/, "");
	const directory = root.replaceAll("\\", "/").replace(/\/+$/, "");
	return file === directory || file.startsWith(`${directory}/`);
}

export function SessionHeaderOverlay(props: SessionHeaderOverlayProps) {
	const [entries, setEntries] = createSignal<ProgressEntry[]>([]);
	const [loading, setLoading] = createSignal(false);
	const [error, setError] = createSignal("");
	let cancelled = false;
	let project: string | null = null;

	async function projectForSession(): Promise<string | null> {
		if (props.session.worktree_path) return props.session.worktree_path;
		const cwd = props.session.cwd;
		if (!cwd) return null;
		const config = await rpc<{ repos?: Record<string, unknown> }>("load_repositories");
		return (
			Object.keys(config.repos ?? {})
				.filter((path) => withinRoot(cwd, path))
				.sort((a, b) => b.length - a.length)[0] ?? null
		);
	}

	onMount(async () => {
		if (props.mode === "details") return;
		setLoading(true);
		try {
			project = await projectForSession();
			if (!project) {
				setError("No registered repository contains this session.");
				return;
			}
			const id = props.session.session_id;
			const all: ProgressEntry[] = [];
			let cursor: number | null = null;
			const seen = new Set<number>();
			while (!cancelled) {
				const input: { ptyId: string; limit: number; cursor?: number } = {
					ptyId: id,
					limit: 100,
					...(cursor == null ? {} : { cursor }),
				};
				const page: ProgressList = await rpc<ProgressList>("progress_list", { project, input });
				if (cancelled || id !== props.session.session_id) return;
				all.push(...page.entries);
				if (page.nextCursor == null || seen.has(page.nextCursor)) break;
				seen.add(page.nextCursor);
				cursor = page.nextCursor;
			}
			if (!cancelled) setEntries(all);
		} catch (cause) {
			if (!cancelled) setError(`Progress is unavailable: ${String(cause)}`);
		} finally {
			if (!cancelled) setLoading(false);
		}
	});
	onCleanup(() => {
		cancelled = true;
	});

	function close() {
		if (props.mode === "progress" && project) {
			void rpc("progress_mark_viewed", { project, ptyId: props.session.session_id }).catch((cause) => {
				appLogger.warn("network", `Failed to mark session progress viewed: ${String(cause)}`);
			});
		}
		props.onClose();
	}

	const title = () =>
		props.mode === "details" ? "Session details" : props.mode === "tasks" ? "Session tasks" : "Session progress";
	const task = () => displayTask(props.state?.current_task, props.state?.agent_type);
	const shownEntries = () =>
		props.mode === "tasks" ? entries().filter((entry) => entry.type === "intent") : entries();

	return (
		<div class={styles.backdrop} onClick={(event) => event.target === event.currentTarget && close()}>
			<section class={styles.sheet} role="dialog" aria-label={title()}>
				<header class={styles.header}>
					<strong>{title()}</strong>
					<Button
						variant="ghost"
						geometry="stretch"
						class={styles.close}
						aria-label="Close session details"
						onClick={close}
					>
						×
					</Button>
				</header>
				<Show when={props.mode === "details" || props.mode === "tasks"}>
					<Show when={props.state?.agent_intent}>
						<p>
							<b>Intent</b>
							<br />
							{props.state?.agent_intent}
						</p>
					</Show>
					<Show when={task()}>
						<p>
							<b>Current task</b>
							<br />
							{task()}
						</p>
					</Show>
					<Show when={(props.state?.active_sub_tasks ?? 0) > 0}>
						<p>{props.state?.active_sub_tasks} sub-tasks running</p>
					</Show>
				</Show>
				<Show when={props.mode !== "details"}>
					<Show when={loading()}>
						<p>Loading…</p>
					</Show>
					<Show when={error()}>
						<p role="alert">{error()}</p>
					</Show>
					<Show when={!loading() && !error() && shownEntries().length === 0}>
						<p>No {props.mode === "tasks" ? "task history" : "progress"} recorded for this session.</p>
					</Show>
					<For each={shownEntries()}>
						{(entry) => (
							<article class={styles.entry}>
								<small>
									{entry.type}
									{entry.step ? ` · ${entry.step}` : ""}
								</small>
								<p>{entry.text}</p>
							</article>
						)}
					</For>
				</Show>
			</section>
		</div>
	);
}
