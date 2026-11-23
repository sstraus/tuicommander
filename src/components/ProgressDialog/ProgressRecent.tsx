import { type Component, createResource, createSignal, For, Show } from "solid-js";
import { invoke } from "../../invoke";
import type { ProgressEntry, ProgressKind } from "../../stores/progress";
import { progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { formatRelativeTime } from "../../utils/time";
import { Select } from "../ui/Select";
import s from "./ProgressDialog.module.css";

type RecentEntry = ProgressEntry & { unread: boolean };

const KIND_LABEL: Partial<Record<ProgressKind, string>> = {
	done: "Done",
	blocked: "Blocked",
	question: "Question",
	answer: "Answer",
};

/** Every project's newest entries for the user; a click opens the project and terminal. */
export const ProgressRecent: Component<{ revision: unknown; onOpen?: () => void }> = (props) => {
	const [kind, setKind] = createSignal<ProgressKind | "">("");
	const [unreadOnly, setUnreadOnly] = createSignal(false);
	const [entries] = createResource(
		() => ({ kind: kind(), unreadOnly: unreadOnly(), revision: props.revision }),
		({ kind, unreadOnly }) =>
			invoke<RecentEntry[]>("progress_recent", {
				input: { ...(kind ? { kind } : {}), unreadOnly, limit: 100 },
			}),
	);
	/** Reading an errored resource rethrows, so every read goes through this guard. */
	const rows = () => (entries.error ? [] : (entries() ?? []));
	const projectName = (path: string) =>
		repositoriesStore.get(path)?.displayName ?? path.split(/[\\/]/).filter(Boolean).pop() ?? path;

	const openEntry = (entry: RecentEntry) => {
		progressStore.open(entry.project, entry.ptyId ?? null);
		props.onOpen?.();
		const terminalId = entry.ptyId ? terminalsStore.findBySessionId(entry.ptyId) : null;
		// Loaded on click: navigation pulls in the pane and tab stores.
		if (terminalId) void import("../../utils/navigateToTerminal").then((m) => m.navigateToTerminal(terminalId));
	};

	return (
		<div class={s.list}>
			<div class={s.recentFilters}>
				<Select
					aria-label="Kind"
					value={kind()}
					onChange={(event) => setKind(event.currentTarget.value as ProgressKind | "")}
				>
					<option value="">All kinds</option>
					<For each={Object.entries(KIND_LABEL)}>{([id, label]) => <option value={id}>{label}</option>}</For>
				</Select>
				<label class={s.filter}>
					<input type="checkbox" checked={unreadOnly()} onChange={(e) => setUnreadOnly(e.currentTarget.checked)} />
					Unread only
				</label>
			</div>
			<Show when={entries.error}>
				<p class={s.error}>{String(entries.error)}</p>
			</Show>
			<Show when={rows().length > 0} fallback={<p class={s.empty}>{entries.loading ? "Loading…" : "Nothing here."}</p>}>
				<For each={rows()}>
					{(entry) => (
						<button
							type="button"
							data-control-layout="row"
							class={s.recentRow}
							data-kind={entry.type}
							data-unread={entry.unread}
							onClick={() => openEntry(entry)}
						>
							<span class={s.kindLabel}>{KIND_LABEL[entry.type] ?? entry.type}</span>
							<span class={s.recentText}>{entry.text}</span>
							<span class={s.meta}>
								{projectName(entry.project)} · {formatRelativeTime(entry.createdAtMs, { showDateFallback: true })}
							</span>
						</button>
					)}
				</For>
			</Show>
		</div>
	);
};
