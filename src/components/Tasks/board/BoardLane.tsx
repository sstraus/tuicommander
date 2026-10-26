import { createVirtualizer } from "@tanstack/solid-virtual";
import { type Component, For, type JSX, Show } from "solid-js";
import type { WorkItemBoardColumn } from "../../../types/work-items.gen";
import { Button } from "../../ui/Button";
import s from "./board.module.css";
import { CardMoveMenu } from "./CardMoveMenu";
import { type BoardCard, cardKey } from "./types";

interface BoardLaneProps {
	column: WorkItemBoardColumn;
	columns: WorkItemBoardColumn[];
	cards: readonly BoardCard[];
	target: boolean;
	hasMore?: boolean;
	loadingMore?: boolean;
	onMore?(): void;
	pending(key: string): boolean;
	canMove(card: BoardCard, column: string): boolean;
	onMove(card: BoardCard, column: string): void;
	onPointerDown(event: PointerEvent, card: BoardCard): void;
	onOpen?(card: BoardCard): void;
	cardActions?(card: BoardCard): JSX.Element;
}
export const BoardLane: Component<BoardLaneProps> = (props) => {
	let scroll: HTMLDivElement | undefined;
	const virtualizer = createVirtualizer({
		get count() {
			return props.cards.length;
		},
		getScrollElement: () => scroll ?? null,
		estimateSize: () => 196,
		overscan: 3,
		getItemKey: (index) => {
			const card = props.cards[index];
			return card ? cardKey(card.item.reference) : index;
		},
		initialRect: { width: 280, height: 600 },
	});
	return (
		<section
			role="region"
			aria-label={props.column.name}
			data-board-column={props.column.id}
			class={s.lane}
			classList={{ [s.dropTarget]: props.target }}
		>
			<header class={s.laneHeader}>
				<h3>{props.column.name}</h3>
				<span>{props.cards.length} loaded</span>
			</header>
			<div class={s.laneScroll} ref={scroll}>
				<Show when={props.cards.length === 0}>
					<p class={s.empty}>No items</p>
				</Show>
				<div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
					<For each={virtualizer.getVirtualItems()}>
						{(row) => {
							const card = () => props.cards[row.index];
							return (
								<Show when={card()}>
									{(entry) => (
										<article
											class={s.card}
											data-board-card={entry().item.key}
											aria-label={entry().item.key}
											aria-busy={props.pending(cardKey(entry().item.reference))}
											style={{ position: "absolute", top: `${row.start}px`, height: `${row.size}px`, width: "100%" }}
											onPointerDown={(event) => props.onPointerDown(event, entry())}
										>
											<div class={s.cardHead}>
												<Button variant="ghost" type="button" class={s.key} onClick={() => props.onOpen?.(entry())}>
													{entry().item.key}
												</Button>
												{props.cardActions?.(entry())}
											</div>
											<span class={s.title}>{entry().item.title}</span>
											<div class={s.metadata}>
												<For each={entry().item.assignees}>
													{(user) => (
														<span class={s.assignee} title={user.name}>
															{user.name}
														</span>
													)}
												</For>
												<For each={entry().item.labels}>
													{(label) => (
														<span class={s.label} title={label.name}>
															{label.name}
														</span>
													)}
												</For>
											</div>
											<Show
												when={props.pending(cardKey(entry().item.reference))}
												fallback={
													<CardMoveMenu
														card={entry()}
														columns={props.columns}
														columnId={props.column.id}
														canMove={props.canMove}
														onMove={props.onMove}
													/>
												}
											>
												<span class={s.pending}>Moving {entry().item.key}…</span>
											</Show>
										</article>
									)}
								</Show>
							);
						}}
					</For>
				</div>
			</div>
			<Show when={props.hasMore}>
				<Button
					type="button"
					class={s.more}
					aria-label={`More in ${props.column.name}`}
					title="Load the next page across all lanes"
					disabled={props.loadingMore}
					onClick={() => props.onMore?.()}
				>
					{props.loadingMore ? "Loading…" : "More"}
				</Button>
			</Show>
		</section>
	);
};
