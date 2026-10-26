import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { Portal } from "solid-js/web";
import type { WorkItemBoardColumn } from "../../../types/work-items.gen";
import { Button } from "../../ui/Button";
import s from "./board.module.css";
import type { BoardCard } from "./types";

/** A portalled menu stays reachable outside the virtualized card's clipped lane. */
export function CardMoveMenu(props: {
	card: BoardCard;
	columns: WorkItemBoardColumn[];
	columnId: string;
	canMove(card: BoardCard, column: string): boolean;
	onMove(card: BoardCard, column: string): void;
}) {
	const [position, setPosition] = createSignal<{ left: number; top: number }>();
	let trigger: HTMLButtonElement | undefined;
	let menu: HTMLDivElement | undefined;
	function close(restore = false) {
		setPosition(undefined);
		if (restore) trigger?.focus();
	}
	function outside(event: PointerEvent) {
		if (event.target instanceof Node && !menu?.contains(event.target) && !trigger?.contains(event.target)) close();
	}
	function dismiss(event: Event) {
		if (event.target instanceof Node && menu?.contains(event.target)) return;
		close();
	}
	function toggle() {
		if (position()) {
			close(true);
			return;
		}
		const rect = trigger?.getBoundingClientRect();
		if (!rect) return;
		setPosition({
			left: Math.max(8, Math.min(rect.left, window.innerWidth - 208)),
			top: Math.max(8, Math.min(rect.bottom + 4, window.innerHeight - 200)),
		});
	}
	createEffect(() => {
		if (!position()) return;
		const focusFrame = requestAnimationFrame(() =>
			menu?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus(),
		);
		document.addEventListener("pointerdown", outside);
		document.addEventListener("scroll", dismiss, true);
		onCleanup(() => {
			cancelAnimationFrame(focusFrame);
			document.removeEventListener("pointerdown", outside);
			document.removeEventListener("scroll", dismiss, true);
		});
	});
	return (
		<>
			<Button
				ref={trigger}
				type="button"
				class={s.moveTrigger}
				aria-label={`Move ${props.card.item.key}`}
				aria-haspopup="menu"
				aria-expanded={!!position()}
				onClick={toggle}
			>
				Move…
			</Button>
			<Show when={position()}>
				{(point) => (
					<Portal>
						<div
							ref={menu}
							role="menu"
							aria-label={`Move ${props.card.item.key} to`}
							class={s.moveMenu}
							style={{ left: `${point().left}px`, top: `${point().top}px` }}
							onKeyDown={(event) => {
								if (event.key === "Escape") {
									event.preventDefault();
									close(true);
								}
								if (event.key === "Tab") close();
								const buttons = [...(menu?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
								const index =
									document.activeElement instanceof HTMLButtonElement ? buttons.indexOf(document.activeElement) : -1;
								const next =
									event.key === "ArrowDown"
										? (index + 1) % buttons.length
										: event.key === "ArrowUp"
											? (index - 1 + buttons.length) % buttons.length
											: -1;
								if (next >= 0) {
									event.preventDefault();
									buttons[next]?.focus();
								}
							}}
						>
							<For each={props.columns}>
								{(column) => (
									<Show when={column.id !== props.columnId}>
										<Button
											type="button"
											role="menuitem"
											disabled={!props.canMove(props.card, column.id)}
											onClick={() => {
												close(true);
												props.onMove(props.card, column.id);
											}}
										>
											{column.name}
										</Button>
									</Show>
								)}
							</For>
						</div>
					</Portal>
				)}
			</Show>
		</>
	);
}
