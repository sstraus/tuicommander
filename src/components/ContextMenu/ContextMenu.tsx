import { type Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { cx } from "../../utils";
import { Button } from "../ui/Button";
import s from "./ContextMenu.module.css";

export interface ContextMenuItem {
	label: string;
	shortcut?: string;
	action: () => void;
	separator?: boolean;
	disabled?: boolean;
	children?: ContextMenuItem[];
	/** Native tooltip shown on hover — e.g. the full path behind a "Copy Path" item */
	title?: string;
}

export interface ContextMenuProps {
	items: ContextMenuItem[];
	x: number;
	y: number;
	visible: boolean;
	onClose: () => void;
}

/** True when a keyboard event matches a menu item's shortcut hint (e.g. "⇧M",
 *  "↵", "r"). Modifier glyphs: ⇧ shift, ⌘ meta, ⌃ ctrl, ⌥ alt. */
function matchesShortcut(e: KeyboardEvent, shortcut: string): boolean {
	const wantShift = shortcut.includes("⇧");
	const wantMeta = shortcut.includes("⌘");
	const wantCtrl = shortcut.includes("⌃");
	const wantAlt = shortcut.includes("⌥");
	const key = shortcut.replace(/[⇧⌘⌃⌥]/g, "");
	if (e.shiftKey !== wantShift || e.metaKey !== wantMeta || e.ctrlKey !== wantCtrl || e.altKey !== wantAlt) {
		return false;
	}
	if (key === "⏎") return e.key === "Enter"; // ↵
	if (key.length !== 1) return false;
	return e.key.toLowerCase() === key.toLowerCase();
}

const VIEWPORT_MARGIN = 8;

const viewportLimit = (axis: "width" | "height") => {
	const size = axis === "width" ? window.innerWidth : window.innerHeight;
	return Math.max(0, size - VIEWPORT_MARGIN * 2);
};

const constrainPopupSize = (el: HTMLDivElement) => {
	el.style.maxWidth = `${viewportLimit("width")}px`;
	el.style.maxHeight = `${viewportLimit("height")}px`;
};

const clampAxis = (value: number, extent: number, viewportExtent: number) => {
	const max = Math.max(VIEWPORT_MARGIN, viewportExtent - extent - VIEWPORT_MARGIN);
	return Math.max(VIEWPORT_MARGIN, Math.min(value, max));
};

/** Clamp a submenu position so it stays within the viewport (8px margin). */
const clampSubmenu = (wrapEl: HTMLDivElement, submenuEl: HTMLDivElement) => {
	constrainPopupSize(submenuEl);
	const parentRect = wrapEl.getBoundingClientRect();
	const subRect = submenuEl.getBoundingClientRect();
	const vw = window.innerWidth;
	const vh = window.innerHeight;
	const subWidth = Math.min(submenuEl.offsetWidth || subRect.width || 160, viewportLimit("width"));
	const subHeight = Math.min(submenuEl.offsetHeight || subRect.height || 36, viewportLimit("height"));

	// Horizontal: prefer right of parent, flip left if needed, clamp to viewport
	let left = parentRect.right;
	if (left + subWidth > vw - VIEWPORT_MARGIN) {
		left = parentRect.left - subWidth;
	}
	left = clampAxis(left, subWidth, vw);

	// Vertical: align top with parent item, clamp to viewport
	let top = parentRect.top;
	if (top + subHeight > vh - VIEWPORT_MARGIN) {
		top = vh - subHeight - VIEWPORT_MARGIN;
	}
	top = clampAxis(top, subHeight, vh);

	submenuEl.style.left = `${left}px`;
	submenuEl.style.top = `${top}px`;
};

/** Single menu item — handles both leaf items and items with submenus */
const MenuItem: Component<{
	item: ContextMenuItem;
	onClose: () => void;
	isLast?: boolean;
}> = (props) => {
	let wrapRef: HTMLDivElement | undefined;
	let submenuRef: HTMLDivElement | undefined;
	const [submenuOpen, setSubmenuOpen] = createSignal(false);
	const hasChildren = () => !!(props.item.children && props.item.children.length > 0);

	const openSubmenu = () => {
		if (props.item.disabled || !hasChildren()) return;
		setSubmenuOpen(true);
		// Position after render
		requestAnimationFrame(() => {
			if (wrapRef && submenuRef) clampSubmenu(wrapRef, submenuRef);
		});
	};

	// `separator` on an EMPTY-label item is a standalone divider row (no button).
	// `separator` on a REAL item (non-empty label) is a trailing-divider modifier:
	// render the item, then a divider after it. FileBrowser/App menus use the
	// modifier form — treating separator as exclusive silently drops those items
	// (e.g. Delete). Disambiguate on the label being empty.
	//
	// A trailing separator (pure divider row OR the modifier's trailing divider)
	// is suppressed on the LAST item — a divider with nothing after it is noise.
	const isPureSeparator = () => !!props.item.separator && props.item.label === "";

	return (
		<Show
			when={!isPureSeparator()}
			fallback={
				<Show when={!props.isLast}>
					<div class={s.separator} />
				</Show>
			}
		>
			<div ref={wrapRef} class={s.itemWrap} onMouseEnter={openSubmenu} onMouseLeave={() => setSubmenuOpen(false)}>
				<Button
					variant="ghost"
					class={cx(s.item, props.item.disabled && s.disabled)}
					title={props.item.title}
					onClick={() => {
						if (props.item.disabled) return;
						if (hasChildren()) {
							if (submenuOpen()) {
								setSubmenuOpen(false);
							} else {
								openSubmenu();
							}
							return;
						}
						props.item.action();
						props.onClose();
					}}
					disabled={props.item.disabled}
				>
					<span class={s.label}>{props.item.label}</span>
					<Show when={props.item.shortcut}>
						<span class={s.shortcut}>{props.item.shortcut}</span>
					</Show>
					<Show when={hasChildren()}>
						<span class={s.arrow}>{"\u203A"}</span>
					</Show>
				</Button>
				<Show when={submenuOpen() && props.item.children}>
					<div ref={submenuRef} class={s.submenu}>
						<For each={props.item.children}>
							{(child, i) => (
								<MenuItem
									item={child}
									onClose={props.onClose}
									isLast={i() === (props.item.children?.length ?? 0) - 1}
								/>
							)}
						</For>
					</div>
				</Show>
			</div>
			<Show when={props.item.separator && !props.isLast}>
				<div class={s.separator} />
			</Show>
		</Show>
	);
};

export const ContextMenu: Component<ContextMenuProps> = (props) => {
	let menuRef: HTMLDivElement | undefined;

	// Close on escape key
	createEffect(() => {
		if (!props.visible) return;

		const handleKeydown = (e: KeyboardEvent) => {
			// Modifier-only keys (Shift, Control, etc.) are ignored so chords can form.
			if (e.key === "Shift" || e.key === "Control" || e.key === "Alt" || e.key === "Meta") return;
			if (e.key === "Escape") {
				e.preventDefault();
				props.onClose();
				return;
			}
			// Trigger a matching item's action while the menu is open (the shortcut
			// hints shown next to each item). Top-level items only.
			const hit = props.items.find(
				(it) => !it.separator && !it.disabled && it.shortcut && matchesShortcut(e, it.shortcut),
			);
			if (hit) {
				e.preventDefault();
				props.onClose();
				hit.action();
				return;
			}
			// Any other key closes the menu (existing behavior).
			props.onClose();
		};

		const handleClickOutside = (e: MouseEvent) => {
			if (menuRef && !menuRef.contains(e.target as Node)) {
				props.onClose();
			}
		};

		document.addEventListener("keydown", handleKeydown);
		document.addEventListener("mousedown", handleClickOutside);

		onCleanup(() => {
			document.removeEventListener("keydown", handleKeydown);
			document.removeEventListener("mousedown", handleClickOutside);
		});
	});

	// Reposition menu after render to use measured dimensions
	const clampToViewport = (anchorX: number, anchorY: number, itemCount: number) => {
		if (!menuRef) return;
		constrainPopupSize(menuRef);
		const rect = menuRef.getBoundingClientRect();
		// Layout dimensions ignore the opening scale animation; rects do not.
		// Fallback estimates support environments without layout (e.g. jsdom).
		const menuWidth = Math.min(menuRef.offsetWidth || rect.width || 180, viewportLimit("width"));
		const menuHeight = Math.min(menuRef.offsetHeight || rect.height || itemCount * 36 + 8, viewportLimit("height"));
		const vw = window.innerWidth;
		const vh = window.innerHeight;

		let x = anchorX;
		let y = anchorY;

		// Horizontal: flip left if overflows right
		if (x + menuWidth > vw - VIEWPORT_MARGIN) {
			x = vw - menuWidth - VIEWPORT_MARGIN;
		}
		x = clampAxis(x, menuWidth, vw);

		// Vertical: if menu doesn't fit below click point, grow upward
		if (y + menuHeight > vh - VIEWPORT_MARGIN) {
			y = anchorY - menuHeight;
		}
		y = clampAxis(y, menuHeight, vh);

		menuRef.style.left = `${x}px`;
		menuRef.style.top = `${y}px`;
		menuRef.style.opacity = "1";
	};

	createEffect(() => {
		if (!props.visible || !menuRef) return;
		// Read reactive inputs here, not in the frame callback: another right click
		// can move an already-visible menu without changing `visible`.
		const x = props.x;
		const y = props.y;
		const itemCount = props.items.length;
		const raf = requestAnimationFrame(() => clampToViewport(x, y, itemCount));
		onCleanup(() => cancelAnimationFrame(raf));
	});

	return (
		<Show when={props.visible}>
			<div
				ref={menuRef}
				class={s.menu}
				onClick={(e) => e.stopPropagation()}
				style={{
					left: `${props.x}px`,
					top: `${props.y}px`,
					opacity: "0",
				}}
			>
				<For each={props.items}>
					{(item, i) => <MenuItem item={item} onClose={props.onClose} isLast={i() === props.items.length - 1} />}
				</For>
			</div>
		</Show>
	);
};

/** Hook to manage context menu state */
export function createContextMenu() {
	const [visible, setVisible] = createSignal(false);
	const [position, setPosition] = createSignal({ x: 0, y: 0 });
	let previousFocus: HTMLElement | null = null;

	const open = (e: MouseEvent) => {
		e.preventDefault();
		previousFocus = document.activeElement as HTMLElement | null;
		setPosition({ x: e.clientX, y: e.clientY });
		setVisible(true);
	};

	/** Open the menu at specific coordinates (for programmatic positioning) */
	const openAt = (x: number, y: number) => {
		previousFocus = document.activeElement as HTMLElement | null;
		setPosition({ x, y });
		setVisible(true);
	};

	let closeRaf = 0;
	const close = () => {
		setVisible(false);
		cancelAnimationFrame(closeRaf);
		if (!previousFocus) return;
		// Snapshot: `previousFocus` is cleared synchronously below, so the frame
		// callback must not read the mutable binding (it would always see null).
		const restore = previousFocus;
		previousFocus = null;
		closeRaf = requestAnimationFrame(() => {
			// Skip when something else already claimed focus — a menu item may have
			// opened a dialog whose input focused itself before this frame ran.
			const active = document.activeElement;
			if (!active || active === document.body) restore.focus();
		});
	};

	onCleanup(() => cancelAnimationFrame(closeRaf));

	return {
		visible,
		position,
		open,
		openAt,
		close,
	};
}

export default ContextMenu;
