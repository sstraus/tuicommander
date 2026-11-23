import { createEffect, createSignal, For, onCleanup, Show, untrack } from "solid-js";
import type { WorkItemProvider } from "../../plugins/workItemProvider";
import type { WorkItemBoard, WorkItemRef } from "../../types/work-items.gen";
import { GatewayNotice } from "../ui/GatewayNotice";
import { Select } from "../ui/Select";
import { createBoardPages } from "./board/createBoardPages";
import { TasksBoard } from "./board/TasksBoard";
import { StartWorkMenu } from "./StartWorkMenu";
import s from "./TasksShell.module.css";

/** Loads one provider board and feeds it to the host board; moves go back through the provider. */
export function TasksBoardView(props: {
	provider: WorkItemProvider;
	visible?: boolean;
	/** Repository that Start work creates its worktree in. */
	repoPath?: string;
	onOpen: (reference: WorkItemRef) => void;
}) {
	const [boardId, setBoardId] = createSignal("");
	const pages = createBoardPages();
	const [boards, setBoards] = createSignal<readonly WorkItemBoard[]>([]);
	const [discoveryError, setDiscoveryError] = createSignal<string | null>(null);
	const [discovering, setDiscovering] = createSignal(false);
	let discovery: AbortController | undefined;
	createEffect(() => {
		const provider = props.provider;
		if (props.visible === false) return;
		discovery?.abort();
		const controller = new AbortController();
		discovery = controller;
		setBoards(provider.boards);
		setDiscoveryError(null);
		setDiscovering(!!provider.listBoards);
		if (provider.listBoards)
			void provider
				.listBoards(controller.signal)
				.then((next) => {
					if (!controller.signal.aborted) setBoards(next);
				})
				.catch((error) => {
					if (!controller.signal.aborted) setDiscoveryError(error instanceof Error ? error.message : String(error));
				})
				.finally(() => {
					if (!controller.signal.aborted) setDiscovering(false);
				});
	});
	onCleanup(() => discovery?.abort());
	const selected = () => boards().find((board) => board.id === boardId()) ?? boards()[0];
	createEffect(() => {
		const provider = props.provider;
		const id = selected()?.id ?? "";
		if (props.visible === false) {
			pages.cancel();
			return;
		}
		untrack(() => pages.select(provider, id));
	});
	onCleanup(pages.cancel);
	return (
		<section class={s.view} aria-label="Tasks board">
			<Show when={boards().length > 1}>
				<label class={s.boardPicker}>
					Board
					<Select value={selected()?.id} onChange={(event) => setBoardId(event.currentTarget.value)}>
						<For each={boards()}>{(board) => <option value={board.id}>{board.name}</option>}</For>
					</Select>
				</label>
			</Show>
			<Show when={discovering()}>
				<p role="status">Loading boards…</p>
			</Show>
			<Show when={discoveryError()}>
				<p class={s.error} role="alert">
					{discoveryError()}
				</p>
			</Show>
			<GatewayNotice limit={pages.limit()} stale={!!pages.error() && !!pages.page()} cachedAt={pages.cachedAt()} />
			<Show when={pages.error() && !pages.limit()}>
				<p class={s.error} role="alert">
					{pages.error()}
				</p>
			</Show>
			<Show when={!selected() && !discovering() && !discoveryError()}>
				<p class={s.empty}>This provider has no boards.</p>
			</Show>
			<Show when={pages.page()}>
				{(snapshot) => (
					<>
						<Show when={snapshot().board.writeDisabledReason}>
							<p role="status">{snapshot().board.writeDisabledReason}</p>
						</Show>
						<TasksBoard
							scopeKey={JSON.stringify([props.provider.providerId, props.provider.accountId])}
							board={snapshot().board}
							lanes={snapshot().lanes}
							onMove={async (reference, action) => {
								try {
									return await props.provider.act(reference, action);
								} catch (error) {
									// Re-read provider permissions after a rejected write; retain the move error.
									void pages.refresh();
									throw error;
								}
							}}
							onUpdated={() => void pages.refresh()}
							hasMore={!!snapshot().nextCursor}
							loadingMore={pages.loading()}
							onMore={() => void pages.more()}
							onOpen={props.onOpen}
							cardActions={(card) => (
								<StartWorkMenu provider={props.provider} item={card.item} repoPath={props.repoPath} />
							)}
						/>
					</>
				)}
			</Show>
		</section>
	);
}
