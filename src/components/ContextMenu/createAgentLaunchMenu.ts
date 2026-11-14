import { createSignal, onCleanup } from "solid-js";
import type { AgentLaunchMenuItem } from "./AgentLaunchMenu";
import { createContextMenu } from "./ContextMenu";

export const AGENT_LAUNCH_LONG_PRESS_MS = 500;

/** The agent builder nests every agent under one "Add Agent" entry; under a + the list is the menu. */
function flattenAgentItems(items: AgentLaunchMenuItem[]): AgentLaunchMenuItem[] {
	return items.length === 1 && items[0].children ? items[0].children : items;
}

/**
 * Long press or right click on a "+" button lists the agents, so a new
 * terminal can open straight into one instead of a shell. Callers can also use
 * openBelow for a primary click; consumeClick suppresses the long-press release.
 */
export function createAgentLaunchMenu(getItems: () => AgentLaunchMenuItem[], options: { rightClick?: boolean } = {}) {
	const rightClick = options.rightClick ?? true;
	const menu = createContextMenu();
	const [items, setItems] = createSignal<AgentLaunchMenuItem[]>([]);
	let pressTimer: ReturnType<typeof setTimeout> | undefined;
	let pressFired = false;

	const openBelow = (btn: HTMLElement): boolean => {
		const list = flattenAgentItems(getItems());
		if (list.length === 0) return false;
		setItems(list);
		const rect = btn.getBoundingClientRect();
		menu.openAt(rect.left, rect.bottom + 4);
		return true;
	};

	const cancelPress = () => {
		clearTimeout(pressTimer);
		pressTimer = undefined;
	};

	// A touch release is followed by emulated mouse events; the menu closes on any
	// mousedown outside it, so the mousedown that ends the press that opened it
	// would shut the list the instant the finger lifts. Browsers that send none
	// (iOS Safari on a long press) leave the one-shot listener to expire.
	const swallowReleaseMouseDown = () => {
		const stop = (e: Event) => e.stopPropagation();
		document.addEventListener("mousedown", stop, { capture: true, once: true });
		setTimeout(() => document.removeEventListener("mousedown", stop, { capture: true }), 500);
	};

	const onPointerUp = (e: PointerEvent) => {
		cancelPress();
		if (pressFired && (e.pointerType === "touch" || e.pointerType === "pen")) swallowReleaseMouseDown();
	};

	const onPointerDown = (e: PointerEvent) => {
		pressFired = false;
		if (e.button !== 0) return;
		const btn = e.currentTarget as HTMLElement;
		pressTimer = setTimeout(() => {
			pressTimer = undefined;
			pressFired = openBelow(btn);
		}, AGENT_LAUNCH_LONG_PRESS_MS);
	};

	const onContextMenu = (e: MouseEvent) => {
		// Without the right-click list, a plain right click belongs to the caller's
		// own menu: only a pending or fired long press claims the event.
		if (!rightClick && (e.button === 2 || e.ctrlKey)) {
			// A mouse right click (button 2) or a macOS Ctrl+click (WebKit reports it
			// as button 0, on mousedown) is not a long press, even while the timer is
			// pending: it cancels the press, closes an open list and leaves the event
			// to the caller's menu.
			cancelPress();
			if (pressFired) close();
			return;
		}
		if (!rightClick && pressTimer === undefined && !pressFired) return;
		e.preventDefault();
		const btn = e.currentTarget as HTMLElement;
		let opened: boolean;
		if (pressTimer !== undefined) {
			// A touch long press also fires the native contextmenu, and it can beat
			// the timer. Open now: the pointerup that follows would cancel the timer.
			cancelPress();
			opened = pressFired = openBelow(btn);
		} else {
			// A right click never starts the timer (button 0 only); a fired press
			// means the long press already opened the list.
			opened = pressFired || openBelow(btn);
		}
		if (opened) e.stopPropagation();
	};

	/** True when the click ends a press that opened the menu and must be swallowed. */
	const consumeClick = (): boolean => {
		if (!pressFired) return false;
		pressFired = false;
		return true;
	};

	const close = () => {
		// A release off the button fires no click, so the flag would otherwise
		// swallow the next keyboard activation of +.
		pressFired = false;
		menu.close();
	};

	onCleanup(cancelPress);

	return {
		items,
		openBelow,
		position: menu.position,
		visible: menu.visible,
		close,
		consumeClick,
		buttonHandlers: {
			onPointerDown,
			onPointerUp,
			onPointerLeave: cancelPress,
			onPointerCancel: cancelPress,
			onContextMenu,
		},
	};
}
