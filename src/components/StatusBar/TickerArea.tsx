import {
	type Component,
	createEffect,
	createMemo,
	createSignal,
	For,
	type JSX,
	on,
	onCleanup,
	onMount,
	Show,
} from "solid-js";
import { useOutsideDismiss } from "../../hooks/useOutsideDismiss";
import { statusBarTicker, type TickerMessage, URGENT_PRIORITY } from "../../stores/statusBarTicker";
import { terminalsStore } from "../../stores/terminals";
import { cx } from "../../utils/cx";
import { countSideBySide } from "../../utils/tickerLayout";
import { Button } from "../ui";
import s from "./StatusBar.module.css";

const MessageBody: Component<{
	msg: TickerMessage;
	class?: string;
	/** Probe copies are never clickable. */
	interactive?: boolean;
	onContextMenu?: (e: MouseEvent) => void;
	children?: JSX.Element;
}> = (props) => (
	<span
		class={cx(
			s.tickerMessage,
			props.class,
			props.msg.priority >= URGENT_PRIORITY && s.tickerWarning,
			props.msg.priority >= 80 && props.msg.priority < URGENT_PRIORITY && s.tickerAttention,
			props.interactive && props.msg.onClick && s.tickerClickable,
		)}
		title={props.interactive ? props.msg.text : undefined}
		onContextMenu={props.interactive ? props.onContextMenu : undefined}
		role={props.interactive && props.msg.onClick ? "button" : undefined}
		tabIndex={props.interactive && props.msg.onClick ? 0 : undefined}
		onClick={props.interactive ? () => props.msg.onClick?.() : undefined}
		onKeyDown={
			props.interactive && props.msg.onClick
				? (e) => {
						if (e.key === "Enter" || e.key === " ") {
							e.preventDefault();
							props.msg.onClick?.();
						}
					}
				: undefined
		}
	>
		<Show when={props.msg.icon}>
			<span class={s.tickerIcon} innerHTML={props.msg.icon!} />
		</Show>
		<Show when={props.msg.label}>
			<span class={s.tickerLabel}>{props.msg.label!}</span>
			<span class={s.tickerSep}>{" \u00B7 "}</span>
		</Show>
		<span class={s.tickerText}>{props.msg.text}</span>
		{props.children}
	</span>
);

/**
 * Shared ticker area in the status bar.
 * Displays plugin messages side by side, rotating overflow in the last slot, with source label,
 * counter badge, click-to-cycle, and right-click popover.
 */
export const TickerArea: Component = () => {
	const [popoverOpen, setPopoverOpen] = createSignal(false);
	let areaRef: HTMLSpanElement | undefined;
	let probeRef: HTMLSpanElement | undefined;
	let popoverRef: HTMLDivElement | undefined;

	// Close popover on Escape
	const handleKeyDown = (e: KeyboardEvent) => {
		if (e.key === "Escape" && popoverOpen()) {
			setPopoverOpen(false);
		}
	};

	// The ticker itself is inside: right-click on it toggles, so it must not count as an outside press.
	useOutsideDismiss({
		inside: () => [popoverRef, areaRef],
		onClose: () => setPopoverOpen(false),
		enabled: popoverOpen,
	});

	onMount(() => document.addEventListener("keydown", handleKeyDown));
	onCleanup(() => document.removeEventListener("keydown", handleKeyDown));

	// On agent tabs the shared agent-usage slot is absorbed into the agent badge (a stale result
	// for the other provider must stay hidden during the async switch), so it is excluded from
	// the pool itself rather than hidden after selection.
	const hidden = createMemo(() =>
		terminalsStore.getActive()?.agentType ? (m: TickerMessage) => m.pluginId === "claude-usage" : undefined,
	);
	const pool = createMemo(() => statusBarTicker.getPool(hidden()));

	// Layout: tickers sit side by side while the area has room; the ones that do not fit
	// rotate in the last slot. Natural widths come from a hidden probe row that renders the
	// whole pool, so the decision never depends on what is currently displayed.
	const [areaWidth, setAreaWidth] = createSignal(0);
	const [probeWidths, setProbeWidths] = createSignal<number[]>([]);
	const [gap, setGap] = createSignal(0);

	const measure = () => {
		if (!areaRef || !probeRef) return;
		setAreaWidth(areaRef.offsetWidth);
		setProbeWidths(Array.from(probeRef.children, (el) => (el as HTMLElement).offsetWidth));
		setGap(Number.parseFloat(getComputedStyle(areaRef).columnGap) || 0);
	};

	onMount(() => {
		measure();
		const observer = new ResizeObserver(measure);
		if (areaRef) observer.observe(areaRef);
		if (probeRef) observer.observe(probeRef);
		onCleanup(() => observer.disconnect());
	});
	// The probe row resizes when message text changes, but a swapped pool of equal total width
	// would not trigger the observer.
	createEffect(on(pool, () => measure(), { defer: true }));

	const pinnedCount = createMemo(() =>
		probeWidths().length === pool().length ? countSideBySide(probeWidths(), areaWidth(), gap()) : 0,
	);
	const pinned = () => pool().slice(0, pinnedCount());
	const rotation = () => {
		const state = statusBarTicker.getRotationState(hidden(), pinnedCount());
		return state.message ? state : null;
	};

	const allMessages = () => statusBarTicker.getActiveMessages();

	const handleBadgeClick = (e: MouseEvent) => {
		e.stopPropagation();
		statusBarTicker.advanceManually();
	};

	const handleContextMenu = (e: MouseEvent) => {
		e.preventDefault();
		setPopoverOpen((prev) => !prev);
	};

	const handlePopoverRowClick = (onClick?: () => void) => {
		onClick?.();
		setPopoverOpen(false);
	};

	return (
		<>
			<span
				ref={areaRef}
				class={cx(s.tickerArea, pool().length === 0 && s.tickerAreaEmpty)}
				style={{
					width: `${probeWidths().reduce((sum, width) => sum + width, 0) + Math.max(0, probeWidths().length - 1) * gap()}px`,
				}}
			>
				<For each={pinned()}>
					{(msg) => <MessageBody msg={msg} class={s.tickerPinned} interactive onContextMenu={handleContextMenu} />}
				</For>
				<Show when={rotation()}>
					{(state) => (
						<MessageBody msg={state().message!} interactive onContextMenu={handleContextMenu}>
							<Show when={state().total > 1}>
								<span class={s.tickerBadge} onClick={handleBadgeClick} title="Click to cycle tickers">
									{state().current}/{state().total} {"\u25B8"}
								</span>
							</Show>
						</MessageBody>
					)}
				</Show>
				<span ref={probeRef} class={s.tickerProbe} aria-hidden="true">
					<For each={pool()}>{(msg) => <MessageBody msg={msg} class={s.tickerPinned} />}</For>
				</span>
			</span>

			{/* Popover listing all active tickers */}
			<Show when={popoverOpen()}>
				<div ref={popoverRef} class={s.tickerPopover}>
					<div class={s.tickerPopoverHeader}>
						Active Tickers
						<Button size="compact" variant="ghost" onClick={() => setPopoverOpen(false)}>
							&times;
						</Button>
					</div>
					<div class={s.tickerPopoverList}>
						<For each={allMessages()} fallback={<div class={s.tickerPopoverEmpty}>No active tickers</div>}>
							{(msg) => (
								<div
									class={cx(
										s.tickerPopoverRow,
										msg.onClick && s.tickerPopoverRowClickable,
										msg.priority >= URGENT_PRIORITY && s.tickerPopoverRowUrgent,
									)}
									onClick={() => handlePopoverRowClick(msg.onClick)}
								>
									<Show when={msg.icon}>
										<span class={s.tickerIcon} innerHTML={msg.icon!} />
									</Show>
									<Show when={msg.label}>
										<span class={s.tickerPopoverLabel}>{msg.label}</span>
									</Show>
									<span class={s.tickerPopoverText}>{msg.text}</span>
									<Show when={msg.priority >= URGENT_PRIORITY}>
										<span class={s.tickerPopoverUrgentBadge}>URGENT</span>
									</Show>
								</div>
							)}
						</For>
					</div>
				</div>
			</Show>
		</>
	);
};
